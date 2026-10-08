# 25 — Sales ownership, ownership changes, data visibility (roadmap 62–64)

Every sale has a clear owner. Ownership changes are controlled through the workflow engine. Every user sees only the
records their role (and any user-specific exception) allows. Everything is enforced in the Rust API; the screens only
reflect it.

| Part | Where |
|---|---|
| Owner on record, eligible owners, change requests, execution on approval | `server/src/routes/sales.rs` (`eligible_owner`, `owners`, `request_owner_change`, `apply_owner_change`) |
| Workflow action `sale.owner_change` | `server/src/workflow.rs`, `routes/approvals.rs` |
| Scope resolver used everywhere | `server/src/auth.rs` (`Ctx::scope`, `Ctx::visibility`, `Ctx::may_view`) |
| Effective permissions (role + user exceptions) | SQL function `effective_permissions` (`migrations/0019_sale_ownership_scopes.sql`) |
| Role scopes, user exceptions, default branch | `server/src/routes/admin.rs` (`/permissions/scopes`, `/users/{id}/access`) |
| Screens | New Sale → *Sale Owner* bar (`pages/sales/SaleOwner.tsx`); sale page → *Sale ownership*; Settings → Roles (*Data visibility*); Settings → Users → *Roles & Access* |

## 62 — Sale Owner vs Recorded By

- `sales.user_id` is **Recorded By**: who physically entered the sale. It never changes and stays in the audit trail.
- `sales.owner_id` is the **Sale Owner**: who is credited with the sale for performance.
- Existing sales were credited to whoever recorded them.

**At New Sale:**
- A compact *Sale Owner: Name ✎* bar shows who will be credited. It defaults to the signed-in user.
- With **Assign sale owner** (`sales.assign_owner`), tapping it opens a searchable list of *eligible* owners: active staff
  of the business who may record sales (`sales.create`) and work at the sale's branch.
- The choice travels with the cart (including offline sales) and is checked again by the server when the sale
  completes.
- Exchanges credit the replacement goods to the original sale's owner. Orders are credited to the person who completes
  them.

**Receipts** say *Served by* the Sale Owner. The sale page shows both Sale Owner and Recorded By, and the Sales report
has both columns.

## 63 — Changing the owner of a completed sale

1. On the sale page, **Change Sale Owner**. This needs `sales.request_owner_change`, given by default to Manager,
   Supervisor, Branch Manager and Salesperson.
2. Fill in the new owner (eligible at the sale's branch) and a reason (required). The request is shown with the receipt
   number and current owner.
3. The request goes through the workflow engine. Default rule: **Sale ownership change → Tenant Administrator**, one
   level, enabled for every business. Administrators can add levels, limit it by branch, amount or role, or switch it
   off, in which case changes apply at once but are still recorded.
4. Only the **final approval** changes the owner. Rejection or withdrawal keeps the original owner.

**Controls:**
- **One pending request per sale** (a unique index).
- **Segregation of duties:** the requester can never approve their own request; the workflow engine applies this to
  every action.
- **Conflict-safe:** both the request and the sale are locked when applied. If the owner changed in the meantime, or
  the new owner is no longer eligible, the approval is refused.
- **Nothing else changes:** no new sale, and the amount, branch, business date, payments, stock and receipt stay as
  they are.
- **History:** previous and new owner, requester, decider, reason, status and times are stored in
  `sale_owner_changes`, shown on the sale, and audited (`sales.owner_change_requested`, `sales.owner_change`, plus the
  approval decisions).

## Performance follows the owner automatically

Dashboards, My Dashboard, staff and product leaderboards, the sales, staff and credit reports, and the credit and
orders "own" filters all attribute sales by `owner_id`. These figures are computed when read, never stored, so an
approved change is reflected everywhere immediately:

- the old owner's figures drop and the new owner's rise by the sale's **net** value (returns already deducted);
- business and branch totals do not change;
- the sale stays in its original business date and period;
- nothing is double-counted.

## 64 — Data-visibility scopes

**Areas:** Sales history · Dashboards & sales analytics · Sales reports & exports · Leaderboards · Orders · Credit sales.

**Scopes:**

| Scope | Sees |
|---|---|
| **Own records** | Sales credited to the user (orders they created or whose sale is theirs; credit on their sales) |
| **Assigned branches** | Everything at the user's branches |
| **All branches** | Every branch of the business — even branches the user is not assigned to (viewing only) |

**Role scopes:**
- Set per role in Settings → Roles → *Data visibility*, stored as `scope.<area>.<own|branches|all>`.
- Not set: the area follows *View other employees' sales & performance* (Assigned branches if granted, otherwise Own
  records). Existing roles therefore behave exactly as before.
