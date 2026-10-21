"""Documentation check (roadmap 146) — run in CI and locally: `python scripts/docs_check.py`.

Fails when the documentation drifts from the repository in ways a machine can see:
  1. a module doc in docs/modules/ is not listed in the docs index (docs/README.md);
  2. a relative link in the README or docs points to a file that does not exist;
  3. docs/user-guide.md was not regenerated after docs/manual/manual.json changed (version or guide tasks differ);
  4. a migration exists in server/migrations that architecture.md's "checked against" line does not cover.
It cannot judge whether the words are right — that is the Definition of done in docs/development.md.
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
problems: list[str] = []

# 1. Every module doc is indexed.
index = (DOCS / "README.md").read_text(encoding="utf-8")
for doc in sorted((DOCS / "modules").glob("*.md")):
    if f"modules/{doc.name}" not in index:
        problems.append(f"docs/README.md does not list modules/{doc.name}")

# 2. Relative links resolve.
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")
pages = [ROOT / "README.md", *DOCS.rglob("*.md")]
for page in pages:
    text = page.read_text(encoding="utf-8")
    text = re.sub(r"```.*?```", "", text, flags=re.S)  # code blocks are not links
    for target in LINK.findall(text):
        if re.match(r"^[a-z]+:", target) or target.startswith("#"):
            continue
        path = target.split("#", 1)[0]
        if path and not (page.parent / path).resolve().exists():
            problems.append(f"{page.relative_to(ROOT)}: broken link → {target}")

# 3. The Markdown user guide matches its source.
manual = json.loads((DOCS / "manual" / "manual.json").read_text(encoding="utf-8"))
guide = (DOCS / "user-guide.md").read_text(encoding="utf-8") if (DOCS / "user-guide.md").exists() else ""
if manual["version"] not in guide:
    problems.append("docs/user-guide.md is older than docs/manual/manual.json — run node scripts/manual/build.mjs")
for g in manual.get("guides", []):
    for task in g["tasks"]:
        if f"#### {task['title']}" not in guide:
            problems.append(f"docs/user-guide.md misses the task “{task['title']}” — run node scripts/manual/build.mjs")
for m in manual["modules"]:
    if m["name"] not in guide:
        problems.append(f"docs/user-guide.md misses the module “{m['name']}” — run node scripts/manual/build.mjs")

# 4. architecture.md was checked against the latest migration.
latest = sorted((ROOT / "server" / "migrations").glob("*.sql"))[-1].name[:4]
arch = (DOCS / "architecture.md").read_text(encoding="utf-8")
m = re.search(r"migrations 0001–(\d{4})", arch)
if not m or m.group(1) != latest:
    problems.append(f"docs/architecture.md: update its tables for migration {latest} and the 'checked against' line")

if problems:
    print("Documentation check failed:\n  - " + "\n  - ".join(problems))
    sys.exit(1)
print(f"Documentation check passed ({len(pages)} pages, {sum(len(g['tasks']) for g in manual.get('guides', []))} guide tasks).")
