# 24 — Onboarding emails, one-time PINs, self-service reset (roadmap 58–61)

What happens from *Request Access* to a business's first sign-in, and how any user gets back in without an
administrator.

**Request Access → owner emailed + in-app → applicant acknowledged → Approve & create → welcome email with a set-up
link (+ one-time PIN shown once) → optional WhatsApp → first sign-in → own PIN.**

| Part | Where |
|---|---|
| Templates, sending with a delivery log, single-use links | `server/src/mailer.rs` |
| Requests, approval, rejection, login details, resend, new one-time PIN, retry | `server/src/routes/access.rs` |
| Forgot PIN, set PIN from a link, applicant status, Resend webhook | `server/src/routes/recovery.rs` |
| One-time-PIN gate, session revocation | `server/src/auth.rs` (`Ctx`), `routes/auth.rs` (login, change PIN) |
| Database | `migrations/0018_onboarding_email_reset.sql` (`auth_tokens`, `email_log`, user PIN state) |
| Screens | `/forgot`, `/set-pin`, `/access-status`, first-sign-in screen (`pages/auth/Recovery.tsx`); Settings → Platform → Access requests; business page → Notification history |

## 58 — Emails and their delivery

| Email | To | When |
|---|---|---|
| Access Request Received | `ACCESS_REQUEST_NOTIFY_EMAILS` (default: the platform admins) | New request: business, applicant, email, mobile, type, branches, **estimated users**, location, date/time (EAT), message, *Review request* button |
| Applicant Acknowledgement | Applicant | New request: received, under review, no need to resubmit |
| Welcome Email | New administrator | Approval and every *Resend*: business, administrator, email, sign-in URL and a **single-use set-up link (72 h)**. Never the PIN |
| Account Rejected | Applicant | Rejection: courteous, with the optional *reason for the applicant*. The internal note is never sent |
| PIN Reset Link | User | *Forgot PIN / Password?* (link valid 30 min) |
| PIN Changed | User | After a PIN is set from a link |
| Request Status Link | Applicant | *Forgot PIN / Password?* with an applicant's email (link valid 30 min) |

- **Template:** responsive HTML (tables and inline styles, so it works in all mail apps) with a plain-text
  alternative, S'Shop branding, and an ETR-receipt-style details block.
- **Delivery:** through Resend from the server only; the API key stays in Railway variables.
- **Logging:** every email is a row in `email_log` (kind, recipient, subject, status, Resend id, error, attempts,
  who, retry-of). The body, PINs and links are never stored.
- **Statuses:**
  - `queued → sent | failed | skipped` (*skipped* = email not configured).
  - With `RESEND_WEBHOOK_SECRET` set, Resend's events then move a sent email to `delivered`, `delayed`, `bounced` or
    `complained`.
  - Events arrive at `POST /api/webhooks/resend`. They are verified with the Svix HMAC signature; unsigned events and
    events older than 5 minutes are refused.
- **Retry** (platform owner): received, acknowledgement, rejection and welcome emails can be retried. Content is
  rebuilt from current data, and a welcome retry gets a fresh link.
- **A failed email never undoes anything.** The approval stands, and the modal shows the email status with the
  provider's error.
- **No duplicates:**
  - one pending request per email, and a repeated submission sends nothing;
  - approval locks the request row, so *Approve* twice → *already approved*;
  - the duplicate-request guard (roadmap 47) covers double clicks;
  - resend, retry and *Issue PIN* are rate-limited (5 per 10 minutes per request).

**Notification history** — Notification Type · Recipient · Date/Time · Delivery Status · Retry. It appears in
*View Login Details* on an approved request and on the business's page.

## 59 — Approval, login details, one-time PINs

- **Business Activated modal:** business, administrator email, login URL, the **one-time PIN (shown once)** with its
  expiry, and the email status (Sent / Pending / Failed / Not sent). Actions:
  - **Resend Email**;
  - **Copy Message**;
  - **WhatsApp:** opens `wa.me/<254…>` with the welcome message already composed. That is WhatsApp Web on desktop and
    the app on phones, with the recipient taken from the request's mobile number. The platform owner presses Send;
    S'Shop never claims it was sent. The message deliberately excludes the PIN, because a chat is not a one-time
    channel.
- **One-time PINs** (approval, *Issue New One-Time PIN*, platform PIN reset):
  - hashed (Argon2) like every PIN;
  - expire after 72 hours;
  - must be replaced at first sign-in;
  - replace the previous PIN and end older sessions;
  - audited without the PIN.
- **First sign-in with a one-time PIN:** the app shows *Create your own PIN*. The server refuses every other request
  (title *Set your own PIN*) until it is done. An expired one-time PIN is refused with directions to *Forgot PIN* or
  support.
