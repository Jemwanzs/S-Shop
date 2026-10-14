# 14 · Users, roles & permissions

**Scope:** §28 · **Routes:** Settings → Users, Roles & permissions · **API:** `/api/auth/*`, `/api/users…`, `/api/roles…`, `/api/permissions`

## Signing in
Email + PIN (4–12 characters). PINs are Argon2-hashed; 5 wrong attempts lock the account for 15 minutes; sessions end
after inactivity and at the platform's longest session (module 30). Users change their own PIN (avatar → Change PIN); administrators reset others' PINs. Emergency recovery:
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

## Inviting users (roadmap 105)

**Settings → Users → Add user** asks only for full name, email, mobile number (optional), role and branches —
**nobody chooses a PIN or Quick PIN for another person**.

- **Create & invite**: the account is created waiting for set-up (its PIN is an unusable random value until then, so
  nobody can sign in) and S'Shop emails the person a **single-use invitation link valid for 10 minutes** (roadmap
  111; `auth_tokens` kind `invite`, stored hashed; new-business welcome links stay `setup`, 72 hours). The email and
  the set-up page say *This invitation link expires in 10 minutes* (the page counts down); the server enforces it. An
  expired or replaced link answers **Invitation expired** (ask your administrator for a new invitation); a used one
  **Invitation already used**. Each new invitation ends every earlier unused invitation or set-up link. They choose their own PIN on `/set-pin`; the link is then used up, other links stop
  working and they sign in with email and PIN. A Quick PIN can never be used to activate an account.
- **User created successfully** shows whether the email went out, with **Send email**, **Share via WhatsApp** and
  **Done**. The WhatsApp message carries the sign-in address, their email and how to activate — never a PIN or the
  link — and opening WhatsApp is never reported as delivered.
- **Statuses** in the list: *Invitation pending* (email not sent), *Invitation sent*, *Activation pending* (link
  opened), *Invitation expired*, *Active*, *Disabled*. For accounts not yet set up, the ✉ action reopens the
  invitation: send the email again, **Copy activation link** (a new single-use link shown once to the administrator,
  replacing earlier ones; rate-limited; audited without the link), Share via WhatsApp, Disable user. An account that
  is already set up is not invited again (use *Reset PIN*). If the email fails the user stays safely pending.
- **First sign-in**: after setting up, the person is offered **Enable Quick PIN login?** once (*Set up Quick PIN* /
  *Maybe later*), only where the business allows Quick PIN.
- `POST /users` still accepts `pin` for integrations (the account is then active straight away, as before).

**Full sign-in needed** (Quick PIN sessions): instead of a dead end, *Confirm it's you* asks for the full PIN /
password (the Quick PIN is not accepted), the session becomes a full one (`POST /auth/step-up`; the Quick session
ends; wrong PINs count towards the normal lockout) and the same request is sent again — what was typed stays on
screen and nothing is created twice (the check runs before anything is written).
