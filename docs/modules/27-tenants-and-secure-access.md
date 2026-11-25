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
| Session checks (support sessions, linked users) | `server/src/auth.rs` (`Ctx` extractor; tokens from `sessions::issue`, module 30) |
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
| Time limit | 15 min – 8 h, enforced on every request; once it passes (or the business revokes it) the token can only take the platform owner back to their own business (roadmap 182) |
| Tenant consent | Each business chooses (Settings → Support access): *Notify administrators* (default — support may enter, administrators are notified at once) or *Ask for approval first* (nothing opens until an administrator approves; an approval must be used within 24 h). Only the business's own administrators change it — never from a support session, and never through the settings form |
| Indicator | *Platform support session · business · access · time left · You: name (S'Shop support) · reason* with **End support session** (*Exit tenant* on phones); ending it, the time limit or a revocation opens the Platform Owner Console without signing in again (roadmap 182) |
| Revocation | The business ends it at any time (*End access now*); the platform owner ends or withdraws it; it expires automatically; deactivation ends it. The next request is refused and the live event stream closes within 15 s |
| Audit | Since roadmap 182: every step and every change attempted is in the confidential **Platform Security Audit Trail**; the business's own decisions (approve, decline, revoke) stay in its audit trail, changes made by support show there as *S'Shop Support · name*, and the business sees every session in Settings → Security |
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
| 8 | Platform activity monitoring and audit trails | ✅ tenant filter; support access in the Platform Security Audit Trail (roadmap 182) |

## 182 — Full-access limits and the Platform Security Audit Trail

**What full access never does** (refused *Not from a support session*, recorded as a refused change): create or change
users, roles or PINs (`/users`, `/roles`), sign-in and security rules (`/security`, `/auth/*` except sign-out),
support consent (`/support-access`), approval rules (`/settings/workflows`, `/settings/approval-policy`), M-Pesa
credentials (`/mpesa/accounts`), billing (`/billing`), the website's access and domain, or approvals (`/approvals` —
support never decides the business's requests). Everything else goes through the business's own approval workflow:
a rule limited to some roles applies to support too (it holds none of the business's roles). View only stays refused
at the server; its *Save* buttons are disabled.

**Platform Security Audit Trail** — table `platform_audit_log`, written by `server/src/platform_audit.rs`:

| Event | When |
|---|---|
| `session_requested`, `request_approved`, `request_declined`, `request_withdrawn`, `approval_lapsed` | the consent steps |
| `session_started` | with `detail.authentication` = *PIN re-entered*, minutes, policy |
| `session_ended`, `session_revoked`, `session_expired`, `session_signed_out`, `session_idle`, `session_replaced` | how it ended (expiry is recorded once, when first noticed) |
| `change`, `change_refused` | every non-read request made with the session's token: method, address, status — after the request, so the outcome is the real one |

Each row copies the business, the platform administrator (name and email), the scope and the reason, so it stays
readable whatever happens later. The database refuses `UPDATE`, `DELETE` and `TRUNCATE` on it and it has no foreign
keys. `support_sessions` counts `changes`, `refused` and `reads`. **Who reads it:** platform administrators only, from
their own business — Settings → Platform → **Security audit** (`GET /api/platform/security-audit`, filters: business,
session, period, kind). Tenant administrators and support sessions are refused. Platform → Activity's *Support access*
filter reads it too.

**The business's audit trail** keeps its own people's history unchanged. Support sessions themselves are no longer
added to it (older ones stay); a change made by support carries `audit_log.support_session_id` and shows as
**S'Shop Support · name**. Disclosure is unchanged: notifications, the consent policy and Settings → Security → Support
access (every session, reason, scope, times, *End access now*).

**Ending:** *End support session*, signing out, the inactivity timeout, the time limit and the business's revocation
all end the support session at once (the sign-in session carries `user_sessions.support_session_id`). After the time
limit or a revocation the app returns to the Platform Owner Console by itself; the old token's only remaining use is
`POST /api/platform/support/{id}/end`.
