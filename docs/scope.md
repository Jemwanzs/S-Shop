# S'Shop — product scope & implementation status

**Scope by:** James Sammy · **Platform:** mobile-first retail, inventory, sales & customer-loyalty application
**Living document.** The original scope (“Pablo Niche — Full Product Scope & Functional Requirements”) is kept here
section by section, with how S'Shop implements it. When the scope changes, update this file and the matching module doc.

Status: ✅ implemented · ◐ partly implemented (see note) · ⏳ planned

## Status board

*Kept current with every shipped item — last updated 2026-10-21.* History stays below; this table is only what is
open now. Release history: [release-notes.md](release-notes.md).

**Latest live:** 145 workspace addresses and 94 Railway settings as code (2026-10-19, verified on s-shop.store).
**Not yet live:** 146 documentation workflow, 147 error alerting, 148 approval eligibility and 149 credential rotation
support are built, tested and CI-green but not deployed. Railway's outage of 21 October was a **security incident**
(attackers decrypted a subset of customers' environment variables; builds restored 22 October). No push since then has
started a deployment — Railway handed GitHub every stored GitHub key for revocation, so the GitHub link on Railway is
reconnected by the owner, then *Deploy latest commit* on **sshop**. Then the credential rotation (149) follows its
runbook.

| # | Open item | Priority | Owner | Depends on | Next action |
|---|---|---|---|---|---|
| 149 | **Rotate credentials** (Railway security incident) | **Critical** | Owner (code part built) | Deploys working (reconnect GitHub on Railway) | Follow the runbook in deployment-railway.md → *Rotating credentials*, in order |
| 28 | Database backups | High | Owner | Railway plan with backups | Turn on scheduled Postgres backups (daily, keep 7+), then restore one into a scratch database to prove it |
| 29 | Email sender (`MAIL_FROM`) | High — invitations and applicant emails | Owner | `s-shop.store` verified in Resend (DNS records at the registrar) | `MAIL_FROM` exists on Railway; confirm the domain shows *Verified* in Resend and send a test invitation to an outside address |
| — | Real production M-Pesa payment | Medium | Owner | The business's own Till / Paybill credentials | Settings → M-Pesa → *Test STK Push* (KSh 1) in Production |
| — | Real-device checks | Medium | Owner | iPhone, label printer, scanner | Face ID sign-in (132), iPhone home screen (128), print and scan item labels (133–138) |
| 30 | `www.s-shop.store` | Low | Owner | DNS; **Railway's custom-domain limit** — the service has 3 (`s-shop.store`, `s-shop.click`, `www.pablo-niche.com`) and Railway refused another on 2026-10-16 | Free a slot (e.g. remove `s-shop.click` if unused) or upgrade the plan, then add it with a CNAME — or redirect `www` at the registrar instead |
| — | *Discard changes?* on screens' own Cancel buttons | Low — decision | Owner | — | Decide yes / no |
| 31 | EU region | Low — deferred by owner | Owner | 28 (tested backup and restore) | Choose a maintenance window |
| 147 | Error alerting | Medium — built | Platform | Resend sender for emails beyond the Resend account owner (29) | Verify live: *Send test alert* reaches the bell and your email |
| — | More than one app instance | Low — not needed yet | Platform | Live events on PostgreSQL `LISTEN/NOTIFY`; one runner for background jobs | Only when traffic needs a second replica; keep one replica until then |
| — | Photo storage | Low | Platform | — | Move product photos and logos from the database to a Railway bucket when their volume grows |

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
| 35 | Returns, reversals, corrections, refunds, stock restoration | ✅ | Partial returns and full cancellations reverse stock, value, customer totals, loyalty (incl. referral share), credit. Exchange screen: return + new sale in one step, difference only. |
| 36 | Receipts: view, PDF, print, share | ✅ | Superseded by 65–67: 50 mm stored receipts, email, WhatsApp file share, secure link. |
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
- Deposits on credit sales ✅ (roadmap 4a, 2026-10-03); combined exchange screen ✅ (roadmap 27, 2026-10-06).

Added 2026-10-05 (owner request), delivered in this order before the remaining items:

- ✅ **5. Password visibility** — show/hide eye on every PIN/password field (sign-in, new, confirm, reset, supervisor).
- ✅ **6. Compact mobile-first sizing** — tighter, consistent font, input, button, card and spacing sizes on phones,
  scaling up on tablet/desktop; compact sign-in card; Outfit stays the default font.
- ✅ **7. Request access** ([module 18](modules/18-access-requests.md)) — no open signup. Sign-in shows *Interested in accessing S'Shop? Request Access*; the form
  stores the request for platform-admin review, emails `jamosammy@gmail.com`, and shows the confirmation with the
  support numbers 0798 993 404 / 0732 968 898. Nothing is activated until a platform admin approves it.
- ✅ **8. User preferences** (per user) — font (Outfit default, Poppins, Inter, Roboto, Nunito) and display currency
  (KES default, USD, EUR) with live exchange rates; the active currency is shown once in the profile and figures are
  shown without repeated currency symbols.
- ✅ **9. Languages** — English, Swahili, French, Arabic (right-to-left), per user: translation system, language
  preference + sign-in picker, full RTL layout, every screen (roadmap 26), and every server message — 244 fixed messages
  and 66 messages with values ("Only {} × {} in stock"), matched as templates so the values are kept and known words
  inside them (statuses, record names) are translated too.
- ✅ **10. Mobile-first UI/UX refinement** (2026-10-05) — one design language across the app: shared tokens for type,
  control heights, radius, card padding, bottom-nav height and overlays; Outfit everywhere (tabular figures for
  numbers); compact primitives (buttons, inputs, selects, textareas, cards, KPI cards, lists, chips, sheets, dialogs,
  bottom nav); tables scroll inside their card; light and dark verified on every main screen at phone and desktop
  widths. Ongoing: screen-by-screen polish as remaining modules are translated. Screen-by-screen pass (2026-10-06): every main screen checked on phone and desktop in light and dark — no overflow,
  no page errors; refinements: comparison hint only with a figure, credit summary side by side on phones, approval
  summaries no longer repeat the amount, transfers received short/damaged marked in the list.
- ✅ **11. Camera & barcode scanning** (2026-10-05) — one shared scanner (camera full screen on phones, handheld,
  manual) for sales, receiving, transfers, counts, returns and order completion; scan-to-cart with qty increments and
  per-unit validation; other-branch availability; *Barcode not found* → Scan again / Search / Assign barcode
  ([module 19](modules/19-barcode-scanning.md)). Fixed on the way: camera scanning never started (video element
  mounted after the camera effect ran); orders with tracked products could not be completed; cost prices were exposed
  to users without financial access in product and stock endpoints.
- ✅ **12. Platform branding** — tenant brand top-left, *Powered by S'Shop* top-right on larger screens (More page on phones).
- ✅ **13. Businesses & demo data** — platform admins open any business (audited, banner, return); repeatable
  Pablo Niche demo business built through the real API ([module 20](modules/20-platform-and-demo.md)). Keep extending
  the demo as modules land.

Added 2026-10-05 after the analytics / access / operations gap review ([gap-review-2026-10.md](gap-review-2026-10.md)):

- ✅ **14. Access & personal analytics** — `staff.view_others` with server-side scoping of sales, dashboards and reports;
  **My Dashboard**; permission-aware **Recent activity**; `sales.print`; settings access per area; role deactivation.
- ✅ **15. Leaderboards** — products (value, units, sales, orders, profit/margin) and staff (value, units, transactions,
  orders processed, average sale, customers served/acquired, discounts, credit) with period/branch filters and medals.
- ✅ **16. Workspace** — working days and operating hours (business-wide, branch overrides), cross-midnight
  **business date** stored with each transaction and used by dashboards, reports and performance.
- ✅ **17. Geofencing** — branch coordinates and radius; *anywhere* (default) or *at the branch* for selected actions,
  enforced on the server, with a bypass permission and the location in the audit trail.
- ✅ **18. Transfer receipt with discrepancies** — short/damaged quantities recorded on receipt with a reason.

Added 2026-10-06 (owner requests and the [production readiness audit](production-readiness-2026-10.md)):

- ✅ **19. Strict barcode clearance at Record Sale** — every scan checked on the server before *Add to cart* (product,
  branch, status, duplicates) with titled errors; exact unit locked and cleared at checkout ([module 19](modules/19-barcode-scanning.md)).
- ✅ **20. Credit sale recall** — goods back to the original branch, re-scan of tracked units, balance revised,
  overpayment refunded or kept as customer credit, audited, approval workflow ([module 03](modules/03-credit-sales.md)).
- ✅ **21. S'Shop dropdowns** — one dropdown component for the whole app (panel / bottom sheet, search, check marks).
- ✅ **22. Reliable product photos** — shared picker with previews and limit, per-photo results, retry-safe uploads;
  *Add photos* on Receive stock ([module 06](modules/06-products.md)).
- ✅ **23. Production safety** — security headers (CSP, HSTS …), CI on every push with Railway *Wait for CI* on.
- ✅ **24. Hardening** — cross-business attack tests (32 kinds of request refused, nothing changed), tenant id on
  id-based writes in route handlers, per-client rate limits (429), database check in `/healthz`, forged
  `X-Forwarded-For` no longer trusted.

Then, in this order:
- ✅ **25. Offline POS** — installable app shell; cash-style sales saved on the device and synced at their real time
  (idempotent `client_ref`), refusals listed for retry/discard ([module 02](modules/02-sales-pos.md)).
- ✅ **26. Languages** — finish roadmap 9: buttons, chips, badges, placeholders, list headers, toasts and error titles
  translate centrally; screen text wrapped; Swahili, French and Arabic for every visible string.
- ✅ **32. M-Pesa at Record Sale** (owner request 2026-10-06) — two separate flows: *manual* (number and confirmation
  code both optional) and *STK Push* (number required → push → confirmed result). Push STK shown always, disabled with
  *STK not configured* when the integration is not active; never blocks a manual M-Pesa sale.
- ✅ **33. Ordering link — Show product prices** (owner request 2026-10-06) — Settings → Orders & ordering link, ON by
  default; when OFF, prices are hidden (and not sent) everywhere on the customer ordering link — cards, details, cart,
  checkout, order history and tracking; staff screens unaffected.
- ✅ **27. Combined exchange screen** — return + new sale in one step, difference paid or refunded; exchange
  payments net to zero ([module 02](modules/02-sales-pos.md)).

Added 2026-10-07 — **Platform Owner, Tenant Monitoring & Billing** (owner request, delivered 2026-10-07; [module 21](modules/21-platform-billing.md)).
Platform Owner functionality stays separate from tenant administration: a Tenant Admin never gains Platform Owner
access or sees another business's billing or activity. Built in this order:
- ✅ **34. Tenant directory** — per business: name, status, admins and contacts, branches and locations, signup /
  onboarding details, activation date, billing model, subscription status, last payment, next due, outstanding.
- ✅ **35. Tenant activity monitoring** — sign-ins, failed sign-ins, sales, stock counts, transfers (existing audit
  trail) filtered by business, branch, user, activity and date range; the platform owner can reset a tenant admin's PIN
  (audited).
- ✅ **36. Activate / deactivate / reactivate** — confirmation + reason; deactivation blocks sign-in, ends sessions,
  disables the ordering link and blocks new transactions while keeping all data; every change in the platform audit.
- ✅ **37. Billing models & documents** — per business *Subscription* (amount, monthly / quarterly / semi-annual /
  annual / custom, start, next due, grace period, auto-renew) or *One-off* (amount, paid, payment date, optional
  maintenance fee with its own frequency and next due); quotation → invoice → payment → receipt (downloadable).
- ✅ **38. Paystack** — credentials only in environment variables; the server initiates, verifies, receives the webhook
  (signature checked) and reconciles; only a verified payment marks an invoice paid and moves the period / next due.
- ✅ **39. Tenant Billing** (Settings → Billing) — model, amount, status, last payment, period covered, next due,
  outstanding, invoice / payment / receipt history; vendor bank details (masked); *Pay now* for one outstanding invoice.
- ✅ **40. Platform billing dashboard** — active, deactivated, paid, due soon, overdue, subscription revenue,
  maintenance due, with drill-down per business.

Added 2026-10-07 — **Tenant-specific & module-based billing** (owner request, delivered 2026-10-07;
[module 21](modules/21-platform-billing.md)). Reuses the billing built in 37–40; one billing implementation for every
screen, the session check and the background job:
- ✅ **41. Module-based packages** — *Full platform* (default) or *Selected modules* (Sales/POS, Orders, Stock &
  Inventory, Customers, Loyalty, Credit Sales, Expenses, Reports & Analytics). Excluded modules are refused by the server
  on every request (and inside a sale: credit payment, points redemption; the ordering link needs Orders) and hidden in
  the web app through the same permission check.
- ✅ **42. Tenant-specific pricing** — per business: model, package, base price (or per-module prices), frequency,
  currency, discount (percentage or fixed), tax (yes/no + %), start date, next billing date, grace period; shown as
  Base → Discount → Tax → Amount payable, and carried on every invoice and PDF.
- ✅ **43. Free, trial & grace** — free access (billing off, business active), trial (start, end, modules; billing
  starts the day after, audited when it ends), grace days plus explicit grace extension, optional automatic suspension
  when overdue (sign-in and Billing still work so the business can pay; lifted on payment).
- ✅ **44. Billing statuses** — Platform owned, Trial, Free, Active subscription, One-off paid, Payment due, Maintenance
  due, Grace period, Overdue, Suspended — used by the dashboard, the business's Billing page, the session and the jobs.
- ✅ **45. Platform owner's protected business** — ownership *Platform owned* (set at start-up for businesses of
  `PLATFORM_ADMIN_EMAILS`): never deactivated or suspended, no plans, invoices or payments, complete platform — enforced
  in the API and by database triggers.
