# S'Shop — product scope & implementation status

**Scope by:** James Sammy · **Platform:** mobile-first retail, inventory, sales & customer-loyalty application
**Living document.** The original scope (“Pablo Niche — Full Product Scope & Functional Requirements”) is kept here
section by section, with how S'Shop implements it. When the scope changes, update this file and the matching module doc.

Status: ✅ implemented · ◐ partly implemented (see note) · ⏳ planned

| § | Requirement | Status | Where / notes |
|---|---|---|---|
| 1 | Product vision — Pablo Loyalty becomes one module of a full retail platform (15 core modules) | ✅ | Product name is **S'Shop**. All 15 modules exist; see [modules](README.md#modules). |
| 2 | Multi-branch: per-branch stock, sales, orders, expenses, users, analytics; Current Branch; cross-branch stock visibility | ✅ | [13-branches](modules/13-branches.md). Current Branch chosen at sign-in, switchable in the top bar, validated server-side on every request (`X-Branch-Id`). Other branches' stock is view-only during a sale. |
| 3 | Products: master catalogue, codes, categories, suppliers, photos (default 5, configurable, primary photo, auto-optimised), availability flags, branch applicability | ✅ | [06-products](modules/06-products.md). Photos are resized to WebP in the browser before upload. |
| 4 | Add stock with current qty/value/price shown; quantity default 1, lockable | ✅ | [07-stock](modules/07-stock.md) → Receive stock. |
| 5 | Native barcode support (camera, manufacturer barcodes, per-item tracking, clearance on sale, history) | ✅ | Camera scanning (BarcodeDetector / ZXing) + handheld scanners. Unique active barcode enforced by a database index. Barcode trace in Stock → Barcoded items. |
| 6 | Sales/POS: product picker, item entry with live “difference from marked price”, optional discount, barcode clearance, branch | ✅ | [02-sales-pos](modules/02-sales-pos.md). **One pricing model:** selling price is the truth, discount = marked − selling (typing a discount sets the price), so discounts are never double-counted. |
| 7 | Sales cart with per-line and total figures incl. points to earn | ✅ | Desktop: persistent side cart. Phone: floating cart bar → sheet. Cart survives navigation. |
| 8 | Payments: M-Pesa (STK + manual code), Cash, Credit Sale; more methods configurable | ✅ | [integrations/mpesa](integrations/mpesa.md). Custom methods (e.g. Card, Bank) in Settings → Sales. An M-Pesa code can settle only one payment. |
| 9 | Credit sales with partial repayments, statuses, aging | ✅ | [03-credit-sales](modules/03-credit-sales.md). A deposit can be taken at the moment of the credit sale (roadmap 4a). |
| 10 | Sale completion as one safe transaction (stock, barcodes, customer, loyalty, analytics, receipt) | ✅ | Single PostgreSQL transaction with row locks; retries are idempotent (`client_ref`). |
| 11 | Customer book, search, default fields, configurable custom fields | ✅ | [09-customers](modules/09-customers.md). Mobile numbers normalised to 2547…/2541…; unique per business. |
| 12 | Loyalty & rewards: earn, referral, redemption, expiry, tiers, award periods, Gold/Silver/Bronze, histories | ✅ | [10-loyalty](modules/10-loyalty.md). Every point change is a ledger row. **Change from legacy:** opening a new award round no longer wipes balances or deletes sales; standings are computed from activity within the period. |
| 13 | Stock transfers Draft → Pending Approval → Approved → Dispatched (In Transit) → Received | ✅ | [08-transfers](modules/08-transfers.md). Receipt control can be switched off (dispatch = receive). |
| 14 | Stock position (opening, added, transfers, sold, allocations, returns, adjustments, damaged, closing, value) with filters and alerts | ✅ | Stock → Position, Stock Position report, low/out-of-stock notifications. |
| 15 | Stock adjustments & stock taking, never silently overwrite | ✅ | Every adjustment records reason, user, time, before/after, approver. Recounts re-derive the variance at approval time. |
| 16 | Customer ordering portal per business (`/order/business-name`), mobile identification | ✅ | [05-ordering-portal](modules/05-ordering-portal.md). Optional WhatsApp one-time code verification. |
| 17 | Order catalogue: photo cards, product detail, gallery, quantity, floating cart | ✅ | |
| 18 | Checkout, confirmation, Track My Order | ✅ | Public tracking link `/track/<token>` (unguessable, no account needed). |
| 19 | Order management statuses, events, reserved vs sold stock, completion → sale | ✅ | [04-orders](modules/04-orders.md). Configurable stage (Delivered or Completed) at which the order becomes a sale. Status names are configurable and optional steps can be switched off (roadmap 1b). |
| 20 | My Orders: latest 3, total count, visual progress | ✅ | |
| 21 | Loyalty points (and value) on the portal | ✅ | Toggles in Settings → Loyalty. |
| 22 | Expenses with categories, attachments, approvals | ✅ | [11-expenses](modules/11-expenses.md). Approval rules by amount threshold, category, branch and requester role (roadmap 2). |
| 23 | Dashboard: periods, filters, KPIs, product performance, charts | ✅ | [01-dashboard](modules/01-dashboard.md). |
| 24 | Gold/Silver/Bronze recognition without hard-coded thresholds | ✅ | Customer tiers are fully configurable (name + spend). Product and staff medals: by rank (default) or by configurable per-day targets on sales value or units, scaled to the period viewed (roadmap 3). Award-period winners and top customers stay rank-based. Underlying figures are always shown. |
| 25 | User / employee performance | ✅ | Dashboard staff panel + User Performance report. |
| 26 | Reports (23 templates), consistent filters, PDF & Excel | ✅ | [12-reports](modules/12-reports.md). Excel built server-side, PDF in the browser. |
| 27 | Approval / maker-checker engine, two-stage default, audit | ✅ | [15-approvals](modules/15-approvals.md). Excessive discounts are approved at the counter by supervisor email + PIN. Multi-level chains (up to 5 levels) with a decision trail (roadmap 2). |
| 28 | Granular roles & permissions, branch restriction | ✅ | [14-users-roles](modules/14-users-roles.md). 9 default roles, 40 permissions. |
| 29 | Audit trail, no hard deletes of financial/operational records | ✅ | [17-audit-notifications-search](modules/17-audit-notifications-search.md). Reversals, cancellations, voids and deactivation instead of deletes. |
| 30 | Settings architecture (business, product, sales, stock, order, customer, expense, reports, workflow) | ✅ | [16-settings](modules/16-settings.md). Product fields and order statuses added in roadmap 1. |
| 31 | Mobile-first UI/UX, responsive to desktop, light/dark, Outfit font, no horizontal overflow | ✅ | [ui.md](ui.md). Verified at 390, 820, 1440 and 1920 px. |
| 32 | Mobile navigation Home · Sales · Stock · Orders · More, permission-aware | ✅ | Desktop uses a grouped sidebar instead. |
| 33 | Data-integrity rules | ✅ | Enforced in the database (unique/partial indexes, checks) and in the ledger (row locks). Negative stock only when explicitly enabled. |
| 34 | Stock reservation: physical − reserved = available | ✅ | `stock_levels.reserved`; counter sales respect reservations. |
| 35 | Returns, reversals, corrections, refunds, stock restoration | ✅ / ◐ | Partial returns and full cancellations reverse stock, value, customer totals, loyalty (incl. referral share), credit. Exchange/correction = return + new sale (no single combined screen yet). |
| 36 | Receipts: view, PDF, print, share | ✅ | 80 mm PDF, browser print, WhatsApp share. |
| 37 | Universal search, Current Branch first | ✅ | Ctrl/⌘ K on desktop, search icon on phones. |
| 38 | In-app notifications; WhatsApp | ✅ | Live via SSE + WhatsApp Cloud API or wa.me links. |
| 39 | Migrate existing Pablo Loyalty data | ✅ | [migration-from-pablo-loyalty.md](migration-from-pablo-loyalty.md) — `sshop import-legacy`. |
| 40 | Reuse stable logic, restructure where needed | ✅ | Legacy points rule (threshold/points-per with product overrides), 50% referral bonus, (own\|referral) display, award tiers and WhatsApp messages preserved. Supabase replaced by the Rust API; PIN auth now hashed (Argon2) with lockout. |
| 41 | Inventory ledger: current stock = Σ movements | ✅ | `stock_movements` + locked `stock_levels` projection. The smoke test asserts they agree. |
| 42–43 | One connected ecosystem; sell in seconds | ✅ | |

