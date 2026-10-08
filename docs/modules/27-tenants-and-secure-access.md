# 27 — Tenants, secure business access, several businesses per tenant (roadmap 70–73)

**Platform owner → tenants → businesses → branches → users & operations.**

A tenant is a customer account. It owns one or more businesses, and each business keeps its own branches, products,
stock, sales, orders, expenses and reports. The platform owner manages tenants centrally (status, billing, services,
activity) without entering their workspaces. Entering a workspace is always a separate step: either the tenant's
administrator signs in with their own credentials, or the platform owner uses audited, time-limited support access.

| Part | Where |
|---|---|
| Tenant accounts, support sessions, switching, linking people | `server/src/routes/tenants.rs` |
| Schema | `migrations/0021_tenant_accounts_support_access.sql` — `tenant_accounts`, `tenants.account_id`, `support_sessions`, `users.login_user_id` |
| Session checks (support sessions, linked users) | `server/src/auth.rs` (`Ctx` extractor, `issue_acting_token`) |
| Business / tenant status | `routes/platform.rs` (`change_status`) |
| Screens | Settings → Platform → **Tenants** (`pages/settings/Tenants.tsx`); *Open business* dialog (`components/OpenBusinessDialog.tsx`); Settings → **Support access** (`pages/settings/SupportAccess.tsx`); support banner and *Switch business* (`components/layout/AppShell.tsx`); Settings → Users → *From another business* (`pages/settings/People.tsx`) |

## 70 — Tenant accounts

- **One tenant per approved request.** Approving an access request creates the tenant account, its first business and
  its administrator, who becomes the tenant's **primary administrator**. No duplicate tenant records are created.
- **Existing data.** Every business that existed before this release became the first business of its own tenant (the
  tenant has the same id). The platform owner can then group businesses that belong to one customer under one tenant
  (`POST /platform/tenants/{id}/account`). Platform-owned and customer businesses never share a tenant.
- **Platform → Tenants** shows a card per tenant: name, primary administrator (email, mobile), businesses, branches,
  people, status, billing status and next due date. Search covers tenant name, business names and emails.
- **Tenant page** has these tabs:
  - *Overview*: profile, primary administrator, notes, totals and outstanding amount.
  - *Businesses & branches*: open a business, add a business.
  - *Users*: every person in every business, with administrator / linked / inactive flags and last sign-in.
  - *Billing*: each business's plan, status, next due date and outstanding amount, linking to the existing billing
    tools.
  - *Website services*: per business, linking to the existing website management.
  - *Activity & security*: support sessions and the activity feed for this tenant.
  - *Access & status*: tenant status and each business's support policy.
- **Status.** Shown as *Active*, *Trial*, *Grace period*, *Suspended* or *Deactivated*. It comes from the businesses'
  billing (the most pressing one) and their activation; *Platform owned* tenants are never billed.
- **Activate / deactivate a whole tenant.** This applies the existing business deactivation to every business of the
  tenant:
  - a reason is required;
  - sessions end and sign-in is blocked;
  - transactions, the ordering link and websites stop;
  - support sessions end;
  - data, billing and audit history are kept.
  Reactivation restores access according to each business's billing and services. Platform-owned tenants and the
  platform owner's own business are refused.
- **Billing stays per business.** Plans, pricing, discounts, tax, trials, invoices, quotations, payments and Paystack
  are set per business (module 21). The tenant page aggregates them. Tenant administrators see their own billing in
  Settings → Billing.

## 71 — Secure business access (no automatic switching)

*Open business* no longer switches into another business. It opens a dialog with two separate routes.

**A. Sign in as the tenant's administrator.** The administrator types their own email and PIN. S'Shop never sees,
stores or shows it, and the platform owner's session is replaced by the administrator's normal session.

**B. Platform support access.**

