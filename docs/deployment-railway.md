# Deploying S'Shop on Railway

S'Shop runs as **one service** (built from the repository `Dockerfile`) plus **one PostgreSQL database**.

| Item | Value |
|---|---|
| GitHub repository | `https://github.com/Jemwanzs/S-Shop` (branch `main`) |
| Railway project | `c7f30712-2adc-403b-bf4b-42c83d5f8dae` |
| Build | `Dockerfile` (Railway uses a root `Dockerfile` automatically) |
| Railway settings | `.railway/railway.ts` (Infrastructure as Code, roadmap 94) — GitHub source, health check, variable names |
| Health check | `GET /healthz` — `ok` only when the database answers within 3 s (503 otherwise) |

## 1. Services

1. In the Railway project add **PostgreSQL** (Database → PostgreSQL).
2. Add a service from **GitHub repo → Jemwanzs/S-Shop**. Railway detects the root `Dockerfile` and builds it.
   Every push to `main` redeploys.
3. Settings → Networking → **Generate Domain** (or attach your own domain).

## Railway settings as code (`.railway/railway.ts`)

Since 2026-10-19 the service's Railway settings are kept in `.railway/railway.ts` (Railway Infrastructure as Code; the
older `railway.json` stopped being read on 2026-12-01). Railway does not read it during deploys — it is applied with the
Railway CLI:

```powershell
cd .railway; npm install; cd ..           # once: the `railway` SDK the file imports
railway link                              # project S'Shop, environment production, service sshop
railway config plan                       # read-only preview — read it
railway config apply                      # asks for confirmation
```

- **Anything the file leaves out is removed from Railway.** It declares the GitHub source and every variable as
  `preserve()` (keep the value already on Railway — values never go in the file). When a variable is added on
  Railway, add its name here as `preserve()` too, or the next apply deletes it. Never apply a plan that shows
  `destroy` unless that is intended.
- Windows with the npm-installed CLI: the SDK's CLI-version check fails unless `$env:_` points at the real
  `railway.exe` (`%APPDATA%/npm/node_modules/@railway/cli/bin/railway.exe`) and the command is run through it:
  `$env:_ = "<that path>"; & $env:_ config plan`.

## 2. Variables (service → Variables)

**Adding a variable:** set it on Railway, then add its name to `.railway/railway.ts` as `preserve()` and commit — otherwise
the next `railway config apply` deletes it (see above).

| Variable | Value |
|---|---|
| `DATABASE_URL` | `${{Postgres.DATABASE_URL}}` (reference variable) |
| `JWT_SECRET` | 48+ random characters (keep it stable — changing it signs everyone out) |
| `BOOTSTRAP_BUSINESS_NAME` | Your business name (first start only) |
| `BOOTSTRAP_ADMIN_EMAIL` | First administrator email |
| `BOOTSTRAP_ADMIN_PIN` | First administrator PIN — **set it yourself**, then change it in the app after first sign-in |
| `PUBLIC_URL` | Optional; defaults to `https://$RAILWAY_PUBLIC_DOMAIN` |
| `PLATFORM_ADMIN_EMAILS` | Comma-separated platform admins who review access requests (falls back to `BOOTSTRAP_ADMIN_EMAIL` — set it before removing the bootstrap variables) |
| `RESEND_API_KEY` | Optional; enables email alerts for access requests ([module 18](modules/18-access-requests.md)) |
| `MAIL_FROM` | Sender, e.g. `S'Shop <noreply@s-shop.store>` — **verify the domain in Resend first**; the default `S'Shop <onboarding@resend.dev>` only delivers to the Resend account owner, so onboarding and reset emails to anyone else fail ([module 24](modules/24-onboarding-and-recovery.md)) |
| `MAIL_REPLY_TO` | Optional; reply-to address on S'Shop emails |
| `RESEND_WEBHOOK_SECRET` | Optional; Resend webhook signing secret (`whsec_…`) for delivered / bounced statuses — endpoint `{PUBLIC_URL}/api/webhooks/resend` |
| `BACKGROUND_JOBS` | Test runs only: `off` stops the scheduler (renewals, trial ends, suspension, reminders). **Never set it in production.** |
| `PAYSTACK_SECRET_KEY` | Optional; enables *Pay now* on Settings → Billing ([module 21](modules/21-platform-billing.md)). Set the Paystack webhook URL to `{PUBLIC_URL}/api/webhooks/paystack` |
| `RAILWAY_API_TOKEN` | Optional; lets S'Shop attach businesses' verified website domains to this service automatically ([module 23](modules/23-website.md#56--custom-domains)). An account or team token; Railway supplies the project, environment and service ids itself. Without it, the platform owner adds each verified domain in Railway and records its CNAME target on the business page. Railway plans limit custom domains per service (Pro: 20 by default). |
| `PEXELS_API_KEY` | Optional; product photos for the demo business ([module 20](modules/20-platform-and-demo.md)) |
| `ACCESS_REQUEST_NOTIFY_EMAILS` | Optional; who is emailed about requests (default: the platform admins) |
| `WEBAUTHN_RP_ID` | Optional; the domain Face ID / biometric passkeys belong to (default: the host of `PUBLIC_URL`). Changing it makes existing passkeys stop working ([module 37](modules/37-face-id-biometric-login.md)) |
| `FX_URL`, `PAYSTACK_BASE_URL`, `RESEND_BASE_URL`, `MPESA_SANDBOX_URL`, `MPESA_PRODUCTION_URL` | **Test runs only**: point the exchange rates, Paystack, Resend and Daraja at the test suite's stand-in. Never set them in production |

Bootstrap runs only when the database is empty; the variables can be removed afterwards (set `PLATFORM_ADMIN_EMAILS` first). Optional M-Pesa and WhatsApp
variables are listed in [`.env.example`](../.env.example) and explained in
[integrations/mpesa.md](integrations/mpesa.md) and [integrations/whatsapp.md](integrations/whatsapp.md).

## 3. First deploy checklist

- [ ] Deploy succeeds and `/healthz` returns `ok`.
- [ ] Sign in with the bootstrap administrator; change the PIN (avatar → Change PIN).
- [ ] Settings → Business profile: name, logo, phone; the business address (slug) is set at creation and is the
  workspace URL, ordering link and website address ([module 39](modules/39-workspace-addresses.md)).
- [ ] Settings → Branches, Users, Roles; Stock / Sales / Loyalty options.
- [ ] Create products, receive opening stock (Stock → Receive stock → *Opening stock*).
- [ ] Optional: `import-legacy` (see [migration](migration-from-pablo-loyalty.md)) — run from a Railway shell:
  `sshop import-legacy "<supabase-connection-string>" --tenant <slug>`.
- [ ] Optional: M-Pesa callback and WhatsApp webhook configured with the public domain.

## Operations

- **Logs:** Railway → service → Deployments → Logs (structured `tracing` output).
- **Backups:** enable Railway PostgreSQL backups; the database is the only stateful component (photos and
  attachments are stored in it).
- **Admin recovery:** Railway shell → `sshop reset-pin <email> <new-pin>`.
- **Migrations** run automatically on start; a failed migration stops the deploy before traffic switches.
- **Scaling:** keep one replica until live events move to PostgreSQL `LISTEN/NOTIFY` (see architecture).