- ✅ **46. One-off & maintenance** — one-off fee (or *paid on* for payments made before), independent maintenance fee
  with its own frequency, tax and discount; a one-off plan never produces subscription invoices. Every pricing, module,
  discount, tax, trial/grace, free-access and status change is audited with previous and new values.

Added 2026-10-07 (owner requests, delivered 2026-10-07; [module 22](modules/22-actions-and-quick-actions.md)):
- ✅ **47. Action button states** — one reusable action-state component for the whole app instead of per-page button
  logic: each action derives visibility, enabled state, label, processing and result from permissions, record/workflow
  state, unsaved changes, validation, configuration and connectivity (Ready → Processing → Success/Error → next state);
  short contextual disabled labels (*Nothing to save*, *Select a file*, *Nothing to receive*, *STK not configured* …);
  no double clicks, no success before the server confirms, work kept on failure; duplicate submissions also refused
  by the server for every endpoint.
- ✅ **48. Floating quick sale** — a small warm-brown *Record Sale* bubble (centre-right) on operational screens for
  users who may record sales (permission, Sales/POS module, branch, trading hours/location, offline rules), opening the
  existing Record Sale screen; *User preferences → Quick actions*: draggable ON/OFF (default OFF), position remembered
  per device and snapped to the nearest edge, tap vs drag never confused; built as a reusable floating quick action.

Added 2026-10-08 (owner request, delivered 2026-10-08; [module 15](modules/15-approvals.md)):
- ✅ **49. Workflow changes reach pending approvals** — audit found the engine read workflows by position (🟡 partial:
  steps inserted before a completed one shifted history, removed steps left requests decidable by any approver, no
  notifications, no sync audit). Steps get stable ids; each pending request keeps its own chain and every decision
  records its step; saving a workflow reconciles pending requests of that action — completed steps kept and never
  repeated, the request moves to the first step not yet approved (never back to the start, never auto-approved),
  new approvers notified and queues refreshed, out-of-order or fully-approved cases flagged for an administrator,
  every change audited (who, previous/new workflow, affected requests, previous/new next approver).

Added 2026-10-08 (owner request):
- ✅ **50. Larger floating sale bubble** — radius 1.5× (66 px phones/tablets, 72 px larger screens), icon scaled and
  centred, edge tuck scaled to 21 px; position, dragging, tap-to-open, permissions and responsiveness unchanged.

Added 2026-10-08 — **Website Add-On** (owner requests: *Complete Tenant Website, Marketing Storefront & Online Ordering*
and *Premium Website UI/UX, Product Pricing, User Access, Themes & Shopping Experience*; [module 23](modules/23-website.md)).
An optional, billable, standalone branded website per business — S'Shop stays the engine behind it (products, orders,
customers, stock, payments). Built in this order, reusing the ordering link, products, orders, billing, roles and audit:
- ✅ **51. Website service** — *Settings → Integrations → Website* locked until activated; *Request Website Service* →
  platform owner (email + in-app) → approve / decline / activate / disable; billed through the existing engine as its own
  service (one-off, subscription, maintenance, trial, grace, tax, % / fixed / 100% discount); disabling keeps everything
  and shows *Temporarily unavailable*; POS and operations unaffected.
- ✅ **52. Website Management Centre** — Overview · Content · Design · Products · Categories · Services · Testimonials ·
  Media · Users & Access · Domain · SEO · Analytics; draft → preview (mobile / tablet / desktop) → publish, discard,
  publication history and rollback; branding (logo, colours with contrast checks), typography (Outfit, Poppins, Inter,
  Roboto, Nunito; compact / balanced / spacious), style (modern / minimal / elegant / bold), light / dark / both;
  navigation (show / hide / reorder / rename), home sections (show / hide / edit / reorder), about, contact, social,
  cookie consent; website permissions granted to existing users without operational access; everything audited.
- ✅ **53. Public website** — mobile-first, fast, accessible, independently branded: home (hero, featured, new arrivals,
  most popular, promotions, categories, services, testimonials carousel, CTA), about, products (search with suggestions,
  categories, grid / carousel), product detail (gallery, availability, quantity), services, contact, cart → existing
  S'Shop ordering (customers, orders, branch, stock, loyalty); SEO metadata, sitemap and canonical URLs from the server.
- ✅ **54. Website products & pricing** — S'Shop products as the source: publish / feature / reorder, marketing name and
  description, badges, categories presentation, own gallery or the product's photos (default 5), website-wide price
  visibility (= the ordering-link setting) with per-product override (inherit / show / hide) and hidden-price action
  (enquire / contact / WhatsApp / order); hidden prices never leave the server.
- ✅ **55. Media library & image quality** — tenant-isolated uploads (logos, banners, products, services, testimonials,
  about, promotions), responsive sizes, quality checks (✓ good / ⚠ warning / ✕ cannot upload).
