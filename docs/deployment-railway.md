# Deploying S'Shop on Railway

S'Shop runs as **one service** (built from the repository `Dockerfile`) plus **one PostgreSQL database**.

| Item | Value |
|---|---|
| GitHub repository | `https://github.com/Jemwanzs/S-Shop` (branch `main`) |
| Railway project | `c7f30712-2adc-403b-bf4b-42c83d5f8dae` |
| Build | `Dockerfile` (selected by `railway.json`) |
| Health check | `GET /healthz` |

## 1. Services

1. In the Railway project add **PostgreSQL** (Database → PostgreSQL).
2. Add a service from **GitHub repo → Jemwanzs/S-Shop**. Railway detects `railway.json` and builds the Dockerfile.
   Every push to `main` redeploys.
3. Settings → Networking → **Generate Domain** (or attach your own domain).

## 2. Variables (service → Variables)

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
| `MAIL_FROM` | Optional; sender, default `S'Shop <onboarding@resend.dev>` |
| `PEXELS_API_KEY` | Optional; product photos for the demo business ([module 20](modules/20-platform-and-demo.md)) |
| `ACCESS_REQUEST_NOTIFY_EMAILS` | Optional; who is emailed about requests (default: the platform admins) |

Bootstrap runs only when the database is empty; the variables can be removed afterwards (set `PLATFORM_ADMIN_EMAILS` first). Optional M-Pesa and WhatsApp
variables are listed in [`.env.example`](../.env.example) and explained in
[integrations/mpesa.md](integrations/mpesa.md) and [integrations/whatsapp.md](integrations/whatsapp.md).

## 3. First deploy checklist

- [ ] Deploy succeeds and `/healthz` returns `ok`.
- [ ] Sign in with the bootstrap administrator; change the PIN (avatar → Change PIN).
- [ ] Settings → Business profile: name, logo, ordering-link slug, phone.
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
