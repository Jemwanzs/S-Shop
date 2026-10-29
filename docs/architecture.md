# Architecture

*Checked against the code on 2026-10-29 (migrations 0001–0043). Update this page when a component, table, integration
or security rule changes.*

## Overview

```
 Phones / tablets / desktops ──HTTPS──▶  S'Shop service (one Rust binary on Railway, https://s-shop.store)
   • staff workspace   /{business}/…        ├─ /api/*            JSON API (Axum)
   • sign-in           /login[/{business}]  ├─ /api/events       live events (SSE)
   • ordering link     /order/{business}    ├─ /api/webhooks/*   M-Pesa, Paystack, WhatsApp, Resend callbacks
   • tracking/receipts /track/…, /r/…       ├─ /s/{business}, custom domains   business websites (server-rendered)
                                            ├─ /assets, /*       built React app (SPA fallback)
                                            └─ background jobs   every minute (STK) and every 15 min
                                                     │
                                      PostgreSQL 16 (Railway) ◀── migrations run at start-up
 Safaricom Daraja ◀── STK Push / status ──────────────┤
 Paystack ◀── platform billing payments ──────────────┤
 Resend ◀── onboarding & account emails ──────────────┤
 Meta WhatsApp Cloud API ◀── messages / templates ────┘
```

One deployable keeps hosting simple and same-origin: one domain, bearer tokens (no cookies), no CORS for the app.
Deployment and Railway settings: [deployment-railway.md](deployment-railway.md).

## Server (`server/src`)

| Path | Responsibility |
|---|---|
| `main.rs` | Start-up, migrations, router, static files, security headers, CLI (`serve`, `reset-pin`, `import-legacy`, `seed-demo`) |
| `config.rs` · `state.rs` | Environment configuration (integrations enabled only when fully configured) · shared state (pool, event channel) |
| `auth.rs` | Argon2id PIN hashing, JWT issue/verify, the `Ctx` extractor (user, permissions, Current Branch, session, support session) and `PortalCustomer` |
| `sessions.rs` | Server-side staff sessions and automatic logout after inactivity (roadmap 87) |
| `perms.rs` | Permission catalogue, default roles, legacy role mapping |
| `settings.rs` | Typed per-business settings with defaults (one JSON document per business) |
| `inventory.rs` | **Inventory ledger** — `lock`, `apply` movement, `reserve`, `release` |
| `loyalty.rs` | Points rules, award, referral bonus, reversal, redemption, expiry |
| `workflow.rs` | Approval engine (`needs_approval`, `submit`, `can_decide`, `approvers`) |
| `audit.rs` | Audit-trail writer (same transaction as the change) |
| `notify.rs` | In-app notifications (+ live push) and WhatsApp fan-out |
| `billing.rs` | Platform billing: packages, modules, tenant pricing, invoices, trials, grace, suspension |
| `receipts.rs` | One receipt structure for every channel (view, PDF, image, print, email, WhatsApp, link) |
| `onboarding.rs` | Excel templates and reading uploaded workbooks (import engine in `routes/onboarding.rs`) |
| `website.rs` · `domains.rs` | Business website configuration · custom domains |
| `mailer.rs` | Account emails through Resend with a delivery log |
| `secrets.rs` | Encryption at rest for credentials businesses give S'Shop (M-Pesa) |
| `dedupe.rs` · `ratelimit.rs` | Duplicate-submission guard for every write · rate limits on unauthenticated endpoints |
| `geo.rs` | Geofencing: selected actions only from devices at the Current Branch |
| `jobs.rs` | STK sweep every minute; every 15 min: billing jobs, points expiry, overdue-credit alerts, M-Pesa timeouts, old portal codes, alert retention |
| `alerts.rs` | Error alerting for the platform owner: faults, failed jobs, restarts → grouped `error_events`, bell + email ([module 40](modules/40-error-alerting.md)) |
| `demo.rs` · `legacy.rs` · `bootstrap.rs` | Pablo Niche demo business · Pablo Loyalty importer · first business on an empty database |
| `integrations/` | `mpesa.rs` (Daraja), `paystack.rs`, `email.rs` (Resend), `whatsapp.rs` (Cloud API) |
| `routes/` | One module per functional area (37); each exposes its routes and, where approvals apply, `on_approved()` |

