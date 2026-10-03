# 16 · Settings

**Scope:** §30 · **Route:** `/settings/*` · **API:** `GET/PUT /api/settings`, `PUT /api/settings/profile`, `POST /api/settings/logo`

Desktop shows a section menu beside the content; phones show the menu, then the section. Configuration sections save as
one document with a sticky *Save changes* bar; every save is audited (before/after).

| Group | Section | Contents |
|---|---|---|
| Business | Business profile | Name, ordering-link slug, tagline, phone, email, address, currency symbol, time zone, logo, ordering link |
| | Branches | module 13 |
| | Users · Roles & permissions | module 14 |
| Configuration | Products | Photos per product (default 5), auto code prefix, categories, suppliers |
| | Sales & payments | Quantity entry, barcode clearance, payment methods (add custom), manual M-Pesa, credit on/off & default days, receipt footer |
| | Stock | Barcode requirement, quantity entry, capture cost, valuation, low-stock default, transfer receipt control, allow negative stock |
| | Orders & ordering link | Link open, fulfilling branch, reserve stock, sale stage (Delivered/Completed), WhatsApp verification, show out-of-stock |
| | Customers | Require email, custom fields |
| | Loyalty & rewards | Earning, referral %, expiry, redemption, tiers, award winners, portal visibility |
| | Expenses | Required description/attachment, categories |
| | Reports | Hide cost & profit without financial access |
| Control | Workflow engine | module 15 |
| | M-Pesa & WhatsApp | Connection status, webhook URL, WhatsApp message toggles |

Credentials (M-Pesa, WhatsApp, JWT) are environment variables on the server — never stored or shown in Settings.
Permissions: `settings.manage` (and `branches.manage`, `users.manage`, `roles.manage` for those sections).
