# Production readiness — full stack, multi-tenancy, Railway (2026-10-06)

Audited against the code, the database and the live Railway project (S'Shop: services `sshop` + `Postgres`,
environment *production*). ✅ complete · 🟡 partial · 🔴 missing.

## Summary

| Area | Status | Evidence | Gap |
|---|---|---|---|
| Frontend | ✅ | React 18 + TypeScript + Vite; every screen reads and writes through `/api` (`web/src/lib/api.ts`); no mock or hard-coded business data. | — |
| Backend | ✅ | Rust (Axum + SQLx); all business rules on the server: prices/discounts, stock ledger, barcode clearance, loyalty, approvals, credit, business date, geofencing. The browser never decides. | — |
| PostgreSQL schema | ✅ | 10 append-only migrations run at start-up (`sqlx::migrate!`); foreign keys, CHECK constraints (statuses, quantities, coordinates), unique keys (document numbers, active barcodes), 40+ indexes, DB triggers for the business date. | Cross-tenant foreign keys are not enforced by the DB (see *Hardening*). |
| Data integrity | ✅ | Every write in one transaction with its audit row; row locks on stock items, transfers, credit, settings; idempotent sale submit (`client_ref`); concurrent sale of one unit verified (one wins). | — |
| Railway — app | ✅ | Built from `Dockerfile` via `railway.json` (health check `/healthz`, restart on failure), non-root container, auto-deploy from `main`. | No CI gate before deploy (see below). |
| Railway — database | 🟡 | Postgres 18 on a 5 GB volume, private network only (no public TCP proxy). | **No backups visible** — enable scheduled backups. Local dev uses Postgres 16. |
| Railway — variables | 🟡 | `DATABASE_URL`, `JWT_SECRET`, `PLATFORM_ADMIN_EMAILS`, `RESEND_API_KEY`, bootstrap vars set; secrets never stored in the database or shown in Settings. | `PUBLIC_URL` and `MAIL_FROM` not set: links use `sshop-live.up.railway.app`, and email goes from Resend's test sender (`onboarding@resend.dev`), which only delivers to the Resend account's own address. |
| Domain | 🟡 | `sshop-live.up.railway.app` live with TLS. | `sshop.io` is attached but has **no DNS records** (CNAME + TXT verification pending). |
| Region | 🟡 | App and database in `us-west2` (together, so queries are local). | Users in Kenya are ~250 ms away; an EU region (Amsterdam) would roughly halve page latency. |
| Multi-tenancy | ✅ | Tenant comes only from the signed token (`Ctx`), never from the request body; every query filters `tenant_id`; branch access checked on every request; platform admins "open" a business with an audited acting session, re-verified each request. Demo business isolated (`is_demo`, no WhatsApp/email). | Defence in depth (see *Hardening*). |
| Roles & permissions | ✅ | 56 `module.action` permissions enforced on the server (UI hides what the server would refuse); settings per area; no self-escalation; own-data scoping (`staff.view_others`). | — |
| Authentication | ✅ | Argon2 PIN hashes, 12 h tokens, account lockout after failed attempts, timing-equalised unknown-email login, portal OTP with attempt and rate limits. | No per-IP rate limit on sign-in / access-request forms (lockout is per account). |
| Audit trail | ✅ | Before/after JSON, user, branch, IP, device, location, approval link — written in the same transaction as the change. | — |
| Browser security | ✅ *(this pass)* | Strict Content-Security-Policy (no inline script), HSTS, `nosniff`, `X-Frame-Options: DENY`, referrer and permissions policies — verified with no CSP violations across all screens, PDF export and the camera. | Session token is kept in browser storage (mitigated by the strict CSP). |
| Browser storage | ✅ | Only: session token + chosen branch, theme, language, unsent till cart and portal cart (all per device). No business data lives in the browser. | — |
| Automated tests | 🟡 | 209 end-to-end API checks (`scripts/smoke_test.py`) + unit tests, run by hand before each push. | **No CI**: nothing runs automatically before Railway deploys. |
| Observability | 🟡 | Structured logs (`RUST_LOG`), Railway deploy/HTTP logs. | `/healthz` does not check the database; no error alerting. |
| Scale-out | 🟡 | One replica: background jobs and live events (SSE) run in-process. | A second replica would run jobs twice and split live events — needs a DB lock for jobs and Postgres `LISTEN/NOTIFY` for events before scaling out. |
| File storage | 🟡 | Logos and product photos stored in Postgres (`bytea`), served with long cache headers. | Fine now; move to object storage (Railway Bucket) as photo volume grows. |
| Offline selling | 🔴 | — | Roadmap item (offline POS / PWA). |

## Genuine gaps — best order to close them

1. **Database backups** (Railway → Postgres → Backups: daily, keep 7+). Nothing else protects the data if the volume is lost. *Dashboard setting.*
2. **Email sender + public URL**: verify a sending domain in Resend, set `MAIL_FROM` (e.g. `S'Shop <no-reply@sshop.io>`), set `PUBLIC_URL`. Without it, access-request emails reach only the Resend account owner. *Variables + DNS.*
3. **`sshop.io` DNS**: add the CNAME (`ckyv3su4.up.railway.app`) and the `_railway-verify` TXT record at the registrar, then set `PUBLIC_URL=https://sshop.io`.
4. **CI before deploy**: GitHub Actions running `cargo test`, type-check, lint, web build and the smoke suite against a Postgres service; turn on Railway *Wait for CI* so a failing build never deploys. *Code.*
5. **Hardening**: cross-tenant attack tests in the smoke suite (a user of business B requesting A's ids → 404 on every resource); `tenant_id` on every id-based write as a second guard; per-IP rate limit on sign-in and public forms; database check in `/healthz`. *Code.*
6. **Region**: move app + database together to an EU region when convenient (needs a short maintenance window and a backup/restore).
7. **Before running two replicas**: job lock + `LISTEN/NOTIFY` events. Not needed at one replica.
8. **Offline POS (PWA)** — on the roadmap.

Items 1–3 are settings on Railway, Resend and the domain registrar (owner action); 4–5 are code and come next.