- ✅ **56. Custom domains** — add, check, DNS instructions (type / name / value, copy), verify ownership and routing,
  states (unconfigured … active / misconfigured); the verified domain alone decides the business.
- ✅ **57. Website analytics** — visitors, product views, most viewed, add-to-carts, order starts, completed orders,
  conversion by period; orders reconcile with S'Shop orders.

  Built 2026-10-08 as specified. Details, permissions, domain states and tests: [module 23](modules/23-website.md).
  Additions found while building: a product's hidden price is hidden on the ordering link, order history and tracking
  too (one rule everywhere); website revenue on the platform billing dashboard; automatic domain attachment needs
  `RAILWAY_API_TOKEN` (otherwise the platform owner records the routing target).

Added 2026-10-08 — **Onboarding & account recovery** (owner requests: *Automated Onboarding Emails & WhatsApp Access
Notifications*, *Approved Access Credentials & Intelligent Login Feedback*, *Self-Service PIN/Password Reset for All
Users*; [module 24](modules/24-onboarding-and-recovery.md)). Audit first: the owner's request email existed (plain text,
best effort, a yes/no flag); applicant acknowledgement, welcome, rejection and reset emails, delivery tracking, resend,
PIN expiry / forced change and self-service reset were missing.
- ✅ **58. Onboarding emails with delivery tracking** — owner email with every detail incl. **estimated users** (new form
  field) and in-app notification; applicant acknowledgement; welcome email with a single-use set-up link (never the PIN);
  courteous rejection email with an optional reason (internal note never sent); responsive branded HTML + text; every
  email logged (queued → sent / failed / skipped, then delivered / delayed / bounced / complained via the signed Resend
  webhook); retry; notification history on requests and businesses; a failed email never undoes an approval.
- ✅ **59. Approved access credentials** — *Business Activated* modal with email status, *Resend Email*, *Copy Message*
  and WhatsApp opening the administrator's chat (254… format) with the message composed and without the PIN; one-time
  PINs expire after 72 h, work until replaced at first sign-in (server-enforced) and end older sessions; approved cards:
  *View Login Details* (receipt-style slip: activation status, last sign-in, email delivery), *Resend Welcome Email*,
  *Issue New One-Time PIN* (confirm, shown once, audited without the PIN), WhatsApp.
- ✅ **60. Self-service PIN / password reset for every user** — *Forgot PIN / Password?* on sign-in → neutral answer →
  single-use 30-minute link (hashed, in the URL fragment) → new PIN → all sessions end → confirmation email; rate-limited,
  no account discovery, audited; changing one's own PIN also ends other sessions; support route when the email is lost.
- ✅ **61. Applicant status at sign-in** — never revealed on a failed sign-in (neutral message + *Check your request
  status*); an emailed status link (proof of ownership) shows pending / approved (with *Resend Setup Instructions*) /
  rejected (no internal notes), each with Call / WhatsApp support contacts.

Added 2026-10-08 — **Sales ownership & data visibility** (owner request: *Sales Ownership, Ownership Transfers &
Role-Based Data Visibility*; [module 25](modules/25-sales-ownership-and-visibility.md)).
- ✅ **62. Sale Owner vs Recorded By** — every sale keeps who recorded it and who is credited; *Sale Owner: Name ✎* at
  New Sale (default the signed-in user; others only with *Assign sale owner* and only eligible salespeople of the branch,
  re-checked by the server); receipts say *Served by* the owner; reports show both.
- ✅ **63. Controlled ownership changes** — *Change Sale Owner* (reason required) through the workflow engine (default:
  one level, Tenant Administrator; configurable); one pending request per sale; requester never approves; conflict-safe;
  amount, branch, date, payments, stock and receipt untouched; full history and audit. Performance, dashboards,
  leaderboards and reports follow the owner immediately in the sale's original period; business totals unchanged.
- ✅ **64. Data-visibility scopes** — per role and area (sales, dashboards & analytics, reports & exports, leaderboards,
  orders, credit): own records / assigned branches / all branches; one server-side resolver for lists, dashboards,
  reports, exports, leaderboards, search and single records; visibility never grants operations; My Dashboard always
  own; per-user exceptions (scopes, allow / restrict) and default branch, never beyond the granter's own access.

Added 2026-10-08 — **Digital receipts & reconciliation** (owner requests: *Premium Compact Digital Sales Receipt &
Sharing*; *Automatic Receipt & Loyalty Points Reconciliation After Returns and Exchanges*;
[module 26](modules/26-receipts-and-reconciliation.md)). Not KRA eTIMS.
- ✅ **65. Premium compact receipt** — 50 mm, height follows the content; logo → business → branch → contact → SALES
  RECEIPT → compact items → totals & payments → *Served by* (Sale Owner) → thank-you → *Digitally signed by [Business]*
  → small S' mark. One template for screen, PDF, print, image, public link and history; issued once as an immutable
  snapshot (logo content-addressed); receipt configuration with live preview (Settings → Sales & payments).
- ✅ **66. Receipt sharing** — PDF, print, image; email with the PDF attached from the business's name (tracked in the
  email log, audited); WhatsApp: Business API when configured, else the phone's share sheet with the PDF file, else
  WhatsApp with a short message + secure link (never claims *sent* without confirmation); secure, rate-limited
  `/r/{token}` page.
- ✅ **67. Reconciliation after returns & exchanges** — nothing changes until final approval (exchanges now go through
  the `sale.return` workflow instead of being refused); adjustment / exchange receipt linked to the original (original
  preserved) with status, items returned & remaining, refunds, net sale value, approver and loyalty (original, reversed,
  not recoverable — recorded as a liability — and net); reconciliation summary on the sale page; every module reads the
  same records.

Added 2026-10-08 — **Domain check fix & order alerts** (owner reports: *DNS check failing* for s-shop.click; *Real-Time
Website Order Notifications & Sidebar Order Badges*).
- ✅ **68. DNS records named the way providers expect** — diagnosis: the TXT record was created as
  `_sshop-verify.s-shop.click.s-shop.click` because the provider's *Name* field is relative to the domain and the full
  name had been typed. Records now show the **host** to type (`_sshop-verify`, `www`, `@`) with the full name as a
  fallback; *Check now* detects a record saved under the doubled name and says exactly how to fix it (*Saved under the
  wrong name*). Same for Railway's `_railway-verify` record.
- ✅ **69. New-order alerts & Orders badge** — every new order (website, ordering link, staff) notifies, once, the users
  who may manage orders at its branch and whose orders scope reaches it: *New customer order ORD-… · Customer · Branch ·
  items · total*, opening the order. Orange **Orders** counter (sidebar and phone bar) = orders still *New* in the user's
  scope, 99+ above 99, hidden at zero, live over SSE; independent of read state (reading alerts never lowers it,
  confirming the order does). User preferences → Notifications: new-order notifications, in-app pop-ups, sound (chime).

Added 2026-10-08 — **Tenant management & secure business access** (owner request: *Centralized Tenant Management, Secure
Business Access & Multi-Business Architecture*; [module 27](modules/27-tenants-and-secure-access.md)):
- ✅ **70. Tenant accounts** — Platform owner → tenants → businesses → branches → users. Every approved access request
  creates one tenant account and its first business; existing businesses become their own tenant's first business.
  Platform → Tenants: cards (administrator, contacts, businesses, branches, users, status, billing status, next due)
  and a tenant page (overview, businesses & branches, users, billing per business, website services, activity &
  security, access & status). Activate / deactivate a whole tenant (reason, sessions ended, data kept; platform-owned
  tenants protected).
- ✅ **71. Secure business access** — *Open business* no longer switches silently. Two routes: *Sign in as the tenant's
  administrator* (their own PIN, entered by them; the platform never sees or stores it) or *Platform support access*
  (fresh PIN re-entry, reason, scope view-only / full, 15 min – 8 h, the tenant's consent when its policy asks for it,
  banner with countdown, ended by either side at any time, audited start to end). Old acting tokens stop working.
- ✅ **72. Several businesses per tenant, one sign-in** — the platform owner adds businesses to a tenant; a tenant's
  administrator gives a person access to another business of the same tenant (own role, branches and scopes there);
  *Switch business* lists only those businesses and needs no new sign-in; other tenants stay out of reach.
- ✅ **73. Tenant activity** — logins, stock taking, transfers, sales recording, support sessions, billing and status
  changes per tenant, with business / branch / user / date filters; no customer or financial details.

Added 2026-10-08 — **Custom domains & premium storefront** (owner requests: *Tenant Custom Domain & Automatic Website
Mapping*; *Premium Website Redesign, Product Experience, Animations & Seamless Custom Domains*;
[module 23](modules/23-website.md), section 74–77). Audit first: domain
verification, host routing, canonical links, the orders engine on custom domains, 2-per-row phone grids, collections
from real data (*Popular* only from actual sales), header search with suggestions, light / dark / both and the
mobile / tablet / desktop preview already existed (✅).
- ✅ **74. Custom domains everywhere** — *Main website address* switch: the S'Shop address forwards to the business's
  own domain (temporary redirect, so it can be switched back or removed safely; previews stay put); `www.` and bare twins
  of a connected domain forward to it; `/orders` works like `/order`; *Awaiting platform configuration* shown honestly
  when ownership is proven but the hosting side is set up by hand; *Test domain connection*. A domain is never *Active*
  before `https://<domain>` answers from S'Shop for that host.
