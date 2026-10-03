# S'Shop

**Mobile-first retail, inventory, sales & customer-loyalty platform.**
S'Shop is the successor of *Pablo Loyalty*: loyalty is now one module inside a complete retail operations system where
**inventory is the backbone, sales and orders drive stock movement, customers connect the transactions, and loyalty
strengthens retention**.

| | |
|---|---|
| **API server** | Rust · Axum 0.8 · Tokio · SQLx 0.8 · PostgreSQL 16 |
| **Web app** | React 18 · TypeScript · Vite · Tailwind CSS · TanStack Query |
| **Real-time** | Server-Sent Events (`/api/events`) |
| **Integrations** | M-Pesa Daraja STK Push · WhatsApp Cloud API |
| **Hosting** | Railway — one Docker service + Railway PostgreSQL |

## Modules

Dashboard & analytics · Sales / POS · Credit sales · Orders · Customer ordering link · Products · Stock & inventory
(barcodes, adjustments, stock take) · Transfers · Customers · Loyalty & rewards · Expenses · Reports (PDF/Excel) ·
Branches · Users, roles & permissions · Approval workflows · Settings · Audit trail · Notifications · Universal search.

Each is documented in [`docs/modules/`](docs/modules/).

## Repository layout

```
S-Shop/
├── server/                 Rust API (also serves the built web app)
│   ├── migrations/         PostgreSQL schema (applied automatically at start-up)
│   └── src/
│       ├── main.rs         start-up, CLI (serve | reset-pin | import-legacy)
│       ├── inventory.rs    inventory ledger (stock movements + locked levels)
│       ├── loyalty.rs      points engine (earn, referral, redeem, reverse, expire)
│       ├── workflow.rs     maker-checker approval engine
│       ├── integrations/   M-Pesa Daraja, WhatsApp Cloud API
│       └── routes/         one file per module (HTTP API)
├── web/                    React web app (staff app + public ordering portal)
├── scripts/smoke_test.py   end-to-end test against a running server
├── docs/                   documentation (start at docs/README.md)
├── Dockerfile · railway.json · .env.example
└── _archive/               backup of the legacy Pablo Loyalty code (not deployed)
```

## Quick start (local)

Prerequisites: Rust 1.80+, Node 22+, PostgreSQL 16 (or Docker).

```bash
docker run -d --name sshop-pg -e POSTGRES_PASSWORD=sshop -e POSTGRES_DB=sshop -p 55432:5432 postgres:16-alpine
cp .env.example .env            # set DATABASE_URL=postgres://postgres:sshop@localhost:55432/sshop
cd web && npm install && npm run build && cd ..
cargo run -p sshop              # http://localhost:8080 — sign in with BOOTSTRAP_ADMIN_EMAIL / PIN
```

For live-reload UI work run `cargo run -p sshop` and, in `web/`, `npm run dev` (http://localhost:5173, proxies `/api`).
Full details: [docs/development.md](docs/development.md).

## Deploy

See [docs/deployment-railway.md](docs/deployment-railway.md).

## Tests

```bash
cargo test                                                        # unit tests
cd web && npm run typecheck && npm run lint                       # UI checks
python scripts/smoke_test.py http://localhost:8080 admin@example.com 1234   # end-to-end (scratch DB only)
```
