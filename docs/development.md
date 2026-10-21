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
| `cargo run -p sshop -- seed-demo [--reset]` | Build (or rebuild) the Pablo Niche demo business |
| `cargo test` | Unit tests (pricing/points rules, mobile parsing, signatures, callbacks, settings) |
| `cd web && npm run typecheck && npm run lint` | UI static checks |
| `python scripts/smoke_test.py <url> <email> <pin>` | End-to-end checks of every critical flow (**scratch database only** — it creates data). CI runs it on every push with stand-ins for Paystack, Resend, Daraja and exchange rates; see `.github/workflows/ci.yml` for the server variables |
| `node scripts/manual/build.mjs` | Rebuild the Product User Manual PDF from `docs/manual/manual.json` ([module 31](modules/31-product-user-manual.md)) |
| `python scripts/docs_check.py` | Documentation check (also in CI): every module doc indexed, no broken links between docs |

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
- **Workspace links:** import `Link`, `NavLink`, `Navigate`, `useNavigate` and `useLocation` from `@/lib/router`, never
  from `react-router-dom` — it adds the business slug to every workspace address ([module 39](modules/39-workspace-addresses.md)).
  The public website app (`web/src/site`) is the exception.
- **Business dates:** business days follow the business's time zone (EAT for Kenya) while CI runs in UTC — tests never
  put the machine's UTC date into business dates.

## Adding a module (checklist)

1. Migration `server/migrations/000N_<name>.sql`.
2. `server/src/routes/<name>.rs` with `routes()`; register in `routes/mod.rs`.
3. Permissions in `perms.rs` (+ default roles), workflow action in `workflow.rs` if needed.
4. Page(s) in `web/src/pages/<name>/`, route in `App.tsx` (inside the `/:biz` workspace route), nav entry in
   `components/layout/nav.ts`; add a new first path segment to `SCREENS` in `lib/router.tsx` and to
   `RESERVED_SLUGS` in `server/src/util.rs`.
5. Report template in `routes/reports.rs` if relevant.
6. Smoke-test steps in `scripts/smoke_test.py`.
7. Docs — see *Definition of done* below.

## Definition of done (every feature, fix or removal)

A change is done only when it is **built, tested, deployed, verified live and documented**. Documentation is part of
the change, in the same commit or the follow-up *marked done* commit — never "later".

| When you… | Update |
|---|---|
| plan or accept work | [scope.md](scope.md): a roadmap entry (⏳) with the purpose, scope and **acceptance criteria**; the *Status board* |
| ship it | the entry → ✅ only after CI is green, the deploy is live and the behaviour was checked on s-shop.store; the *Status board* |
| add or change behaviour | the module doc in [modules/](modules/) (create `NN-<name>.md` + index it in [README.md](README.md) for a new module) |
| add or change an endpoint | [api.md](api.md) |
| add a table, column, migration, component, integration or security rule | [architecture.md](architecture.md) |
| add or change an environment variable or Railway setting | [deployment-railway.md](deployment-railway.md), `.env.example`, `.railway/railway.ts` (`preserve()`) |
| change anything a user sees | [manual/manual.json](manual/manual.json) — feature bullets and the step-by-step role guides — then rebuild the PDF and [user-guide.md](user-guide.md) (`node scripts/manual/build.mjs`) |
| verify it live | [release-notes.md](release-notes.md): one line under the date |
| remove or replace something | delete or rewrite the old instructions everywhere above (no outdated steps left); keep the roadmap history |

Keep history: completed roadmap entries and decisions are never deleted, only marked superseded. Dated reports
(`*-2026-10.md`) are snapshots and are not edited after the fact.

Acceptance criteria are written as checks a person could run ("a cashier without `barcodes.reveal` sees `•••4821`"),
and the smoke test or browser test encodes them.
