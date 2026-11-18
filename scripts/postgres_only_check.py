"""Roadmap 166: S'Shop is PostgreSQL only. Fails if Cargo.lock holds SQLite or MySQL code again, or if sqlx's driver
stand-ins in vendor/ are no longer the empty crates Cargo uses (for example after a sqlx upgrade; see vendor/README.md)."""
import re
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
lock = (root / "Cargo.lock").read_text(encoding="utf-8")
packages: dict[str, list[str]] = {}
for block in lock.split("[[package]]")[1:]:
    name = re.search(r'^name = "([^"]+)"', block, re.M)
    if name:
        packages.setdefault(name.group(1), []).append(block)

problems = []
for banned in ("libsqlite3-sys", "rsa", "mysql_async", "mysql_common"):
    if banned in packages:
        problems.append(f"{banned} is in Cargo.lock: S'Shop is PostgreSQL only")

sqlx = re.search(r'^name = "sqlx"\nversion = "([^"]+)"', lock, re.M)
for driver in ("sqlx-sqlite", "sqlx-mysql"):
    for block in packages.get(driver, []):
        if "source = " in block:
            problems.append(f"{driver} comes from crates.io, not the empty stand-in vendor/{driver}")
        if "dependencies = [" in block:
            problems.append(f"{driver} has dependencies: it must be the empty stand-in")
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        if sqlx and version and version.group(1) != sqlx.group(1):
            problems.append(f"vendor/{driver} is {version.group(1)} but sqlx is {sqlx.group(1)}: update the stand-in")
    crate = root / "vendor" / driver
    if "compile_error!" not in (crate / "src" / "lib.rs").read_text(encoding="utf-8"):
        problems.append(f"vendor/{driver} must stay an empty crate that refuses to compile")
    if re.search(r"^\[dependencies\]", (crate / "Cargo.toml").read_text(encoding="utf-8"), re.M):
        problems.append(f"vendor/{driver} must not have dependencies")

if problems:
    print("\n".join(problems))
    sys.exit(1)
print("PostgreSQL only: no SQLite or MySQL code in Cargo.lock")
