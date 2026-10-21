# 40. Error alerting for the platform owner (roadmap 147)

Failures that need someone to act reach the platform owner — in the S'Shop bell and by email — instead of staying in
Railway's logs. Settings → Platform → **Errors & alerts** lists them.

## What alerts

| Source | Examples |
|---|---|
| Server | A database error or internal error answering a request (500); a crash (panic) inside a request — now answered with a 500 instead of a dropped connection |
| Outside services | An integration refusing (502 *upstream*): Railway (e.g. *custom-domain limit for the plan*), Paystack, Daraja, DNS |
| Background jobs | The housekeeping run (every 15 min), billing renewals and reminders, the M-Pesa payment check (every minute) |
| Health | *Service restarted* — a second start of the same Railway deployment (crash, out of memory; `RAILWAY_DEPLOYMENT_ID`, which Railway injects); *Database not answering* on the health check (kept in memory and stored once the database answers) |

**Never alerted:** normal business answers (400, 401, 403, 404, 409, 422, 429 — wrong PIN, validation, permissions,
rules) and a business website shown as *temporarily unavailable* (a deliberate 503 when it is not published or is
suspended).

## Grouping and repeats

- One entry per distinct error: same source, kind, message shape and place; ids and numbers are ignored, so the same
  failure on another record groups with it.
- The first occurrence alerts. Repeats only increase the count. It alerts again when it is still happening 6 hours
  after the last alert, or when it comes back after being marked **Resolve**d.
- Reports never slow a request: a background writer stores them; while the database is down up to 500 are kept in
  memory.

## Delivery

- **In-app**: a bell notification for every platform administrator (`PLATFORM_ADMIN_EMAILS`), linking to Errors & alerts.
- **Email**: to `PLATFORM_ADMIN_EMAILS` through Resend, recorded in the email log (kind `error_alert`). Until
  `s-shop.store` is verified in Resend (roadmap 29), Resend only delivers to the Resend account owner.
- **Send test alert** (on the page) sends one through the real path and says whether the email went out.

## Privacy and security

- Messages are shortened (400 characters) and anything that looks like a secret (long tokens, `Bearer …`, `key=…`)
  is replaced by `[removed]`. Request bodies and headers are never stored.
- Only platform administrators, fully signed in, can list, resolve or test (`require_platform_admin`); business
  administrators and staff get 403.
- Resolved entries and service start records are kept 90 days.

## Acceptance criteria (tested)

- Smoke suite: 4xx answers create no entry; a test alert creates an entry, a bell notification and a sent email; two
  test alerts are one entry with a count; Resolve moves it to *Resolved*; it reopens and alerts again when it
  happens after Resolve; a business administrator and a salesperson get 403.
- Unit tests: a panic answers 500 and carries the alert; business errors never alert, faults always do; secrets and
  length are removed; the same error at another record groups; re-alert only after 6 h or after Resolve.

## API

| Method | Path |
|---|---|
| GET | `/api/platform/errors?status=open\|resolved\|all` — entries (≤ 200, latest first), `open` count, whether email is configured, masked recipients, last service start |
| POST | `/api/platform/errors/test` — test alert through the real path; `{id, email: {status, error}}` |
| POST | `/api/platform/errors/{id}/resolve` — 404 when not open |
