# Development guide

## Prerequisites

Rust (stable, 1.80+), Node 22+, PostgreSQL 16 (Docker is easiest), Git. Optional: Python 3 for the smoke test.

## First run

```bash
docker run -d --name sshop-pg -e POSTGRES_PASSWORD=sshop -e POSTGRES_DB=sshop -p 55432:5432 postgres:16-alpine
cp .env.example .env
#   DATABASE_URL=postgres://postgres:sshop@localhost:55432/sshop
#   JWT_SECRET=<32+ random characters>
#   BOOTSTRAP_ADMIN_EMAIL / BOOTSTRAP_ADMIN_PIN = your first administrator
cd web && npm install && npm run build && cd ..
cargo run -p sshop
```

Open http://localhost:8080. The first start applies migrations and creates the business, default roles, a *Main
Branch*, expense categories and an open award period.

### Live-reload UI

```bash
cargo run -p sshop                 # API on :8080
cd web && npm run dev              # UI on :5173, proxies /api to :8080 (override with API_URL)
```

## Commands

| Command | Purpose |
|---|---|
| `cargo run -p sshop` | Run the server |
| `cargo run -p sshop -- reset-pin <email> <pin>` | Recover a locked-out administrator |
| `cargo run -p sshop -- import-legacy <db-url> [--tenant <slug>]` | Import Pablo Loyalty data |
| `cargo test` | Unit tests (pricing/points rules, mobile parsing, signatures, callbacks, settings) |
| `cd web && npm run typecheck && npm run lint` | UI static checks |
| `python scripts/smoke_test.py <url> <email> <pin>` | End-to-end checks of every critical flow (**scratch database only** — it creates data) |

## Conventions

- **One route module per functional area** (`server/src/routes/*.rs`), mirrored by `web/src/pages/<area>/`.
- **Every write that changes money, stock or points runs in one transaction** and writes an `audit::record`.
- **Stock only changes through `inventory::apply`.** Never `UPDATE stock_levels` directly.
- **Points only change through `loyalty.rs`** (ledger + customer totals together).
- **Sensitive actions** call `workflow::needs_approval` and, when gated, `workflow::submit` with the request as
  payload; the module's `on_approved` executes it later.
- **Migrations are append-only.** Never edit a migration that has run anywhere; add `0002_….sql`.
- **No hard deletes** of financial/operational records — use status changes.
- **Settings**: add a field with a default in `settings.rs` and in `web/src/lib/types.ts`; no migration needed.
- UI: mobile-first; use `DataList` (table ↔ cards), `ResponsiveDialog` (drawer ↔ dialog), `.num` for figures.

## Adding a module (checklist)

1. Migration `server/migrations/000N_<name>.sql`.
2. `server/src/routes/<name>.rs` with `routes()`; register in `routes/mod.rs`.
3. Permissions in `perms.rs` (+ default roles), workflow action in `workflow.rs` if needed.
4. Page(s) in `web/src/pages/<name>/`, route in `App.tsx`, nav entry in `components/layout/nav.ts`.
5. Report template in `routes/reports.rs` if relevant.
6. Smoke-test steps in `scripts/smoke_test.py`.
7. Docs: `docs/modules/NN-<name>.md`, [scope.md](scope.md) status, [api.md](api.md).