## Decisions taken during implementation

1. **Stack.** Rust (Axum/SQLx) for the API and React/TypeScript for the UI. A Rust/WASM UI was considered and rejected
   for now: camera barcode scanning, charts and PDF generation are far more mature in the TypeScript ecosystem.
2. **One deployable.** The Rust binary serves the API, webhooks, live events and the web app — one Railway service.
3. **Photos & attachments in PostgreSQL** (≤3 MB photos, ≤5 MB attachments, resized client-side). Moving to Railway
   object storage (buckets) later only touches `routes/catalog.rs` and `routes/expenses.rs`.
4. **Award periods are non-destructive** (legacy reset all balances and deleted transactions).
5. **Settings are one JSON document per business** with typed defaults — new options never need a migration.
6. **Secrets live in environment variables**, never in Settings (M-Pesa, WhatsApp, JWT).

## Backlog (⏳)

- ~~Custom product fields; configurable order statuses.~~ ✅ done (roadmap 1, 2026-10-03).
- ~~Multi-level approval chains; expense approval rules by category/branch/role.~~ ✅ done (roadmap 2, 2026-10-03).
- ~~Threshold-based medals for products and staff.~~ ✅ done (roadmap 3, 2026-10-03).
- Deposits on credit sales ✅ (roadmap 4a, 2026-10-03); a combined exchange screen ⏳.

Added 2026-10-05 (owner request), delivered in this order before the remaining items:

- **5. Password visibility** — show/hide eye on every PIN/password field (sign-in, new, confirm, reset, supervisor).
- **6. Compact mobile-first sizing** — tighter, consistent font, input, button, card and spacing sizes on phones,
  scaling up on tablet/desktop; compact sign-in card; Outfit stays the default font.
- **7. Request access** — no open signup. Sign-in shows *Interested in accessing S'Shop? Request Access*; the form
  stores the request for platform-admin review, emails `jamosammy@gmail.com`, and shows the confirmation with the
  support numbers 0798 993 404 / 0732 968 898. Nothing is activated until a platform admin approves it.
- **8. User preferences** (per user) — font (Outfit default, Poppins, Inter, Roboto, Nunito) and display currency
  (KES default, USD, EUR) with live exchange rates; the active currency is shown once in the profile and figures are
  shown without repeated currency symbols.
- **9. Languages** — English, Swahili, French, Arabic (right-to-left), per user.

Then:
- Combined exchange screen (rest of roadmap 4).
- Offline queueing of sales on the POS (installable PWA).
