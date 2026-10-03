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