- ✅ **75. Product photos** — thumbnails made in the browser at upload (480 px) and served with `?size=thumb` on the
  website and in grids (older photos fall back to the optimised original); responsive `sizes`, lazy loading, fade-in
  on load; a branded placeholder (product initial on the brand colours) when a product has no photo or a photo fails —
  never a broken-image icon, never a stock photo.
- ✅ **76. Premium product cards** — image → name → availability (dot) → an action row of its own (price + *Add*):
  nothing floats over names or prices; equal heights per row; three per row on phones switches to a compact card with
  an icon-only *Add* under the price.
- ✅ **77. Animations** — Website → Design → *Animations: Off / Subtle (default) / Standard*: page fade, sections fading
  in on scroll, card hover lift, button press, cart badge bump, *Added to cart · View cart* confirmation; always off
  for visitors whose device asks for reduced motion; CSS only, no animation library.

Added 2026-10-09 — **Automatic Railway domains, premium listings, responsive tabs** (owner requests: *Railway Automatic
Domain Attachment*; *Premium Product Cards, Portrait Photography, Hover Animations & Configurable Pagination* with the
*Default Landing Page* note; *Responsive Settings Navigation & Tab Styling*).
- ✅ **78. Railway automatic domain attachment** — audited against Railway's live GraphQL schema: the integration read
  `RAILWAY_API_TOKEN` server-side only but compared Railway's statuses with values Railway never sends (`VALID` /
  `FAILED` instead of `DNS_RECORD_STATUS_PROPAGATED` / `CERTIFICATE_STATUS_TYPE_ISSUE_FAILED`), took the first DNS record
  as the routing one and looked up `_railway-verify` without the domain — fixed. Now: attach only after S'Shop
  ownership is proven; reuse an existing attachment on the service (`domains` query) instead of duplicating; Railway's
  own records shown by type, host label and purpose (traffic route, certificate challenge); certificate errors explained;
  attach audited; *Active* still only when `https://<domain>` answers for that business. Needs `RAILWAY_API_TOKEN` set
  on the service (not set yet).
- ✅ **79. Responsive tab rows** — one `ScrollRow` for the Website tabs, tenant page, chip filters (period, status
  segments on Orders, Sales, Credit, Customers, Stock, Transfers, Approvals, Leaderboards …), platform billing and
  activity: single row, smooth touch / trackpad / wheel scrolling, ‹ › arrows only when more is hidden that way, faded
  edges, the active tab kept in view, no scrollbar, works in RTL.
- ✅ **80. Premium portrait product cards** — 3:4 frames with the whole product visible by default (contain, neutral
  frame, breathing room; Cover / Square / Landscape still selectable); badges from real data (New / Featured / Offer,
  −X % from a *Was price*, Out of stock); price with the struck-through *was* price; desktop hover: 4 % zoom, card lift and
  brand-tinted border, the next photo, emphasised *Add* (≈250 ms); phones: press feedback; reduced motion respected.
- ✅ **81. Filters, sorting & pagination** — Search · Category · *In stock only* · Sort (Recommended, Newest, Name A–Z / Z–A,
  Price low–high / high–low — price sorts only where prices are shown; hidden prices never reveal their order);
  *Showing 1–10 of 143 products*; server-side pages counted in products (default 10; 6 / 10 / 12 / 16 / 20 / 24 / 30 /
  custom 1–100), Previous · 1 … 5 6 7 … 12 · Next (phones: ‹ Page 2 of 8 ›), page in the URL, filters and sort reset to
  page 1, scroll back to the heading; pagination off = *Load more*. The ordering link gets the same portrait cards,
  sort and pages of 12.
- ✅ **82. Default landing page** — Website → Design: Products (default) · Home · Categories · Services at the root of the
  S'Shop address and custom domains; Home moves to `/home`; every page stays in the menu.

Added 2026-10-09 — **Quick Login PIN** (owner request: *Configurable Quick Login PIN (4–6 Digits)*;
[module 28](modules/28-quick-login-pin.md)).
- ✅ **83. Quick Login PIN on trusted devices** — a 4–6 digit PIN (salted hash) per person, working only on devices they
  trusted after a full sign-in (device secret stored as SHA-256); keypad sign-in with *Use password instead* and
  *Forgot PIN?*; Quick sessions marked — payments, roles & access, PIN and security settings, support decisions and
  platform administration need the full sign-in (also after switching business); locks after repeated wrong PINs,
  device revoked at twice the limit; every trusted device revoked when the full PIN changes or is reset, on *Sign out
  everywhere*, Quick PIN off or reset, or when the business / platform switches it off; platform rules (on/off,
  shortest PIN, session length, attempts, device lifetime) and business rules (on/off, roles); administrators reset but
  never see a Quick PIN; audited without PINs. Gap closed on the way: an administrator's PIN reset now also ends the
  person's older sessions.

Added 2026-10-10 — **Holiday & promotional campaigns** (owner request: *Holiday Greetings, Promotional Banners &
Featured Products*; [module 29](modules/29-holiday-campaigns.md)). Off by default.
- ✅ **84. Campaigns** — 15 occasions with ready-written greetings; templates (Christmas, New Year, hearts, pastel,
  crescent & lanterns, celebration, sale, elegant gold, minimal), colours, background image from the media library,
  CSS decorations, gentle animation (reduced motion respected), height, button; featured products as real best sellers
  (period, category, never invented) or a manual pick, with the website's prices, stock and cart; hero / compact / top
  strip / floating card on chosen pages; dismissible per campaign version.
- ✅ **85. Schedule & lifecycle** — draft · scheduled · live · expired · archived computed from the times in the
  business's timezone (on time, no background job); one campaign at a time by priority; save, preview on mobile /
  tablet / desktop, publish, unpublish, duplicate, archive, delete drafts; version-checked edits; website permissions;
  audited; platform switch and limits.
- ✅ **86. Insights** — views, visitors, product and button clicks, orders and sales attributed (visitor clicked the
  campaign within 7 days before a website order; cancelled orders excluded), side by side per campaign.

Added 2026-10-10 — **Session timeout & automatic logout** (owner request: *Configurable Session Timeout & Automatic
Logout*; [module 30](modules/30-session-timeout.md)).
- ✅ **87. Automatic logout after inactivity** — server-side sessions (every token carries one; ended, expired or idle
  sessions refused with 401 *session expired*, audited); platform limits (default 5 min, range 1–120 min, warning
  30 s, longest session 12 h); business timeout (1/2/3/5/10/15/20/30/60 min or custom) with stricter role timeouts and
  the effective policy shown; only genuine activity counts (heartbeat after real input, `X-User-Activity` on changes;
  background requests, polling, notifications and offline sync never do); warning with *Stay Logged In* / *Log Out
  Now* and an unsaved-changes notice (nothing auto-submitted); one clock for all tabs, wall-clock based for sleeping
  devices; sign-in screen explains the expiry, Quick PIN still offered on trusted devices. Gaps closed on the way: the
  user-menu sign-out now ends the server session, every tab follows a sign-out, and saving other settings never
  writes a timeout the business did not choose.

Added 2026-10-10 — **Product User Manual** (owner request: *Product User Manual & PDF Download*;
[module 31](modules/31-product-user-manual.md)).
- ✅ **88. User Manual PDF** — *Manual ↓* under the sign-in form (no sign-in), `SShop_Product_User_Manual.pdf`: A4,
  Outfit, S'Shop colours, page numbers, cover with title, subtitle and numbered module directory with page numbers,
  then one page per module (one-line description, 5–8 bullets, implemented features only, platform-owner page marked).
  Content in `docs/manual/manual.json`, built by `scripts/manual/build.mjs`, versioned with the release. Gap closed on
  the way: the service worker no longer stores a directly opened file as the offline app page.

Added 2026-10-11 — **Quick PIN dots & testimonial visibility** (owner requests: *Dynamic Quick PIN Dots & Automatic
Centering*; *Testimonials Visibility Toggles & Publishing Controls*).
- ✅ **89. Dynamic Quick PIN dots** — the keypad starts with no dots and shows one centred orange dot per digit entered
  (no empty placeholders, so a 4-, 5- or 6-digit PIN's length is never shown); Backspace, clear, OK and the 6-digit
  maximum unchanged; authentication untouched ([module 28](modules/28-quick-login-pin.md)).
- ✅ **90. Testimonials: two-level visibility** — *Show testimonials* hides the whole section, page and menu link
  without touching each testimonial's setting; *Show this testimonial* works for every testimonial with a status line
  beside it; samples have a *Show in preview* switch (staff preview only, badged, never public) and become genuine only
  after confirmation and with the customer's own words (sample text refused on save); no empty carousel or menu link;
  drafts may be incomplete. Root causes fixed: the sample switch was disabled without explanation, and any switched-on
  sample blocked publishing ([module 23](modules/23-website.md)).

Added 2026-10-11 — **M-Pesa per business & STK Push sales** (owner request: *M-PESA Till Integration & Customer STK
Push*; [module 32](modules/32-mpesa-stk.md)).
- ✅ **91. M-Pesa Till / Paybill per business** — each business's own Buy Goods Till or Paybill accounts (Store / Head
  Office or Paybill number, Till number, account reference, Sandbox / Production) assigned to branches; Daraja
  credentials encrypted at rest (AES-256-GCM) and never shown again; test connection and test prompt (verification);
  STK Push off until verified and again after any change; per-account secret callback URL matched to the request;
  activity log with errors and masked phones. At the till: *Manual M-PESA | STK Push*, customer phone (07 / 01 / 254),
  amount, *Send STK Push* with Pending / Successful / Failed / Cancelled / Timed out, retry, stop waiting, cart locked
  while waiting, no duplicate prompts. The sale is validated in full, its stock held, and completed **exactly once**
  only after Safaricom confirms (callback or status query; late receipts filled in); failures release the stock and
  are kept; paid-but-unrecordable payments are flagged for attention, notified and completed by an authorised person.
  Gaps closed on the way: the platform's `MPESA_*` configuration served every business (customers would have paid into
  one shortcode) — now only the platform owner's own business; the callback overwrote the requested amount — the
  reported amount is kept apart and must cover what is due; test prompts can never pay for a sale; server
  configuration secrets are redacted from debug output.

