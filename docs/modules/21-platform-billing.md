# 21 — Platform owner, tenant monitoring & billing (roadmap 34–46)

Platform-owner functionality is separate from tenant administration. Only accounts listed in
`PLATFORM_ADMIN_EMAILS` (the platform owner, `jamosammy@gmail.com` in production) reach `/api/platform/*`; every
endpoint checks it with `require_platform_admin`. A business administrator — even with every permission (`*`) — gets
403 there, and a business's own `/api/billing/*` endpoints only ever read that business's rows. The smoke suite
tests both.

## Where it lives

| Who | Screen | API |
|---|---|---|
| Platform owner | Settings → Platform → **Businesses** (directory) → business detail | `GET /platform/tenants`, `GET /platform/tenants/{id}` |
| Platform owner | Settings → Platform → **Platform billing** (dashboard, vendor bank details) | `GET /platform/billing`, `GET/PUT /platform/billing/vendor` |
| Platform owner | Settings → Platform → **Activity** | `GET /platform/activity` |
| Business | Settings → Business → **Billing** (permission `settings.billing`) | `GET /billing` … |

## 34 — Tenant directory

Per business: name, slug, status (+ reason, date), activation date, the first active administrator's name, email and
phone, business contacts, users, branches (with location and coordinates), sales and last sale, last sign-in,
onboarding details from the access request (contact, business type, branches requested, message, requested and
approved dates, who approved) and the billing position (below).

## 35 — Activity monitoring

Built on the existing audit trail (no parallel log). Activities: sign-ins, **failed sign-ins** (now audited as
`auth.login_failed` with the attempt number and whether the account locked), sales (incl. offline sync and
exchanges), stock counts/adjustments, stock received, transfers, PIN resets, platform actions and billing. Filters:
business, branch, user, activity, date / date range (platform calendar, Africa/Nairobi), with totals per activity
and paging.

**Platform PIN reset**: `POST /platform/tenants/{id}/users/{user_id}/reset-pin` returns a one-time PIN (shown once)
and clears the lock. Recorded in that business's audit trail and the platform owner's own.

## 36 — Activate / deactivate / reactivate

`POST /platform/tenants/{id}/status {status, reason}` — a reason is required to deactivate. Deactivation:

* blocks sign-in (422 *Business deactivated* with the support numbers),
* ends every session: tokens issued before `tenants.sessions_valid_after` are refused, also after reactivation,
* switches off the ordering link (*Ordering unavailable*),
* therefore blocks new transactions — nothing is deleted.

The platform owner can still open a deactivated business to look (GET only; changes are refused). The platform
owner's own business (any business with a platform administrator) can never be deactivated. Every change is
audited in the business and at home; the detail page shows the status history.

## 37 — Billing models & documents

One plan per business (`billing_plans`):

* **Subscription** — amount, frequency (monthly, quarterly, semi-annual, annual, custom 1–60 months), start date, next
  due date, grace period (0–90 days), auto-renew.
* **One-off** — one-off amount; optional **maintenance fee** with its own amount, frequency, start and next due date.

Documents (`billing_documents`): **quotations** (`QUO-YYYY-NNNNN`) and **invoices** (`INV-YYYY-NNNNN`) with
description, amount, issue date, due date, billing period and status. A quotation becomes one invoice (accepted by
the business or invoiced by the platform owner). One live invoice per billing period is enforced by a unique index,
so auto-renewal can never bill a period twice. Open documents can be voided with a reason (kept in the history).

**Auto-renew** (background job, every 15 minutes): the invoice for the next period is issued 7 days before it starts.

**Billing status** (shared by every screen): `not_set`, `paid`, `pending`, `due_soon` (open invoice not yet due, or
next due within 7 days), `grace` (past due, within the grace period), `overdue` (past due + grace).

## 38 — Paystack

