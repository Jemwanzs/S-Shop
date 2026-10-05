# S'Shop documentation

| Document | What it covers |
|---|---|
| [scope.md](scope.md) | The living product scope — every requirement with its implementation status and design decisions. **Update it whenever the scope changes.** |
| [architecture.md](architecture.md) | Stack, components, data model, inventory ledger, transactions, security, real-time, background jobs |
| [development.md](development.md) | Local setup, project conventions, testing, adding a module |
| [deployment-railway.md](deployment-railway.md) | Deploying to Railway (service, database, variables, domains, webhooks) |
| [api.md](api.md) | HTTP API reference |
| [ui.md](ui.md) | Design system and responsive layout rules |
| [migration-from-pablo-loyalty.md](migration-from-pablo-loyalty.md) | Importing the legacy Supabase data |
| [integrations/mpesa.md](integrations/mpesa.md) | M-Pesa Daraja STK Push |
| [integrations/whatsapp.md](integrations/whatsapp.md) | WhatsApp Cloud API: messages, templates, webhooks |

## Modules

| # | Module | Doc |
|---|---|---|
| 1 | Dashboard & analytics | [modules/01-dashboard.md](modules/01-dashboard.md) |
| 2 | Sales / POS & receipts | [modules/02-sales-pos.md](modules/02-sales-pos.md) |
| 3 | Credit sales | [modules/03-credit-sales.md](modules/03-credit-sales.md) |
| 4 | Orders | [modules/04-orders.md](modules/04-orders.md) |
| 5 | Customer ordering portal | [modules/05-ordering-portal.md](modules/05-ordering-portal.md) |
| 6 | Products | [modules/06-products.md](modules/06-products.md) |
| 7 | Stock & inventory (barcodes, adjustments, stock take) | [modules/07-stock.md](modules/07-stock.md) |
| 8 | Stock transfers | [modules/08-transfers.md](modules/08-transfers.md) |
| 9 | Customers | [modules/09-customers.md](modules/09-customers.md) |
| 10 | Loyalty & rewards | [modules/10-loyalty.md](modules/10-loyalty.md) |
| 11 | Expenses | [modules/11-expenses.md](modules/11-expenses.md) |
| 12 | Reports | [modules/12-reports.md](modules/12-reports.md) |
| 13 | Branches | [modules/13-branches.md](modules/13-branches.md) |
| 14 | Users, roles & permissions | [modules/14-users-roles.md](modules/14-users-roles.md) |
| 15 | Approval workflows (maker-checker) | [modules/15-approvals.md](modules/15-approvals.md) |
| 16 | Settings | [modules/16-settings.md](modules/16-settings.md) |
| 17 | Audit trail, notifications & search | [modules/17-audit-notifications-search.md](modules/17-audit-notifications-search.md) |
| 18 | Access requests (no open signup) | [modules/18-access-requests.md](modules/18-access-requests.md) |
| 19 | Barcode scanning (camera, handheld, manual) | [modules/19-barcode-scanning.md](modules/19-barcode-scanning.md) |
| 20 | Platform owner: businesses & the Pablo Niche demo | [modules/20-platform-and-demo.md](modules/20-platform-and-demo.md) |

## Glossary

| Term | Meaning |
|---|---|
| **Tenant** | One business using S'Shop. Every record belongs to exactly one tenant. |
| **Current Branch** | The branch a user is operating from; the default for sales, stock, expenses and reports. |
| **Marked price** | The product's list selling price. |
| **Selling price** | The price actually charged on a sale line. Discount = marked − selling. |
| **Movement** | One row in the inventory ledger. Stock on hand = Σ movements. |
| **Reserved** | Units held for confirmed orders. Available = on hand − reserved. |
| **Tracked item** | A product unit with its own barcode, sold and transferred individually. |
| **Maker / checker** | The person requesting a sensitive action / the person approving it. |
