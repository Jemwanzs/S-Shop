# 14 · Users, roles & permissions

**Scope:** §28 · **Routes:** Settings → Users, Roles & permissions · **API:** `/api/auth/*`, `/api/users…`, `/api/roles…`, `/api/permissions`

## Signing in
Email + PIN (4–12 characters). PINs are Argon2-hashed; 5 wrong attempts lock the account for 15 minutes; sessions last
12 hours. Users change their own PIN (avatar → Change PIN); administrators reset others' PINs. Emergency recovery:
`sshop reset-pin <email> <pin>` on the server.

## Users
Name, email, phone, role, branches (all or selected), active. Deactivation signs the user out on the next request.
Safeguards: you cannot deactivate yourself; only administrators create or change administrators; at least one active
administrator must remain.

## Permissions (module.action)
| Module | Permissions |
|---|---|
| Dashboard | view |
| Sales | view, create, discount, discount_override, change_branch, return, cancel, view_financials |
| Credit | view, collect, write_off |
| Orders | view, manage |
| Products | view, create, edit, deactivate |
| Stock | view, add, adjust, write_off, transfer, receive_transfer |
| Customers | view, create, edit, view_loyalty, redeem_points, view_credit · loyalty.manage |
| Expenses | view, create |
| Reports | view, export |
| Administration | approvals.approve, branches.manage, users.manage, roles.manage, settings.manage, audit.view |

## Default roles
Tenant Administrator (everything, cannot be edited) · Manager · Branch Manager · Salesperson · Storekeeper ·
Order Manager · Finance · Auditor · View Only. Create your own roles by ticking permissions per module.

Navigation, buttons and figures follow permissions in the UI, and every API call checks them again on the server.

## Access rules added in roadmap 14
- **View other employees' sales & performance** (`staff.view_others`). Without it a user sees only their own sales
  (list and receipts), their own figures on dashboards and reports, and per-employee reports are hidden and refused.
  Granted by default to Manager, Director, Supervisor, Branch Manager, Finance and Auditor; not to Salesperson.
- **Print & share receipts** (`sales.print`) controls Print, PDF and Share/WhatsApp on receipts.
- **Settings by area**: business profile, sales, stock, products, orders, customers & loyalty, expenses, reports,
  workflows, integrations. *Manage all settings* still grants every area. Saving the settings document checks each
  changed area against its permission on the server.
- **No self-promotion**: only administrators can grant permissions they do not hold. A role can keep permissions it
  already has, but an editor cannot add ones they lack, and cannot assign a role carrying permissions they lack.
- **Retire a role**: roles can be made inactive once no active user holds them; retired roles cannot be assigned.
- New role templates: **Director** (oversight, figures, approvals, reports) and **Supervisor** (counter lead:
  discounts, returns, cancellations).
- Migration 0007 grants `staff.view_others` to existing roles that could already open dashboards or reports, and
  `sales.print` to every role that sells or views sales, so nobody silently loses access.