| Control | How |
|---|---|
| Fresh authentication | The platform owner re-enters their PIN for every request (rate-limited) |
| Reason | Required (10–500 characters); shown to the business and stored |
| Scope | *View only* (the server refuses every change) or *Full access* |
| Time limit | 15 min – 8 h; the token expires with the session |
| Tenant consent | Each business chooses (Settings → Support access): *Notify administrators* (default — support may enter, administrators are notified at once) or *Ask for approval first* (nothing opens until an administrator approves; an approval must be used within 24 h). Only the business's own administrators change it — never from a support session, and never through the settings form |
| Banner | *Support access · business · scope · time left · reason* with **End session** |
| Revocation | The business ends it at any time (*End access now*); the platform owner ends or withdraws it; it expires automatically; deactivation ends it. The next request is refused and the live event stream closes within 15 s |
| Audit | Requested, approved, declined, started, ended and revoked are recorded in the business's audit trail and the platform's. Everything done in the session is audited as usual |
| No impersonation | The session acts as the platform owner (their own name in every record), never as a tenant user. Bare acting tokens from before this release no longer work |

- Platform-owned businesses (such as the demo) use the same flow, with no consent step; the dialog suggests full
  access for 8 h.
- Only the platform owner's own business opens directly.
- From inside a support session, nobody can approve support requests, change the policy, switch business or change
  the PIN.

## 72 — Several businesses per tenant, one sign-in

- **Add business** (platform owner, tenant page) creates another business for the tenant, with its own Main Branch,
  roles, workflows and settings. Billing starts unset. The tenant's primary administrator administers it with their
  existing sign-in.
- **Give a person access to another business** (Settings → Users → *From another business*, `users.manage`). This
  lists active people from the tenant's other businesses, never the platform owner. Each person gets this business's
  role and branches; scopes and exceptions are set as for anyone else.
  - Technically this creates a *linked user row* in this business (`users.login_user_id` → the person's sign-in
    account). Every business therefore keeps working with its own users: sale owners, leaderboards, reports, approvals
    and audit.
  - Linked rows cannot sign in by themselves and have no PIN.
  - Their name, email and phone are managed in the person's own business. Discount overrides check the person's own
    PIN.
- **Switch business** (user menu → *Current business*) lists only the person's businesses within the same tenant and
  switches without a new sign-in.
- **The boundaries are enforced on every request.** Access ends immediately if:
  - the person is deactivated in their own business;
  - their link in the target business is deactivated;
  - their PIN changes (all sessions end);
  - the target business is deactivated;
  - the business is moved to another tenant.
  Other tenants are never reachable this way.

## 73 — Tenant activity

The activity feed (Platform → Activity, and the tenant page) filters by tenant (`account_id`), business, branch, user,
activity and date range.

- **Activities shown:** sign-ins and failed sign-ins, sales, stock counts and adjustments, stock received, transfers,
  PIN resets, platform actions (including *business added*, *moved* and *tenant updated*), **support access** (every
  step) and billing.
- **Privacy:** rows show the activity, user, business, branch, time and a short reference only — no customer or
  financial details.

## Audit report (owner checklist)

| # | Check | Status |
|---|---|---|
| 1 | Approved access requests create proper tenant records | ✅ one tenant account + first business + primary administrator, in one transaction |
| 2 | Tenants can own multiple businesses | ✅ *Add business*; existing businesses can be grouped |
| 3 | Businesses keep independent branches and operational data | ✅ unchanged — every record is scoped to its business |
| 4 | Platform owner switching bypassed authentication | ✅ fixed — *Open business* now needs the administrator's own sign-in or a support session (fresh PIN, reason, scope, time limit, consent policy) |
| 5 | Secure tenant login and controlled support access | ✅ both routes; MFA beyond PIN re-entry is a later option |
| 6 | Billing, website services and activation at the right level | ✅ status at tenant level; billing and website per business, aggregated per tenant |
| 7 | Permissions and session boundaries prevent cross-tenant access | ✅ server-side on every request (smoke suite *Roadmap 70–73*, *Hardening*) |
| 8 | Platform activity monitoring and audit trails | ✅ tenant filter, support access in both audit trails |