- **Approved tab, per request:**
  - **View Login Details:** a receipt-style slip with business, administrator, email, mobile, login URL, approval
    date, activation status (*Active · Awaiting first sign-in · One-time PIN expired · Deactivated*), last sign-in and
    email delivery, plus the notification history. The PIN is never shown, because it is not stored.
  - **Resend Welcome Email:** a fresh link; the previous link stops working.
  - **Issue New One-Time PIN:** confirm, then the PIN is shown once with *Copy* and *Call administrator* (read it out,
    don't post it).
  - **WhatsApp.**
- **Rejecting:** optional *reason for the applicant*, which is emailed, and an *internal note*, which is never sent.

## 60 — Forgot PIN / Password (every user)

1. **Sign in → Forgot PIN / Password?** The user enters their email and always sees the same answer: *If this email is
   registered, we'll send you a secure link…*. They can go *Back to Sign In*, or *Resend Link* after 60 seconds.
2. A registered, active user receives a reset link. Someone with no account but an access request receives a status
   link instead (61).
3. On the link page (`/set-pin`):
   - It shows only the purpose, the first name, a masked email (`l•••e@gmail.com`) and the business.
   - The user sets a new PIN twice, under the PIN policy (4–12 characters).
   - The link is used up and other open links stop working.
   - All sessions end and a *PIN changed* email is sent.
   - The event is audited (`auth.reset_requested`, `auth.reset_completed` / `auth.setup_completed`).
4. **The same flow covers everyone:** Platform Owner, Tenant Admins, managers, cashiers. No administrator approval is
   needed. Deactivated accounts get nothing, because their administrator restores them.

**Security:**
- **Tokens:** 256-bit random. Only their SHA-256 is stored. They travel in the URL fragment (`#t=`), which browsers
  never send to servers, so they are kept out of access logs. The page removes the token from the address bar once it
  is read.
- **Expiry and use:** links expire (30 min for reset, 72 h for set-up), work once, and a newer link replaces an older
  one.
- **Rate limits:** 3 emails per address per 15 minutes; 10 requests per connection per 15 minutes.
- **No account discovery:** the response and its timing are identical for unknown emails, because the work happens
  after the response.
- **Changing your own PIN** (More → Change PIN) also ends your other sessions; the current device continues with a
  fresh token.
- **No access to the email:** *Contact S'Shop Support* (0798 993 404 / 0732 968 898, Call / WhatsApp). The platform
  owner verifies the person, then uses *Issue New One-Time PIN* (for administrators) or the business page's *Reset
  PIN*. A business administrator can only reset users of their own business; nobody can reset another business's
  users except the platform owner.

## 61 — Applicant status at sign-in

Signing in with an applicant's email never says *pending* or *rejected* directly. Doing so would let anyone find out
who applied. Instead:

1. A failed sign-in shows the neutral *Invalid email or PIN* with **Applied for access? Check your request status**,
   which leads to the same Forgot page.
2. The applicant receives a **status link** by email, which proves they own the address.
3. `/access-status` shows one of:
   - **Pending** — *Your access request is currently under review. Please allow us a little time to complete the
     approval process.*
   - **Rejected** — *Your access request could not be approved at this time. Please contact S'Shop Support for
     assistance or further clarification.* Internal notes are never shown.
   - **Approved, not yet set up** — *Your S'Shop access has been approved. Please check your email for your account
     setup instructions*, with **Resend Setup Instructions** (3 per hour per request). The answer is the same whether or
     not an email was sent.
   - **Approved and active** — a *Sign in* link.

   Every status shows **Need assistance? Contact S'Shop Support — 0798 993 404 | 0732 968 898** with Call and WhatsApp.

## Configuration (Railway)

| Variable | Purpose |
|---|---|
| `RESEND_API_KEY` | Sending (already set) |
| `MAIL_FROM` | Sender, e.g. `S'Shop <noreply@s-shop.store>`. **The domain must be verified in Resend**: Resend's test sender `onboarding@resend.dev` only delivers to the Resend account owner, so applicants and new administrators would not receive anything (roadmap 29) |
| `MAIL_REPLY_TO` | Optional; where replies go (e.g. a support mailbox) |
| `RESEND_WEBHOOK_SECRET` | Optional; Resend → Webhooks → endpoint `https://s-shop.store/api/webhooks/resend`, events *sent, delivered, delivery_delayed, bounced, complained, failed*; paste the signing secret (`whsec_…`) |
| `ACCESS_REQUEST_NOTIFY_EMAILS` | Optional; who receives new-request emails (default: `PLATFORM_ADMIN_EMAILS`) |

## Tests

The smoke suite serves a Resend stand-in (`RESEND_BASE_URL`, test-only) and reads every email back. Section
*Roadmap 58–61* follows the whole journey (39 checks):

- owner and applicant emails, and no repeats;
- welcome email and link, with the PIN never in an email or the WhatsApp text;
- the one-time-PIN gate, set-up link single use and session revocation;
- *Issue New One-Time PIN*, resend, retry, and signed / unsigned delivery events;
- forgot PIN for users and applicants, and rate limits;
- pending / approved / rejected status pages without internal notes.

Unit tests cover token hashing, email masking, template escaping and Svix signatures.
