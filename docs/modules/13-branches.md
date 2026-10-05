# 13 · Branches (multi-branch)

**Scope:** §2 · **Route:** Settings → Branches · **API:** `/api/branches…`

## What is per branch
Stock levels and values, sales, orders, expenses, credit, transfers, analytics. Customers and products are shared by
the business (products can be limited to selected branches).

## Managing branches
Name, code (unique), location, phone, **branch manager** (used by *Branch manager* approvals), active. At least one
branch must stay active. **Own trading hours** (optional) override the business hours for that branch and set its
business day (module 16); changing them needs `settings.workspace`. **Location** (latitude/longitude, *Use my
current location*), **radius** (20–5,000 m, default 150) and **geofencing on/off** — used when the business
requires work at the branch (module 16); changing them also needs `settings.workspace`. Users are assigned to one or several branches, or to all (Settings → Users).

## Current Branch
- Users with one branch go straight in; users with several choose “Where are you working today?” after sign-in.
- The Current Branch defaults sales, stock receipts, adjustments, expenses, transfers (as source) and lists.
- Switch from the top bar (sidebar on desktop) or More → Switch branch.
- The server checks every request's branch against the user's assignments.

## Cross-branch rules
- During a sale, *View stock in other branches* shows availability elsewhere — read-only.
- Stock belonging to another branch is never deducted; selling from another branch needs `sales.change_branch`, or
  move the stock with a transfer.
- Reports and the dashboard can show all of a user's branches or one branch.
