# 16 · Settings

**Scope:** §30 · **Route:** `/settings/*` · **API:** `GET/PUT /api/settings`, `PUT /api/settings/profile`, `POST /api/settings/logo`

Desktop shows a section menu beside the content; phones show the menu, then the section. Configuration sections save as
one document with a sticky *Save changes* bar; every save is audited (before/after).

| Group | Section | Contents |
|---|---|---|
| Business | Business profile | Name, ordering-link slug, tagline, phone, email, address, currency symbol, time zone, logo, ordering link |
| | Branches | module 13 |
| | Workspace & hours | Working days (Mon–Sun), opening and closing time, sales outside trading hours: allow / block — see below |
| | Users · Roles & permissions | module 14 |
| Configuration | Products | Photos per product (default 5), auto code prefix, **product fields**, categories, suppliers |
| | Sales & payments | Quantity entry, barcode clearance, payment methods (add custom), manual M-Pesa, credit on/off & default days, receipt footer |
| | Stock | Barcode requirement, quantity entry, capture cost, valuation, low-stock default, transfer receipt control, allow negative stock |
| | Orders & ordering link | Link open, fulfilling branch, reserve stock, sale stage (Delivered/Completed), WhatsApp verification, show out-of-stock, **order status names & optional steps** |
| | Customers | Require email, custom fields |
| | Loyalty & rewards | Earning, referral %, expiry, redemption, tiers, award winners, portal visibility |
| | Expenses | Required description/attachment, categories |
| | Reports | Hide cost & profit without financial access · Medals for best sellers and staff: by rank, or by Gold/Silver/Bronze per-day targets (sales value or units; 0 = off; must decrease Gold → Bronze) |
| Personal | User preferences (every user) | Display currency KES / USD / EUR with live rates (open.er-api.com, refreshed when the app opens, cached 1 h on the server; last known rates if the source is down); font Outfit (default) / Poppins / Inter / Roboto / Nunito. Stored per user, so it follows them across devices. The active currency is shown under the profile (More page, account menu). |
| Control | Workflow engine — per action: on/off, 1–5 approval levels, conditions (amount, branch, requester role, expense category) | module 15 |
| | M-Pesa & WhatsApp | Connection status, webhook URL, WhatsApp message toggles |

## Workspace & hours (`settings.workspace`)
- **Working days and trading hours** for the whole business; a branch can keep its **own hours** (Branches).
  Default: every day, 00:00–00:00 (open all day, calendar days) — nothing changes until hours are set.
- **Business date.** Every sale, order, payment, credit sale, stock movement and return stores its true time *and* the
  business day it belongs to. A closing time at or before the opening time runs past midnight: with 06:00 → 02:00 a
  sale at 01:30 on Tuesday counts for **Monday**. Same opening and closing time = open 24 hours with the day starting
  at that time. The date is filled by the database (trigger) from the branch's hours at the moment of the
  transaction, so changing hours later never moves past records.
- **Today** everywhere (dashboard, reports, lists, leaderboards, expense date, credit due dates) is the business day
  of the Current Branch. Dashboards, reports, staff performance and leaderboards filter by business date; transfers,
  adjustments, loyalty and the audit trail keep the true time.
- **Outside trading hours:** *allow* (default; the till shows an “Outside trading hours” notice) or *block* counter
  sales — people with **Sell outside trading hours** (`sales.outside_hours`) can still sell. Existing roles that may
  approve discount overrides were given this permission, and the Manager, Supervisor and Branch Manager templates
  include it. Online orders are always accepted.

Credentials (M-Pesa, WhatsApp, JWT) are environment variables on the server — never stored or shown in Settings.
Permissions: `settings.manage` or the area permission (`settings.workspace`, `settings.sales`, …) (and `branches.manage`, `users.manage`, `roles.manage` for those sections).