* Secrets only in environment variables: `PAYSTACK_SECRET_KEY` (never sent to the browser, never logged — the
  config's debug output redacts it). `PAYSTACK_BASE_URL` exists only so CI can point at a local stand-in.
* *Pay now* (`POST /billing/invoices/{id}/pay`) creates a pending payment with a server reference (`SSB-…`) for exactly
  that invoice's amount and currency and returns Paystack's checkout URL. An earlier pending checkout for the same
  invoice is verified first, so a payment that went through is never paid twice.
* Back on Settings → Billing (`?reference=`) the browser asks the server to verify; the server calls Paystack's
  `transaction/verify`. The return page is never proof.
* Webhook `POST /api/webhooks/paystack`: the `x-paystack-signature` HMAC-SHA512 of the raw body must match (else 401);
  even then the payment is re-verified with Paystack before it is settled.
* Reconciliation: pending payments older than 10 minutes are re-verified by the background job; checkouts still
  open after 24 hours are marked abandoned (and still settle if Paystack later reports success).
* Settlement (`billing::settle`, one implementation for Paystack and payments recorded by the platform owner):
  payment → success with a receipt number (`RCT-YYYY-NNNNN`), invoice → paid, recurring period → next due date moved
  past the period. Idempotent. Amount or currency mismatch → payment failed, invoice stays open. A second successful
  payment for an already-paid (or voided) invoice is kept and flagged under *Needs attention* for a refund or credit.

Set up in Paystack: callback URL is automatic (`{PUBLIC_URL}/settings/billing`); set the webhook URL to
`{PUBLIC_URL}/api/webhooks/paystack`.

## 39 — Tenant billing portal

Settings → Billing shows the model, amount and frequency, status, last payment, period covered, next due date,
outstanding amount, invoices to pay (*Pay now*, PDF), quotations (*Accept*, PDF), receipts (PDF) and billing history.
One-off businesses see *One-off payment: Paid/Pending* and *Maintenance fee: amount + next due date*. Vendor bank
details (default *I&M Bank, Account •••450*) are edited by the platform owner and shown masked to businesses.
Invoices, quotations and receipts are A4 PDFs built in the browser.

## 40 — Platform billing dashboard

Active, deactivated, paid up, due soon, in grace, overdue, payment pending, no plan; subscription revenue (month,
year, monthly recurring), one-off revenue, maintenance due in the next 30 days, total outstanding, payments needing
attention, and every business filterable by status with drill-down to its detail page. The demo business is
excluded from the figures.

## Data model (migration 0014)

`tenants.status / status_reason / status_changed_at / activated_at / sessions_valid_after`, `platform_settings`
(vendor details), `billing_plans`, `billing_documents`, `billing_payments`, sequences for quotation, invoice and
receipt numbers, and an audit index on `(module, action, created_at)` for activity monitoring.

## 41 — Packages and modules

A plan's package is **Full platform** (default) or **Selected modules**. Modules (`billing::MODULES`):

| Module | API it owns | Permissions it owns |
|---|---|---|
| Sales / POS | `/sales`, `/mpesa` | `sales.*` |
| Orders | `/orders` (+ the ordering link) | `orders.*` |
| Stock & Inventory | `/stock`, `/transfers` | `stock.*` |
| Customers | `/customers` | `customers.view/create/edit` |
| Loyalty | `/loyalty` (+ points redemption in a sale) | `loyalty.*`, `customers.view_loyalty`, `customers.redeem_points` |
| Credit Sales | `/credit` (+ credit payment in a sale) | `credit.*`, `customers.view_credit` |
| Expenses | `/expenses` | `expenses.*` |
| Reports & Analytics | `/reports`, `/leaderboards` | `reports.*` |

Core (always included): products, dashboard, settings, users, branches, roles, approvals, audit, notifications,
search, billing. The session check (`auth.rs`) refuses a module outside the package with 422 *Not in your package*;
the profile carries the module list and the web app's `can()` treats the module's permissions as not held, so menus,
screens and buttons follow. No plan = everything (existing businesses keep working). The platform owner acting
inside a business is not restricted.

## 42 — Tenant-specific pricing

Per business: billing model, package, base price — or per-module prices (the base is the sum of the included modules) —
frequency, currency, discount (none / percentage / fixed), tax (yes/no + %), start date, next billing date, grace days.
`billing::calculate`: discount on the base, tax on the discounted amount, rounded to cents:
**Base → Discount → Tax → Amount payable** (e.g. 120,000 + 16% = 139,200). Invoices and quotations store subtotal,
discount, tax rate, tax and the amount payable; Paystack charges the amount payable; the PDF shows the lines.

## 43 — Free, trial and grace

* **Free** — billing off without deactivating; no price needed; no automatic invoices.
* **Trial** — start, end and (optionally) its own modules. The Billing page and a banner in the last week show when
  it ends; the background job then switches the plan to billed (audited `trial_ended`) and the first recurring
  payment is due the day after the trial.
* **Grace** — `grace_days` after an invoice's due date, plus an explicit extension date (`grace_until`). With
  *Suspend when overdue* on, an invoice overdue past both suspends the business: sign-in, notifications and Billing
  keep working so an administrator can pay; everything else gets 422 *Billing suspended*; the ordering link stops.
  Paying, voiding, extending grace or switching to free lifts it immediately (`billing::refresh_suspension`, audited
  `billing_suspended` / `billing_restored`).

## 44 — Statuses

`platform_owned`, `trial`, `free`, `active` (subscription), `one_off_paid`, `payment_due`, `maintenance_due`, `grace`,
`overdue`, `suspended` (and `not_set`) — computed once in `billing::summary` and used by the platform dashboard,
directory, business detail, the business's Billing page and the session.

## 45 — The platform owner's business

`tenants.ownership = 'platform'` for every business with an active platform administrator (set at start-up). It keeps
the complete platform; *Deactivate*, *Plan* and *Issue* are disabled on its page; the API refuses a plan, quotation,
invoice or deactivation for it; database triggers refuse deactivation, suspension, plans, documents and payments for it
and the removal of the platform ownership itself. Platform-owner privileges remain the separate `PLATFORM_ADMIN_EMAILS`
check, not a tenant permission.

## 46 — One-off and maintenance

A one-off plan has the one-off fee (invoiced, or marked *paid on* a date for a payment made before) and, independently,
*Maintenance required* with its own amount, frequency, start and next due date; discount and tax apply to both.
Automatic invoices for a one-off plan are maintenance invoices only, and only when maintenance is on.

## Audit

Every plan save writes `billing.plan_updated` (in the business and the platform owner's audit trail) with
`changes: { field: { from, to } }` for each field that changed — pricing, modules, discount, tax, trial, grace,
free access, auto-suspend — plus who and when. Suspension, restoration and trial end are audited by the system.