Added 2026-10-11 — **Daraja Sandbox app in the existing Railway service** (owner request: *Daraja Integration in
Existing Railway Service*; [integrations/mpesa.md](integrations/mpesa.md)).
- ✅ **92. Platform Daraja app from Railway variables** — `MPESA_ENVIRONMENT`, `MPESA_CONSUMER_KEY`,
  `MPESA_CONSUMER_SECRET`, `MPESA_STK_PASSKEY`, `MPESA_SHORTCODE`, `MPESA_CALLBACK_URL` (earlier names still read);
  validated at start-up without ever stopping the server (problem shown and logged); callback URL must be HTTPS with a
  long secret path; OAuth tokens cached and renewed; platform-owner card with masked callback URL, address check, *Test
  OAuth* and *Test STK prompt* (also inside a demo or support session). Serves the platform owner's business and demo
  businesses (so the demo can show STK Push), never customers' businesses. STK Push sales, callbacks, status queries and duplicate protection are those of 91;
  tenants keep their own encrypted production accounts (91).

Health check 2026-10-11 (owner request: *health check of the S'Shop app; detect and fix gaps or bugs*).
- ✅ **93. Health check** — production healthy (health check, sign-in, manual, API and site answer in under 1 s; strict
  CSP, HSTS, framing denied; no errors in the service logs); M-Pesa release live (new routes answer, forged callbacks
  refused); full smoke suite 653/653 and 54 unit tests. Fixed: **jsPDF 3.0.4 → 4.2.1** (critical/high advisories incl.
  HTML injection in new-window output and image-decoding denial of service; receipt and report PDFs verified).
- ✅ **94. Railway config migration (before 2026-12-01)** — `railway.json` (Config as Code) is deprecated and stops on
  2026-12-01. Railway's automatic migration drops the Dockerfile builder and restart policy and renames the service,
  so it is done by hand with the owner: write `.railway/railway.ts` (Dockerfile build, `/healthz` with 120 s,
  restart on failure × 5), `railway config plan`, then apply.
  **Prepared 2026-10-15** (dry run only, nothing changed on Railway): `railway config migrate --service sshop` keeps
  `healthcheck: "/healthz"` + `healthcheckTimeout: 120`, leaves the Dockerfile builder as a comment (fine — Railway
  detects the root `Dockerfile`) and drops the restart policy (Railway's default is also *on failure*, 10 retries
  instead of 5; set 5 in the service settings if wanted). Steps with the owner: `railway link` (project S'Shop,
  production, service sshop) → `railway config migrate --service sshop --apply` (writes `.railway/railway.ts`,
  clears the service's Config File setting) → `railway config plan` (expect only healthcheck settings) → `railway
  config apply` → delete `railway.json`, commit, and confirm the next deploy uses the Dockerfile and passes `/healthz`.
  **Evidence (checked 2026-10-19):** docs.railway.com/config-as-code — "Existing `railway.json` / `railway.toml` files
  continue to work for services that already use them until **2026-12-01** (hard cutoff)"; docs.railway.com/
  infrastructure-as-code — "Existing Config as Code files stop being read on 2026-12-01 (hard cutoff)". The live
  service settings (Railway API, read-only) hold none of what `railway.json` sets: builder **RAILPACK**, no health check,
  no restart policy. So after the cutoff, deploys would go live **without the `/healthz` gate** (a broken build could
  replace the working one). The root `Dockerfile` is still used (Railway uses a root `Dockerfile` automatically:
  docs.railway.com/builds/dockerfiles). Railway CLI 5.57.2 has `railway config migrate` (dry run by default).
  **Migrated 2026-10-19 (owner applied):** the generated file would have **deleted all 25 service variables and
  disconnected the GitHub source** (IaC removes what the file leaves out) — caught in `railway config plan`, never
  applied. `.railway/railway.ts` now declares the GitHub source (main, waits for CI), every variable as `preserve()`
  (names only) and the health check; the applied plan was *0 add, 1 change (healthcheck → `/healthz`, 120 s), 0
  destroy*; a fresh plan reports *already up to date*. `railway.json` removed. How to change settings from now on:
  [deployment-railway.md](deployment-railway.md#railway-settings-as-code-railwayrailwayts).
  **Verified live** (deployment of c75e839, first without `railway.json`): built from the repository `Dockerfile`
  (stages `server` rust:1-bookworm, `web` node:22-alpine, final debian:bookworm-slim), *Healthcheck succeeded*, CI green.
- ✅ **95. Front-end dependency majors** — React Router 6.30 → **7.18** (advisory cleared; every link is absolute, so v7's
  relative-path change does not apply) and Tailwind 3 → **4.3** (configuration in CSS `@theme`). The upgrade tool's
  renames inside non-styling text were reverted (translation keys, the `outline` button variant, the website's
  button-style option, the public site which uses its own `site.css`); Tailwind 3's fixed line heights and the
  buttons' pointer cursor are kept. Regression: 41 screens (desktop, phone, dark) pixel-compared with the Tailwind 3
  build — identical within run-to-run noise — plus the interaction suites (dialogs, sign-in landing, invitations &
  step-up, onboarding import).

Added 2026-10-11 — **Daraja InvalidTransactionType** (owner request: *Fix Daraja InvalidTransactionType Error*).
- ✅ **96. Sandbox and Production kept apart** — Sandbox requests always use Safaricom's test shortcode 174379 as a
  Paybill (the business's real Till and Store numbers are kept for Production, where a Buy Goods Till sends Store →
  `BusinessShortCode` and Till → `PartyB`); each account shows OAuth, *STK request accepted* and *Test payment
  confirmed* separately (OAuth no longer reads as proof of STK); Daraja refusals explained (transaction type, passkey,
  credentials, shortcode, phone, callback); refusals logged with environment, transaction type and shortcodes, never
  secrets. Root cause: a Sandbox account carried the business's real Buy Goods numbers, which Daraja's Sandbox refuses.

Added 2026-10-11 — **Daraja production activation** (owner request: *Daraja Production Activation & Live STK Push*).
- ✅ **97. Production testing actions** — per account and for the platform configuration: *Test OAuth connection*,
  *Test STK Push* (a controlled real payment of KSh 1–100, default 1), *Check payment status* (asks Safaricom at once
  about the latest test) and the STK switch; production Buy Goods requests send Store → `BusinessShortCode`, Till →
  `PartyB`, `CustomerBuyGoodsOnline`; go-live steps documented. Production credentials stay encrypted per business,
  STK never switches on when saved, and S'Shop's own configuration never serves customers' businesses.

Added 2026-10-11 — **Barcodes: manual entry, scanning and inventory validation** (owner request: *Manual Barcode
Entry, Scanning & Inventory Validation Enhancement*; [module 19](modules/19-barcode-scanning.md)).
- ✅ **98. Barcode settings, inline entry, stock takes by barcode** — Barcode settings card (management, manual entry,
  camera, required at stock-in / stocktaking / sales) on the existing settings; one inline *Enter or scan barcode*
  field (stock take, stock-in, Record Sale via Enter in the search box) next to the shared camera scanner; product
  barcodes validated as text (leading zeros kept), unique per business with the clashing product named, enforced by a
  database index created safely once no duplicates remain, conflicts listed in settings; *Generate* a unique in-store
  EAN-13; stock takes count by barcode, unit by unit for tracked products (duplicates, foreign units and unknown codes
  flagged; missing units become loss adjustments through approvals), barcode-only counting when required
  (server-enforced), the session kept on the device until submitted. Gaps closed: tracked products could not be
  counted at all; a refresh lost the count; Enter in the till's search did nothing with a scanned code; product
  barcodes were unique only in application code.

Added 2026-10-11 — **Business onboarding & data migration** (owner request: *Business Onboarding, Excel Bulk Uploads &
Historical Data Migration*; [module 33](modules/33-onboarding.md)). Delivered in phases.
- ✅ **99. Onboarding centre, phase 1** — Settings → Business → Onboarding: migration cutover; Excel templates from the
  real fields (Instructions sheet, required marks, sample row, text codes, dropdowns incl. branch codes and payment
  methods); upload → column mapping (common alternative names, re-mapping) → colour-coded preview with per-row reasons →
  error report → import or send for approval (*Data import* workflow, never the uploader) → history and totals; posting
  re-validated and atomic; the same workbook once; records point to their batch. Datasets: **Products** (numbering,
  barcode rules, update mode, catalogue branches without stock), **Customers** (matched by mobile, never duplicated),
  **Opening stock** (stock-ledger opening receipts at the as-of date, item barcodes, never on top of recorded stock),
  **Expenses** (original dates, references once). Invalid stock or money rows never skipped.
- ✅ **100. Onboarding phase 2** — **Sales history** (one historical sale per receipt reference: original receipt number
  and date, lines, discounts, payment and salesperson; no stock movements, no loyalty points; in reports for their
  dates; refused after the cutover, for credit, or when lines disagree), **Credit sales & opening balances** (original
  due dates for aging, amounts paid before S'Shop, opening balances without invented items; imported overdue debts never
  trigger automatic reminders), **Loyalty opening points** (ledger opening entries, earned − redeemed = balance, once per
  customer). Customers' spend, visits and tier update with the original dates. Gap avoided: the purchase helper would
  have stamped *last purchase* as today.
- ✅ **101. Onboarding phase 3** — **Orders** (closed ones as frozen history: no sale, no stock; outstanding ones into
  Orders with their original date, confirmed stages holding stock, refused line by line when stock is short) and
  **Stock transfers** (received / cancelled as history without moving stock again, *in transit* as dispatched to receive
  in S'Shop, *pending* as drafts). **Migration reconciliation** (filters: dataset, batch, branch, dates; Excel export) and
  **Complete onboarding**, refused while any difference or unfinished import remains. Gaps closed: an imported delivered
  order could have been moved to *Completed* and created a duplicate sale; history orders no longer sit in *Active*.
- ✅ **102. Branded loader** — every generic spinner in the app replaced by one `SShopLoader` (official mark inside a
  rotating brand ring; sm / md / lg; light and dark; reduced motion; delayed fade-in against flicker), plus a boot
  splash in `index.html`. Buttons, pages, dialogs, scanner, receipts, reports, onboarding, website management, sign-in.
- ✅ **103. Import progress & result** — imports run in the background (still one all-or-nothing transaction) with real
  progress (stage, records processed, %) on a shared S'Shop-branded progress screen, a success / partially completed /
  failed summary with an import reference, the Excel import report, reopening after a refresh, no double submission,
  and interrupted imports marked failed instead of hanging. Gap closed: the rows-to-fix report was empty after a
  partial import (left-out rows are now included).
- ✅ **104. Dialogs close only on purpose** — outside clicks / taps never close any dialog or sheet; every one has a ×;
  typed but unsaved input is protected (× / Escape / swipe-down ask *Discard changes?*), Escape closes freely when
  nothing would be lost; saves and existing flows unchanged. Applied centrally in the shared dialog, sheet and
  responsive dialog components. Gap closed: phone sheets had no × and a swipe could silently discard a form.
- ✅ **105. Users are invited** — no administrator-chosen PIN: a single-use 72-hour set-up link by email, the person
  sets their own PIN, statuses (invitation pending / sent / activation pending / expired / active / disabled),
  resend, copy a new activation link, WhatsApp sign-in guidance (no secrets), a once-only Quick PIN offer, and
  **step-up sign-in** so "Full sign-in needed" asks for the full PIN and continues the same request.

**Loyalty rewards, split payments, credit & configurable POS** ([module 34](modules/34-loyalty-payments-pos.md)):
- ✅ **106. Loyalty earning & redemption controls** — earning base (net payable default / gross), points never earned
  on value paid with points (explicit), rounding, redemption window (manual + scheduled), minimum points, rate, hard
  50 % maximum per sale enforced by the server, optional supervisor approval, audited settings.
- ✅ **107. Split payments & partial payment → credit** — several payment lines (cash, M-PESA/STK, others, points) on
  one sale, remainder on credit linked to the sale, stock deducted once, receipts / history / statuses / reports.
- ✅ **108. Landing page after sign-in** — business default and personal choice, after picking the branch.
- ✅ **109. POS display** — compact / image cards / list rows, images, tablet & desktop columns, price visibility;
  product-list thumbnails and prices.
- ✅ **110. Top-selling products** — quantity / value / number of sales, today / week (Monday) / month, Gold ★★★★★,
  Silver ★★★, Bronze ★★, predictable ordering with search and categories (S'Shop has no favourites / manual order to preserve).
- ✅ **111. Invitation links valid for 10 minutes** — user invitations get their own single-use link kind (`invite`,
  10 minutes, enforced by the server); every new invitation ends older invitation / set-up links; the email and the
  set-up page (with a live countdown) say *This invitation link expires in 10 minutes*; expired or replaced links say
  **Invitation expired** — ask the administrator, who can send a fresh one (Users → ✉); used links say so.

**Tenant partial white labelling** ([module 35](modules/35-white-labelling.md)):
- ✅ **112. Branding & White Labelling settings** — enable switch (off), the existing business logo (upload / replace /
  remove, light & dark preview), where the logo appears (login, sidebar & header, dashboards, POS, receipts & documents,
  customer pages & emails, website, favicon / PWA), preview, audited, *Business Branding Updated* with one Log Out.
  Logos are checked by their bytes (PNG / JPEG / WebP; SVG converted to WebP in the browser) and carry a fingerprint
  (ETag, 304) so a new logo shows at once. The placements take effect in 113.
- ✅ **113. Business logo applied** — login form icon from trusted tenant context only (large S'Shop marketing logo
  kept unless the business's own login address opts in), sidebar / header, dashboards, POS header, receipts and
  documents, customer pages and emails, website sync, favicon / PWA; *Powered by S'Shop* kept; platform admin always
  S'Shop; versioned logo URLs, no cross-tenant caching. Branding off keeps today's behaviour exactly (the logo stays
  where it already showed: sidebar, receipts, customer pages, website).

**POS refinements, sale dates & receipt logo** ([module 36](modules/36-pos-dates-receipts.md)):
- ✅ **114. POS stock visibility & images first** — POS Display switches *Show available stock count* (on) and *Show
  products with images first* (on); ordering: top sellers → sellable → with images → name, server-side.
- ✅ **115. Editable sale date** — Sale Date on New Sale (today by default, earlier dates with `sales.backdate`, never
  future) and per-product dates in the cart; real entry time kept; stock movements on the line date with no negative
  history; product reports by line date, payments / receipts by sale date; M-Pesa confirmations never backdated.
- ✅ **116. Receipt logo transparency** — no black box behind transparent logos in the PDF; one logo preparation for
  preview, PDF, image, print and shared links.
- ✅ **117. New Sale category strip** — warm-cream strip, one scrollable row, overflow arrows, selected category kept
  in view.

**Mobile session & app updates** ([module 30](modules/30-session-timeout.md#roadmap-118--branch-selection-per-person-device-data--app-updates-mobile)):
- ✅ **118. Branch selection, per-person device data & app updates** — the Current Branch is forgotten at sign-out and
  chosen afresh at every sign-in (full or Quick PIN: one branch, the default branch, or the picker), validated by the
  server; carts and stock-count drafts kept per person; a new release noticed while the app is open (Refresh, or
  applied on the next screen change when nothing is unsaved); old build files dropped by the service worker.

**Product health clinic follow-ups** (2026-10-15 report):
- ✅ **119. Unknown API addresses answer 404** — `/api/*` paths with no route returned the app page (200, HTML),
  so an older app version calling a removed endpoint got a confusing error; now a JSON 404 (`not_found`).
- ✅ **120. Clearer report dates** — Sales report: *Sale date* (the effective date, first) and *Recorded at* (the real
  entry time); Stock Movement: *Date* and *Recorded at* — backdated records read correctly.
- ✅ **121. Closing the books** — Settings → Business → *Closing the books*: a *Books closed up to* date (yesterday at
  the latest, `settings.business`, audited). Nothing dated on or before it can be recorded, changed or cancelled:
  backdated sales and their lines, cancelling such a sale (a return dated today still works), stock received with
  an earlier date, expenses recorded or voided. Today's trading is never blocked. Onboarding history keeps its own
  cut-over.
- ✅ **122. Dependency security scanning** — CI runs `cargo audit` (RustSec) and `npm audit --audit-level=high` on every
  push and weekly (Monday 06:00 UTC); reported as its own job; scheduled runs never cancel a push's run.
  The audit job is `continue-on-error`: it never gates deploys. On 2026-10-21 it reported two new Rust advisories
  (RUSTSEC-2026-0381 `webpki-roots`, RUSTSEC-2026-0382 `rustls-webpki`: TLS certificate checks for outgoing calls →
  updated to 1.0.10 / 0.103.16) and one npm advisory (`vite` / `launch-editor` → 7.3.7); all fixed the same day.
- ✅ **123. Smaller first download** — the translation dictionary (464 KB source, the largest part of the main script)
  is downloaded only for Kiswahili, French or Arabic; English needs none of it. Text shows in English until it
  arrives; the service worker keeps it for offline use.
- ✅ **124. Customer credit statements** — Customer → *Statement* (with `credit.view` and `customers.view_credit`):
  all time, this year or the last 90 days; opening balance, every charge (sale on credit), payment, return / recall
  and write-off with a running balance, closing balance (always equal to what the customer owes) and what is still
  to pay with due dates. A4 PDF to download or print, business logo per Branding (*Receipts & business documents*),
  transparency kept as on receipts. `GET /api/customers/{id}/credit-statement?from&to`.
- ✅ **125. Dependency advisories fixed** — the first audit run (122) found 7 Rust advisories: `quick-xml` 0.31 (2 high:
  memory / CPU exhaustion from a crafted spreadsheet — onboarding imports read customer files) → `calamine` 0.26 →
  0.36 (quick-xml 0.41); `rustls` 0.23.45 (4 low / medium) → 0.23.46; `rsa` (Marvin timing, no fix exists) is only in
  the lockfile through sqlx's MySQL driver, which is not built — recorded as ignored in `.cargo/audit.toml` with the
  reason.
- ✅ **126. Statement lines in the reader's language** — each statement line also carries its parts (payment method
  and reference, return kind and the sale it was for), so the screen words it in Kiswahili, French or Arabic; the PDF
  keeps English wording (its built-in font cannot draw Arabic script).
- ✅ **127. Test suite independent of outside services** — the exchange-rate address is configurable (`FX_URL`,
  default the provider); CI and the local runner point it at the suite's own stand-in (as for Paystack, Resend and
  Daraja), so an internet blip at open.er-api.com can no longer stop a full run (it did three times); test sessions get the longest idle time and session so a slow machine cannot expire them.
- ✅ **128. iPhone home screen (Progressive Web App)** — staff install S'Shop from Safari (Share → *Add to Home
  Screen*) and it opens full screen; the sign-in page and each ordering link show that tip on iPhones (dismissible,
  hidden once installed). A business's ordering link installs as the business: its name, its logo (PNG / JPEG,
  where it shows on customer pages) and its ordering page (`/api/public/{slug}/order.webmanifest`,
  `/api/public/{slug}/touch-icon`, falling back to S'Shop's icon). Apple's full-screen tags added for older iPhones.
  The installed app keeps its own storage, so people sign in once more inside it (how iOS works).

**Website prices, hidden-price ordering & imports** (brief *Website Product Price Visibility, Hidden-Price Ordering &
Product Import Enhancements*):
- ✅ **129. Per-product price visibility** — Settings → Website → Products → each product: *Use global setting*
  (default) / *Show price* / *Hide price*, with the effective status shown. One server rule for the website and the
  ordering link (cards, details, search, categories, catalogue): a product's own setting wins, else *Show product
  prices*; a hidden price never leaves the server before an order.
- ✅ **130. Hidden-price products in the cart** — a hidden-price product that can be ordered shows *Add to Cart* and
  *Price revealed after order submission*; the business's *Ask on WhatsApp* / call / enquire stays as a second
  action. Carts show a total only when every price is public, otherwise the public subtotal and how many items are
  priced after the order. After a successful submission the confirmation, tracking page and the customer's own order
  history show the prices recorded with the order (snapshot), line totals and total — never for a failed submission or
  another customer's order; *Submitting is not a payment*. Product page: no empty space under the purchase actions.
- ✅ **131. Selling price optional on import** — *Selling Price* not mandatory in the product templates; blank = not
  set (NULL, distinct from a real 0); *Price on Request* on the website / ordering link; no checkout total, payment or
  free sale for a product without a price; prices set later in Products.

**Face ID & biometric sign-in** (brief *Face ID & Biometric Quick Login Enhancement*):
- ✅ **132. Passkey / biometric sign-in (WebAuthn)** — Settings → Preferences, below Quick Login PIN: *Face ID &
  Biometric Login* (off by default): set up (full PIN re-entered first, then the device's own Face ID / fingerprint /
  screen-lock prompt), registered devices (registered / last used), remove. Server-side verification with the
  audited **webauthn-rs** (single-use, short-lived challenges; origin, RP ID, credential ownership, user verification);
  only the public-key credential is stored, per user and business; audited. Sign-in offers *Sign in with Face ID /
  Biometrics* (or *Passkey*) only where this device has registered one; then the same branch selection, permissions,
  session limits and audit as any sign-in; a passkey session needs the full PIN for sensitive actions like a Quick PIN
  session. Quick PIN and biometrics are independent. Cancelled or failed prompts never sign in. No camera, no face
  images, no biometric data ever reach S'Shop.
  Built: `routes/passkeys.rs`, migration 0038, `lib/passkey.ts`, Preferences card, login button; business and platform
  switches; ends on PIN change / reset and *Sign out everywhere*; real-ceremony tests with a software authenticator
  ([module 37](modules/37-face-id-biometric-login.md)).

**Item barcodes, labels & scan-to-clear** (brief *Advanced Item Barcode Generation, Physical Labelling, Hardware
Integration & Scan-to-Clear*, 2026-10-18). Builds on the existing individual-item tracking (`stock_items`, roadmap 98):
units already carry their own barcode through receiving, sales, transfers, stock takes, returns and write-offs, and the
server already refuses a unit sold twice, from another branch or another product.
- ✅ **133. Seven-digit item barcodes & platform registry** — S'Shop-generated item barcodes: 7 digits `1000000–9999999`
  from a cryptographically secure generator (never sequential), allocated through a platform-wide registry (unique
  across every business, branch and status; never recycled — sold, returned, written-off or replaced codes stay
  retired). Manually entered item codes can never take a code that S'Shop issued to another business, and an item code
  this business used before is never given to a new unit. Capacity on Platform → Security & features (used / left,
  warning at 70 %, documented move to longer codes). Permissions *Generate item barcodes*, *Print & reprint labels*,
  *Reveal full item barcodes*, *Replace item barcodes*, *Enter barcodes manually at the till*; audit for each.
  Product-level (shared / manufacturer) barcodes unchanged.
- ✅ **134. Barcode stickers (Code 128)** — the standard S'Shop sticker: business name (never the branch) · Code 128 bars
  with quiet zones · the 7-digit number · the product name small (truncated to fit); white background, black bars, no
  price, no internal ids. Sizes 40×25, 50×30, 60×40 mm or custom; default size in Settings → Stock. Preview, Print
  (browser / system printer, one label per page or A4 sheets), Download PDF, Download PNG; single labels and batches.
  Label printers that speak ZPL or TSPL get a file to send. Scanner-readable output verified in tests.
- ✅ **135. Receiving with generated barcodes** — Stock → Receive, individually tracked product: *Generate barcodes* (one
  per unit) or *Scan / enter existing* (per the stock settings) → labels ready → print → *I have attached the labels*
  → optional verification scan of every label (*Require barcode verification before completing a receipt*) → the
  existing receive / approval flow. Product details → Barcode Management: print or reprint labels of units in stock,
  replace a damaged label (new code, old one retired), *Label existing stock* when a product switches to individual
  items with stock already on hand. Opening-stock import can generate the item barcodes.
- ✅ **136. Concealed item barcodes** — after receiving, an item barcode shows as `****047` in products, stock, stock takes,
  transfers, sales, Record Sale, search, reports and exports — masked by the server, not only on screen. Full values
  only through audited actions: printing / reprinting labels and *Reveal* (permission). The printed sticker keeps the
  full code. Scanning still works everywhere (the server compares the scanned code).
- ✅ **137. Scan-to-clear at the till** — Settings → Sales & Payments → Barcode controls: *Allow manual barcode entry during
  sales* (default **off**): barcode-controlled items are cleared only by scanning (handheld / camera); typing is refused
  by the server on every route that clears stock (sales, credit sales, exchanges, order completion). Each cleared line
  records how its barcode was captured (scanner / camera / typed). Settings → Stock → Barcode controls: *Allow manual
  barcode entry during stock receiving* (default on), *Generate unique barcodes automatically*, *Require barcode
  verification before completing a receipt*. Keyboard-style scanners cannot be told apart from fast typing with
  certainty — the capture source is recorded as information, not proof; camera scanning is the stronger option.
- ✅ **138. Barcode hardware** — Settings → Stock → Barcode hardware: label printer type (system / browser printing,
  ZPL, TSPL), default label size, *Print a test label*, *Test your scanner* (shows what arrived and how), connection
  guidance for USB / Bluetooth / Wi-Fi printers and scanners and phone cameras; honest limits (no direct printer access
  from iPhone or most browsers — printing goes through the system print dialog or the printer's own app).

**Website light / dark mode** (customer question, 2026-10-18):
- ✅ **139. Theme follows the visitor's device** — with Theme = *Both (visitors choose)*, a first visit opens in the
  visitor's own light / dark setting; the sun / moon toggle then remembers their choice. The Theme setting explains where
  the toggle appears and that it needs Publish.

**Product price filter & phone navigation** (brief *Product Price Filter & Mobile Bottom Navigation Enhancement*):
- ✅ **140. Price status filter** — Products → after *All categories*: *All prices* · *Price set* (a genuine 0 included) ·
  *Missing price* (NULL); server-side, combined with status, category and search; counts and pages follow; the price
  column shows *Not set* while filtering for missing prices.
- ✅ **141. Products on the phone bar** — bottom bar **Home · Sales · Stock · Products · More**; Orders moves to More
  (module, permissions, links and notifications unchanged; the new-order count shows on *More* and on the Orders tile);
  Products is highlighted on the catalogue and product pages; desktop sidebar unchanged.

**Stock balances from Excel** (brief *Prefilled Stock Balances Template, Bulk Upload & Inventory Reconciliation*):
- ✅ **142. Prefilled stock balance sheet, upload & reconciliation** — Stock → Stock balances (and Onboarding):
  *Download template* (products × the person's branches, today's balance, recorded cost, today's date, product barcode
  or masked item barcodes, notes; hidden ids and the balance at download; Item Barcode Details sheet, masked unless the
  person may reveal), *Upload stock balances* (preview Current · Uploaded · Difference · Unit cost · Status; quantities
  are target balances, never additions), *Download current balances*. Refuses duplicates, unknown or changed rows,
  other branches, future / closed dates, sheets older than the latest movements, past dates before later movements and
  tracked products that do not match their labelled items; applies as audited count adjustments with the effective
  date, linked to the upload; approval when adjustments need it; never applied twice.

**Product browsing context** (brief *Preserve Product Filters & Browsing Position After Editing*):
- ✅ **143. Catalogue remembers its filters and position** — status, category, price status, search and page in the
  URL (refresh-safe, shareable); product page and edit screen return to it (app Back, browser Back, save, cancel) with
  the scroll position; the list refreshes after edits (an edited product leaves filters it no longer matches) and an
  emptied page moves to the nearest page with results; remembered per business, for the browser tab only.

- ✅ **144. Stock balances card in Onboarding** — Settings → Onboarding shows the Stock balances panel (identical to
  Stock → Stock balances: Download template · Upload stock balances · Download current balances · recent uploads, same
  preview and progress) in the place of *Opening stock balances*, which stays available at the end of the list (owner
  choice 2026-10-19: it still creates barcode-tracked opening items and feeds the migration reconciliation).

**Business workspace addresses** (brief *Tenant-Specific URL Paths & Persistent Workspace Navigation*):
- ✅ **145. `s-shop.store/{business-slug}/…`** — every signed-in screen under the business's slug; sign-in (all methods)
  opens it, a bookmark opened before sign-in is honoured; links, refresh, direct links and Back/Forward keep it; old
  unprefixed addresses redirect. The slug is generated at creation, unique (also against former slugs), never a reserved
  word, and permanent (renaming keeps it); an administrator may change it with a full sign-in and confirmation; old
  slugs are kept and keep resolving (workspace, ordering link, sign-in page, logo, website). The address is never
  access: another business's slug sends the person back to their own workspace, a business of the same tenant asks to
  switch. Business profile: *S'Shop Workspace URL* with Copy link. [modules/39](modules/39-workspace-addresses.md).

**Documentation as part of the workflow** (brief *Continuous documentation, roadmap, scoping and user manuals*):
- ◐ **146. Documentation kept current with every change** — purpose: the roadmap, technical docs and manuals always
  describe the working system. Scope:
  - **Status board** at the top of this file (live, in progress, open items with priority, owner, dependency, next
    action); stale marks corrected (127 ✅).
  - **Release notes** ([release-notes.md](release-notes.md)), newest first, only verified-live changes, from the
    commit history since 2026-10-03.
  - **Step-by-step guides by role** in the Product User Manual (Everyone, Cashier / Salesperson, Branch manager /
    Supervisor, Business administrator, Platform owner — 33 tasks using the screens' own labels), PDF 1.8 and a
    generated Markdown copy ([user-guide.md](user-guide.md)) from the same `manual.json`.
  - **Accuracy pass:** architecture (all 37 route modules, all 66 tables, sessions, sign-in methods, encryption,
    webhooks, integrations), deployment (undocumented variables, Railway settings as code), development
    (conventions); dated reports marked as snapshots with what is still open.
  - **Permanent workflow:** *Definition of done* in [development.md](development.md#definition-of-done-every-feature-fix-or-removal),
    `CLAUDE.md` for every session, and `scripts/docs_check.py` in CI (fails on an unindexed module doc, a broken
    link, a user guide older than its source, or architecture not checked against the latest migration).

  Acceptance criteria: CI fails when a module doc is not indexed or a docs link is broken (checked by adding a test
  file); the PDF opens with a directory whose page numbers match, and no guide task is cut at a page edge (the build
  refuses); every guide step names a screen or button that exists in the code; the Status board lists every open ⏳ /
  ◐ item.

**Credential rotation after Railway's security incident** (Railway report 2026-10-22, updated 2026-10-26):
- ◐ **149. Rotating every stored credential safely** — purpose: Railway recommends that every customer rotate the
  sensitive variables stored before 21 October (attackers decrypted a subset of customers' environment variables).
  `DATA_ENCRYPTION_KEY` protects the businesses' M-Pesa credentials in S'Shop's database, and changing it used to make
  them unreadable. Scope: `DATA_ENCRYPTION_KEY_PREVIOUS` — at start-up every stored credential that only the previous
  key reads is re-encrypted with the new key (row by row, counts logged, never values); unreadable values alert the
  owner; empty fields are skipped. Runbook for every variable, in order, with checks:
  [deployment-railway.md → Rotating credentials](deployment-railway.md#rotating-credentials).
  Acceptance criteria: unit test (only previous-key values are re-encrypted, current-key values untouched, neither key
  → reported); end to end on a scratch database: a credential stored under key A is *unreadable* with key B alone,
  *rotated* with B + previous A, *readable* with B alone afterwards, and no key or secret appears in the logs.

**Error alerting** (owner question 2026-10-21; owner chose in-app + email):
- ◐ **147. Error alerting for the platform owner** — built and tested ([module 40](modules/40-error-alerting.md)); live verification pending. — purpose: failures that need action reach the owner instead of
  sitting in the logs (evidence: on 2026-10-16 Railway refused a website domain — *custom domain limit for the plan*
  — and only the person on screen saw an error). Scope: server faults (database / internal errors, crashes inside a
  request), integrations refusing (Railway, Paystack, Daraja… — the 502 *upstream* errors), failed background jobs,
  the service restarting without a new deploy, the database not answering. Not alerted: normal business answers
  (4xx: wrong PIN, validation, permissions) and a website shown as *temporarily unavailable* (a deliberate 503).
  Grouped by fingerprint (same error, same place): the first occurrence alerts, repeats only count, a repeat alerts
  again after 6 h or after it was marked resolved. Delivery: in-app notification to the platform admins and an email
  to `PLATFORM_ADMIN_EMAILS` (email log). Settings → Platform → *Errors & alerts*: open / resolved list with count,
  first / last seen, where; *Resolve*; *Send test alert*. Messages are shortened and long tokens removed; request
  bodies and headers are never stored.

  Acceptance criteria: *Send test alert* creates an event, a bell notification for the platform admin and an email
  log row (sent through Resend); a 4xx answer creates no event; the same error twice is one event with count 2 and
  one notification; a resolved event that happens again reopens and alerts; a panic inside a request answers 500
  (not a dropped connection) and creates an event; a business administrator gets 403 on the errors API; a restart
  with the same deployment id creates *Service restarted*.

**Approval eligibility** (brief *Fix Inconsistent Workflow Approvals & Administrator Authorization*, 2026-10-21):
- ◐ **148. Approvals explain who can decide; requests nobody can approve are caught** — reported: Pablo Niche, *Deactivate
  product: AMOUAGE SUNSHINE EDP 100ML* waiting with only *View*, Product deactivation → Administrator.
  **Root cause** (from the code and the screen; reading production data was not permitted from this machine): the card
  showed no *Next approver*, which appears whenever at least one person may decide the step — so **nobody** could:
  the step is *Administrator* (full access), the request was made by its only full-access person (*Pablo*), and makers
  never approve their own requests. The earlier deactivation was approved because someone else had requested it.
  The screen hid the buttons without saying why, and nothing told anyone the request could never move.
  Consistency hardening (no behaviour change today): the engine judged approvers by their role's permissions while
  every other check uses **effective** permissions (role + personal exceptions); personal exceptions cannot include
  approval rights today, so both gave the same answer — the engine now uses the same source, so a future exception
  cannot make the Approvals screen and the decision disagree.
  Scope: one eligibility function with reasons (requester, earlier level, assigned to another approver, no
  permission, not of this business) used by the list, the details and approve / reject; *Assigned to* and the
  reason on every card; **View** details (type, requester, date, level, assigned to, status, history); requests nobody
  can approve flagged with why and the controlled ways forward (change the step's approver — waiting requests follow,
  audited — or withdraw), and the requester and workflow managers notified at once; Workflow engine warns while only
  one person has full access; one waiting deactivation request per product. Maker-checker unchanged: requesters never
  approve their own requests, nobody decides two levels, nothing is approved or reassigned automatically.

  Acceptance criteria (smoke 148): the sole administrator's own request shows *own request* and *nobody can approve*,
  approving it is refused with the reason, and they are notified; a second request for the same product is refused;
  after assigning the step to a role the waiting request is decidable by its members; a salesperson is refused with
  the reason; approval deactivates the product once and a second approval is refused; rejection leaves it active; a
  level-1 approver sees why they cannot decide level 2 and another person can; approval rights cannot be given as a
  personal exception.

Towards the end (owner decision 2026-10-06 — deferred, not dropped):
- ⏳ **28. Database backups** — Railway scheduled backups (daily, keep 7+) and a tested restore.
- ◐ **29. Email sender & public URL** — `PUBLIC_URL=https://s-shop.store` ✅ (2026-10-07). Pending: verify `s-shop.store` as
  a sending domain in Resend (DNS records) and set `MAIL_FROM` (e.g. `S'Shop <noreply@s-shop.store>`). **Now needed for
  58–61**: until then Resend's test sender only reaches the Resend account owner, so applicants and new administrators
  receive nothing (the email log shows Resend's refusal). Optionally `RESEND_WEBHOOK_SECRET` for delivered / bounced.
- ◐ **30. Custom domain** — replaced by **s-shop.store** ✅ (2026-10-07: live with HTTPS, Paystack webhook
  `https://s-shop.store/api/webhooks/paystack`). Pending: `www.s-shop.store` (add it on Railway + CNAME, or redirect at the
  registrar).
- ⏳ **31. Region** — move app + database together to an EU region (with a backup/restore window).