- Tenant Administrators see all branches.

**Visibility never grants operations.** Seeing all branches does not let anyone record, edit, cancel, change owners or
export. Each of those keeps its own permission.

**What the server does in each scope:**
- **Lists, dashboards, reports, Excel exports, leaderboards and search** share one resolver (`Ctx::visibility`), so they
  can never disagree.
- **Own scope:**
  - adds `owner = me`;
  - a request for another person's figures (`user_id`) or for a branch outside the scope is refused;
  - the *Sales by Employee* and *Employee Performance* reports are hidden;
  - the staff leaderboard shows only the user's own row.
- **Single records** (sale, credit sale, order) are checked with the same rule (`may_view`). Whoever recorded a sale
  can always reopen it, for example to reprint the receipt.
- **My Dashboard** is always the signed-in user's own performance, whatever their wider scope.
- **Other businesses' data is never reachable:** every query is limited to the tenant.

## User-specific access (Settings → Users → Roles & Access)

Precedence: **tenant restrictions (package / billing) → role → user exceptions → branch assignment → effective access.**

- **Default branch:** after sign-in it becomes the Current Branch for users with several branches.
- **Scope exception per area:** replaces the role's scope for that area only.
- **Permission exceptions:** *Allow* adds, and *Restrict* removes, any of:
  - Assign sale owner;
  - Request ownership changes;
  - View dashboard & analytics;
  - View reports;
  - Export reports;
  - View other employees.
- **Effective access** is shown per area, exactly as the server computes it (`effective_permissions`).

**Guards:**
- Nobody can give a permission they do not hold, or a scope wider than their own.
- Only administrators change administrators.
- A permission cannot be both allowed and restricted.
- Exceptions do not apply to Tenant Administrators (full access).
- The database CHECK lists every allowed exception.
- Changes are audited (`users.access`). Website permissions (module 23) are kept separately.

## Audit before this work

| # | Item | Was |
|---|---|---|
| 1 | Owner and recorder separate | 🔴 one `user_id` |
| 2 | Owner chosen while recording | 🔴 |
| 3 | Ownership change requests | 🔴 |
| 4 | Workflow engine can run them | ✅ deferred execution, levels, self-approval blocked |
| 5 | Performance follows approved changes | 🔴 (no changes existed) |
| 6 | Role / user scopes | 🟡 own vs others (`staff.view_others`) + branch assignment |
| 7 | Same restrictions on sales, dashboards, reports, exports | 🟡 binary rule, inconsistent for records outside assigned branches |
| 8 | Branch / tenant isolation server-side | ✅ |

## Tests

Smoke section *Roadmap 62–64* (29 checks):

- eligible owners, owner vs recorder, and permission to assign;
- own-scope lists and detail refusal;
- change request → approval with self-approval refused;
- recalculated My Dashboard with business totals unchanged;
- rejection and audit;
- own-scope leaderboard;
- role scopes (assigned vs all branches, one scope per area, visibility without operations);
- user exceptions (widen, restrict, contradictory or unknown refused, no wider than the granter), default branch.
