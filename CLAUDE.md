# S'Shop — working rules

Rust (Axum/SQLx) + PostgreSQL + React/TypeScript, one service on Railway (https://s-shop.store, auto-deploys from
`main` after CI). Start every session with [docs/scope.md](docs/scope.md) — the **Status board** at the top says what
is live, in progress and open.

## Definition of done

A change is done only when it is built, tested, deployed, **verified live** and **documented**. Follow the table in
[docs/development.md → Definition of done](docs/development.md#definition-of-done-every-feature-fix-or-removal):
roadmap entry with acceptance criteria and the Status board ([scope.md](docs/scope.md)), module doc, API,
architecture, deployment, user manual and role guides (`docs/manual/manual.json` → `node scripts/manual/build.mjs`),
and one line in [release-notes.md](docs/release-notes.md). Remove outdated instructions; keep roadmap history.
CI runs `python scripts/docs_check.py`.

## Always

- Build continuously without breaking what works: no removal of functionality without the owner's confirmation;
  root causes, not patches; mobile-first, Outfit, light and dark.
- Run the smoke suite (`scripts/smoke_test.py`, scratch database only) and browser checks before pushing; CI is the
  full-suite gate. Commit and push after each completed feature.
- Never put secrets in code, docs, logs or commits. Credentials stay in Railway variables; business credentials are
  encrypted at rest. A tenant administrator never gains platform access or sees another tenant's data.
- Railway settings live in `.railway/railway.ts`: anything left out is **deleted** by `railway config apply`. Read
  every plan; production changes (apply, domains, service settings) are made with the owner.
- Workspace links come from `@/lib/router` (adds the business slug); business dates are in the business's time zone.