### Request context

Every staff request carries `Authorization: Bearer <jwt>` and usually `X-Branch-Id`. The `Ctx` extractor, on **every
request**:

- checks the server-side session (ended, idle too long → 401 `session_expired`);
- loads the user's role permissions and branches, so deactivation and permission changes apply immediately;
- rejects a branch the user is not assigned to;
- applies the business's package (modules) and billing state.

Handlers call `ctx.require("module.action")`; sensitive actions also call `ctx.require_full()` (no Quick PIN session).

The business is always the one in the session token. The business slug in workspace addresses
(`/{business}/products`, [module 39](modules/39-workspace-addresses.md)) is never used to choose data.

### Transactions & integrity

All multi-step operations run in **one PostgreSQL transaction**: sale completion, returns, transfers, order
completion, approvals and imports. Stock rows are locked with `SELECT … FOR UPDATE` before checking availability, so
two cashiers cannot sell the last unit twice. The database enforces:

- unique customer mobile per business, unique product code, unique **active** barcode (partial index);
- platform-unique item codes, never reused (`barcode_registry`);
- non-negative reservations, positive quantities, `returned_qty ≤ quantity`;
- one open award period per business, one active referrer per customer;
- one payment per M-Pesa confirmation code, idempotent POS submissions (`client_ref`);
- unique business slugs, including former ones (`tenant_slug_history`).

Financial and operational records are never deleted. Instead:

- sales are cancelled or returned;
- expenses are voided;
- products and customers are deactivated.

A *Books closed up to* date blocks changes dated on or before it.

## Inventory ledger

```
stock_movements (append-only)            stock_levels (projection, locked per update)
 branch · product · item · kind · qty     branch · product · on_hand · reserved
```

`inventory::apply` does four things, all in the caller's transaction:

1. locks the level row;
2. checks the rule: `Available` (on hand − reserved), `OnHand`, or `None` for recounts;
3. inserts the movement;
4. updates `on_hand`.

Movement kinds: `opening, received, sale, order_completion, transfer_out, transfer_in, customer_return, sale_reversal,
supplier_return, damage, loss, write_off, adjustment, count_variance`. Individually tracked units live in
`stock_items` (`in_stock → reserved/in_transit → sold/written_off/…`), with barcode history in
`stock_item_barcode_history`.

## Data model (tables)

| Area | Tables |
|---|---|
| Businesses & access | `tenant_accounts`, `tenants`, `tenant_slug_history`, `branches`, `roles`, `users`, `user_branches`, `access_requests`, `support_sessions`, `platform_settings` |
| Sign-in & sessions | `user_sessions`, `auth_tokens`, `trusted_devices`, `passkeys`, `passkey_challenges` |
| Catalogue | `products`, `product_branches`, `product_photos`, `product_fields`, `categories`, `suppliers`, `barcode_registry` |
| Inventory | `stock_levels`, `stock_movements`, `stock_items`, `stock_item_barcode_history`, `stock_adjustments`, `transfers`, `transfer_items` |
| Sales | `sales`, `sale_items`, `payments`, `pending_sales`, `mpesa_accounts`, `mpesa_requests`, `credit_sales`, `sale_returns`, `sale_return_items`, `sale_owner_changes`, `receipts`, `receipt_assets` |
| Orders | `orders`, `order_items`, `order_events`, `portal_otps` |
| Customers & loyalty | `customers`, `customer_fields`, `referrals`, `loyalty_ledger`, `award_periods`, `award_winners` |
| Finance | `expenses`, `expense_categories` |
| Platform billing | `billing_plans`, `billing_documents`, `billing_payments` |
| Websites | `websites`, `website_versions`, `website_media`, `website_domains`, `website_campaigns`, `website_events` |
| Onboarding | `import_batches`, `import_rows`, `import_progress` |
| Control | `workflows`, `approvals`, `audit_log`, `notifications`, `whatsapp_messages`, `email_log`, `doc_counters` |
| Operations | `error_events`, `service_starts` |

