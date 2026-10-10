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
