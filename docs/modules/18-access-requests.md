# 18 · Access requests (no open signup)

**Scope:** owner request 7 (2026-10-05) · **Routes:** `/request-access` (public), Settings → Access requests (platform
admins) · **API:** `POST /api/access-requests`, `/api/platform/access-requests…`

## Who can do what
- **Anyone** can submit a request from the sign-in screen (*Interested in accessing S'Shop? → Request Access*).
- **Platform admins** — users whose email is in `PLATFORM_ADMIN_EMAILS` (falls back to `BOOTSTRAP_ADMIN_EMAIL`) *and*
  who hold the Tenant Administrator role — review requests. Nobody else sees the section or can call its API.
- Nothing is activated automatically.

## The form
Business name, your name, phone, email, number of branches, type of business, town/location and an optional message.
After submitting, the visitor sees: *Access request submitted successfully. We will review your request and get back
to you. In case of any delays, please call 0798 993 404 / 0732 968 898* (tap-to-call).

Safeguards: a hidden honeypot field drops bot submissions; at most 5 requests per connection per hour; one pending
request per email (a repeat is acknowledged but not duplicated); an email that already has a login is told to sign in.

## Notifications
- **Email** to `ACCESS_REQUEST_NOTIFY_EMAILS` (default: the platform admins, i.e. `jamosammy@gmail.com`) via
  [Resend](https://resend.com) when `RESEND_API_KEY` is set. Without a verified domain, Resend's test sender
  (`onboarding@resend.dev`, the default `MAIL_FROM`) delivers only to the Resend account owner's address — sign up to
  Resend with the address that should receive the alerts, or verify a domain and set `MAIL_FROM`.
- **In-app** notification (bell + live toast) for every platform admin.
- If email is not configured the request is still stored and the review screen shows a warning.

## Review (Settings → Access requests)
Tabs Pending · Approved · Rejected · All. Each card shows the business, contact (tap to email/call), location, type,
branches and message.
- **Approve & create** creates the business exactly like a first start (default roles, *Main Branch*, workflow rows,
  expense categories, award period, a unique ordering-link slug) and its Tenant Administrator with a random
  8-character **temporary PIN**, shown once with *Copy message* and *WhatsApp* buttons to pass on the sign-in details.
  The new administrator should change the PIN after signing in.
- **Reject** with an optional reason.
Both decisions are written to the audit trail (`platform.approve_access` / `platform.reject_access`).
