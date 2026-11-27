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
| Products | view, create, edit, price (*Set selling prices*, default and per branch — roadmap 176), deactivate |
| Stock | view, add, adjust, write_off, transfer, receive_transfer |
| Customers | view, create, edit, view_loyalty, redeem_points, view_credit · loyalty.manage |
| Expenses | view, create |
| Reports | view, export |
| Administration | approvals.approve, branches.manage, users.manage, roles.manage, settings.manage, audit.view |

## Tenant Administrator and approvals (roadmap 150)

The Tenant Administrator role is the business's system role: the only role with full access (`*`); its name cannot be
changed, full access cannot be given to another role, and its administrators may switch off optional permissions
(roadmap 183, below). It is the only role that can use the
self-approval exception of the workflow engine ([module 15](15-approvals.md#tenant-administrator-self-approval-roadmap-150)),
and only a Tenant Administrator changes that policy.

## Default roles
Tenant Administrator (everything; optional permissions can be switched off, essentials locked) · Manager · Branch Manager · Salesperson · Storekeeper ·
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

## Editing the Tenant Administrator role (roadmap 183)

Settings → Roles & permissions → Tenant Administrator → **Edit** (pencil): the usual editor, every permission checked.
**Locked on** — checked, disabled, *Required for Tenant Administrator — cannot be disabled*: Manage users, Manage roles
& permissions, Manage all settings (and every settings area), Manage branches, Approve requests, View audit trail
(`perms::ADMIN_REQUIRED`). Anything else can be switched off; it is stored as `roles.excluded_permissions` and sent to
the app as `-permission` entries, so the role keeps `*` (full access to everything else, every branch and record, the
maker-checker exception, the platform owner's console). Only a Tenant Administrator of the same business, fully signed
in and not through S'Shop support, sees the Edit button and can save (`PUT /api/roles/{id}` refuses anyone else and any
essential permission sent switched off). Saving is audited (`roles · update`, before/after *switched_off*). Switched-off
permissions apply at once on the server, in the app and in notifications (`notify::to_permission`, new-order alerts,
sale-owner choices).

## My profile and the account menu (roadmap 185)

The avatar — top-right on every screen size, the sidebar profile on desktops, the More page on phones — opens the
account menu: **My profile**, **Edit profile**, **Change PIN** (the existing dialog), **Face ID & passkeys**
(User preferences → *Sign-in*, the existing passkey and Quick PIN cards), **User preferences**, **Sign out**.

- **My profile** — Settings → Personal → My profile (`pages/settings/Profile.tsx`, `GET /api/auth/profile`).
- **Edit profile** — full name (2–80 characters; tidied; no markup) and photo. Email, role and branches are shown
  read-only and are never changed here (`PUT /api/auth/profile` reads the name only). *Save changes* only for a valid
  change.
- **Photo** — chosen on the device, cropped and repositioned in the browser (drag, zoom) to a 512 × 512 square
  (WebP, else JPEG), previewed, then `POST /api/auth/profile/avatar`; *Remove photo* → `DELETE` (initials again). The
  server accepts PNG, JPEG or WebP up to 1 MB whose own header shows a square-ish image of 64–2048 px. Stored on the
  person's sign-in identity (`users.avatar`, shared by their linked access in the tenant's other businesses) and served
  at `/api/avatars/{key}` — a key that changes with every photo, cached for good, sent with a sandboxing policy.
  `user_avatar_url(user_id)` gives the address (null = initials) to the profile, Settings → Users and Leaderboards;
  `components/UserAvatar.tsx` shows it round with `object-fit: cover`.
- **History** — audit entries keep the name the person had when they acted (`audit_log.actor_name`); the change is
  audited (`profile_updated` with before/after name; `profile_photo_added` / `_replaced` / `_removed`, never the image).
- Only the person changes their own profile (any role); never from a support session.

## Saving a role (roadmap 189)

The role editor's button reads *Nothing to save* (off) until something differs from the role as it was opened —
permissions (one by one or *Toggle all*), name, description, *Active* or a visibility scope — then *Save changes*;
undoing every change switches it off again. *Saving…* blocks a second click; a failure keeps the changes.

## Approval and notification permissions (roadmaps 193–194)

*View own approval requests* (Administration) and the **Notifications** group (ten *Receive … notifications* permissions,
Toggle all) are set per role like any other; existing roles received them all so nothing they saw disappears. A
notification permission never widens module or branch access.

## Tenant Administrator permissions (roadmap 199)

Every permission of the Tenant Administrator role can be switched off (none is locked). Critical administrative ones
(approve requests, branches, users, roles & permissions, all settings, audit trail) need a warning and the person's PIN.
Tenant Administrators can always reopen Roles & permissions and restore their role (recovery: no permanent lockout).
Switched-off permissions are enforced in the session, screens, API, approvals and notifications; changes are audited.
