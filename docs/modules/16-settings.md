# 16 · Settings

**Scope:** §30 · **Route:** `/settings/*` · **API:** `GET/PUT /api/settings`, `PUT /api/settings/profile`, `POST /api/settings/logo`

Desktop shows a section menu beside the content; phones show the menu, then the section. Configuration sections save as
one document with a sticky *Save changes* bar; every save is audited (before/after).

| Group | Section | Contents |
|---|---|---|
| Business | Business profile | Name, ordering-link slug, tagline, phone, email, address, currency symbol, time zone, logo, ordering link |
| | Branches | module 13 |
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

Credentials (M-Pesa, WhatsApp, JWT) are environment variables on the server — never stored or shown in Settings.
Permissions: `settings.manage` (and `branches.manage`, `users.manage`, `roles.manage` for those sections).