Approval requests carry the requester's reason (`note`) and, when created together, a shared `bulk_ref` (migration 0043).

Document numbers (`RCP-2026-000001`, `ORD-…`, `TRF-…`, `RTN-…`) come from `doc_counters` per business and year.
Schema changes are numbered migrations in `server/migrations` (applied at start-up, never edited once released).

## Security

**Sign-in**
- PINs are hashed with Argon2id. 5 failed attempts lock an account for 15 minutes. Sign-in timing does not reveal
  whether an email exists.
- Optional faster sign-in on trusted devices:
  - Quick PIN ([module 28](modules/28-quick-login-pin.md));
  - Face ID / biometric passkeys ([module 37](modules/37-face-id-biometric-login.md)).
- A Quick PIN session must confirm the full PIN for sensitive actions (step-up).

**Sessions**
- Staff sessions are server-side. They end after inactivity: the platform default is 5 min, a business may choose
  1–120 min, and roles can be stricter. The longest session is 12 h.
- *Sign out everywhere* and PIN changes end sessions.
- Portal (customer) sessions last 30 days and are bound to one business.

**Access control**
- Permissions, branch access, package modules and billing state are checked server-side on every request; the UI
  only hides what is not allowed.
- Maker-checker: requesters never approve their own requests.
- Platform support access is scoped, time-limited and audited. A business administrator never gains platform access.

**Secrets and credentials**
- Platform secrets live only in environment variables.
- Credentials a business enters (M-Pesa) are encrypted at rest and never logged.
- Settings shown in the app never contain credentials.

**Webhooks**
- WhatsApp: verified with Meta's app-secret HMAC.
- M-Pesa callbacks: authenticated by a secret URL token.
- Paystack: signature checked.

**Abuse and data protection**
- Rate limits on sign-in, access requests and the ordering portal.
- Duplicate-submission guard on every write (`Idempotency-Key`).
- Strict CSP, HSTS and no framing.
- Cost prices and item barcodes are masked for roles without the right permission.
- CI runs `cargo audit` and `npm audit` on every push and weekly (reported, not gating deploys; advisories are fixed
  as their own change).

**Operations**
- Faults, failed background jobs, restarts and an unreachable database alert the platform owner in-app and by email,
  grouped ([module 40](modules/40-error-alerting.md)). A panic inside a request answers 500 instead of dropping the
  connection.

## Real-time

`GET /api/events?access_token=…` streams `notification`, `order`, `stock`, `sale`, `approval` and `mpesa` events
(ids only). The web app invalidates the matching queries, so screens refresh themselves — e.g. the POS sees an STK
payment confirmation the instant Safaricom calls back. Events are delivered in-process (a broadcast channel); when
scaling beyond one instance, replace it with PostgreSQL `LISTEN/NOTIFY`.

## Web app (`web/`)

**Stack**
- React 18 + TypeScript.
- React Router 7. Workspace links go through `@/lib/router`, which adds the business slug
  ([module 39](modules/39-workspace-addresses.md)).
- TanStack Query 5: cached, refetched on focus and on live events.
- Tailwind CSS 4 design tokens, Radix primitives (shadcn/ui).
- Every page is code-split.

**Other parts**
- Languages: English, Kiswahili, French and Arabic (right-to-left). The translation dictionary loads only for a
  non-English language.
- A service worker keeps the app shell for offline use. The offline POS queues sales.
- The public business websites are a separate small app (`web/src/site`), served under `/s/{business}` and custom
  domains.

See [ui.md](ui.md).
