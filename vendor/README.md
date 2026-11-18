# vendor/ — PostgreSQL only

S'Shop stores everything in PostgreSQL. The database library, `sqlx` 0.8, also declares optional **SQLite** and
**MySQL** drivers. They were never built into the server, but Cargo records every optional dependency in `Cargo.lock`,
so their dependencies (the bundled SQLite C library `libsqlite3-sys`, and `rsa`) were listed there and reported by
`cargo audit` (RUSTSEC-2026-0459, RUSTSEC-2023-0071).

`sqlx-sqlite` and `sqlx-mysql` here are empty stand-ins, wired in by `[patch.crates-io]` in the workspace `Cargo.toml`.
With them, neither driver nor its dependencies is in `Cargo.lock` or the build, and nothing can use SQLite or MySQL:
each stand-in is a single `compile_error!`, so enabling sqlx's `sqlite` or `mysql` feature fails the build at once.

**When upgrading sqlx:** set both stand-ins' `version` to the new sqlx version and copy the real drivers' feature
names into their `[features]` (they must match what sqlx refers to). CI checks that `Cargo.lock` holds no SQLite
or MySQL packages (`scripts/postgres_only_check.py`).
