# 18 · Access requests (no open signup)

**Scope:** owner request 7 (2026-10-05) · **Routes:** `/request-access` (public), Settings → Access requests (platform
admins) · **API:** `POST /api/access-requests`, `/api/platform/access-requests…`

## Who can do what
- **Anyone** can submit a request from the sign-in screen (*Interested in accessing S'Shop? → Request Access*).
- **Platform admins** — users whose email is in `PLATFORM_ADMIN_EMAILS` (falls back to `BOOTSTRAP_ADMIN_EMAIL`) *and*
  who hold the Tenant Administrator role — review requests. Nobody else sees the section or can call its API.
- Nothing is activated automatically.

## The form
Business name, your name, phone, number of branches, estimated users, email, type of business, town/location and an
optional message.
After submitting, the visitor sees: *Access request submitted successfully. We will review your request and get back
to you. In case of any delays, please call 0798 993 404 / 0732 968 898* (tap-to-call).

Safeguards: a hidden honeypot field drops bot submissions; at most 5 requests per connection per hour; one pending
request per email (a repeat is acknowledged but not duplicated); an email that already has a login is told to sign in.

## Notifications, review and first sign-in
Emails (owner, applicant acknowledgement, welcome with a set-up link, rejection), delivery tracking, the *Business
Activated* modal, *View Login Details*, *Resend Welcome Email*, *Issue New One-Time PIN*, WhatsApp and the first
sign-in are described in [module 24](24-onboarding-and-recovery.md). In-app notifications go to every platform admin.

Tabs Pending · Approved · Rejected · All. **Approve & create** creates the business exactly like a first start (default
roles, *Main Branch*, workflow rows, expense categories, award period, a unique ordering-link slug) and its Tenant
Administrator. **Reject** takes an optional reason for the applicant and an optional internal note.
Both decisions are written to the audit trail (`platform.approve_access` / `platform.reject_access`).
