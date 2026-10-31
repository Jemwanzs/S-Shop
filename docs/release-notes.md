# S'Shop release notes

What changed in S'Shop, newest first. Only changes that are **live and verified** are listed (CI green, deployed to
https://s-shop.store and checked). Numbers in brackets are roadmap items in [scope.md](scope.md); the module docs
hold the full behaviour.

**Adding an entry:** when a roadmap item is verified live, add one line under its date (create the date heading if
needed): what a user or operator notices, then the roadmap number and module link. Fixes and security changes count.

---

## 2026-10-31

- **Approvals and bulk actions, tightened** — if a workflow is changed while you have a request open, your decision is
  refused as *Request changed* (open it again) instead of being applied to a step you did not see; *no action needed*
  notices name the right next level, use the business's time zone and never go to the requester; next-level approvers
  get one notice per bulk; a bulk item that fails says so plainly (details reach the owner's error alerts) and never
  stops the rest; switching Tenant Administrator self-approval changes only that setting. (152,
  [module 15](modules/15-approvals.md))
- **Key rotation without a payment gap** — `DATA_ENCRYPTION_KEY` is now rotated in three deploys with
  `DATA_ENCRYPTION_KEY_NEXT`, so M-Pesa keeps working while old and new versions overlap. (152,
  [deployment](deployment-railway.md#rotating-credentials))

## 2026-10-29

- **Find free and unpriced products** — Products → Price status now also filters *Zero price*, *Zero or missing price*
  and *Above zero* (a price of 0 and no price stay different). (151, [module 6](modules/06-products.md))
- **Deactivate several products at once** — tick products (or a whole page, or every matching product), review what
  will happen to each, add a reason and confirm. With approval on, one request per product, the products stay active
  until approved, and nothing is ever requested twice. (151)
- **Approve or reject several requests at once** — tick requests in Approvals → Waiting, add one comment (saved on
  every request) and confirm. Each request is still checked and decided on its own: levels are never skipped, and one
  that someone else just handled is reported, not decided. (151, [module 15](modules/15-approvals.md))

## 2026-10-27 (evening)

- **Several approvers per approval level** — each level of a workflow can name several roles and people (Administrator,
  Manager, Supervisor, a person…); any one of them decides it and the first decision counts. The others see it is
  handled (*Already handled — approved by … at …*) and are told who decided and what happens next. Add level still
  builds approvals in sequence. (150, [module 15](modules/15-approvals.md))
- **Tenant Administrators may approve their own requests** — on by default, switchable in Settings → Workflow engine;
  always by pressing Approve, recorded as self-approved; never for refunds, cancellations, credit write-offs or
  recalls, expenses or counter discounts. Everyone else still needs a second person. (150)
- Every approval decision in the audit trail now records the requester, approver and role, level, other approvers,
  self-approval, branches and outcome. (150, [module 17](modules/17-audit-notifications-search.md))

## 2026-10-27

Deployed together (Railway's security incident of 20–22 October had paused every build; nothing from 21 October
reached production until now).

- **Credential rotation, safely** — `DATA_ENCRYPTION_KEY` can now be changed: with the old key kept as
  `DATA_ENCRYPTION_KEY_PREVIOUS`, every stored M-Pesa credential is re-encrypted at start-up; unreadable ones alert the
  owner. Runbook for rotating every credential after Railway's incident. (149,
  [deployment](deployment-railway.md#rotating-credentials))
- **Approvals say who can decide and why** — every waiting request shows who it is assigned to and, if you cannot
  decide it, why (your own request, an earlier level you decided, assigned to another approver); requests nobody can
  approve are flagged at once with the way forward; **View** shows the full request and its history; Workflow engine
  warns when only one person has full access; one waiting deactivation request per product. (148,
  [module 15](modules/15-approvals.md))
- **Error alerts for the platform owner** — server faults, outside services refusing, failed background jobs and
  restarts reach the owner in the bell and by email, grouped; Settings → Platform → *Errors & alerts* with *Send test
  alert*. A crash inside a request now answers with an error instead of a dropped connection. (147,
  [module 40](modules/40-error-alerting.md))
- **Product User Manual 1.8** — step-by-step guides for everyone, cashiers, managers, administrators and the platform
  owner; release notes; roadmap status board. (146, [user guide](user-guide.md))
- **Security:** TLS certificate checks on outgoing calls (`rustls-webpki`, `webpki-roots`), `tokio` (RUSTSEC-2026-0386),
  `dompurify` (critical, via the PDF library), `vite` (developer tooling).

## 2026-10-19

- **Business workspace addresses** — every signed-in screen lives under the business's own address,
  `s-shop.store/{business}/…`. Links, refresh, bookmarks and Back/Forward keep it; old addresses redirect; a
  bookmark opened before sign-in opens after it. Another business's address never gives access. Business profile
  shows the *S'Shop Workspace URL* with Copy link; the address can be changed under control and old links keep
  working. (145, [module 39](modules/39-workspace-addresses.md))
- **Fix:** after signing in, the sign-in page no longer overrode the chosen landing page with the home page. (145)
- **Railway settings as code** — the deployment settings moved from `railway.json` (which Railway stops reading on
  2026-12-01) to `.railway/railway.ts`; health check `/healthz` now also held in Railway's own settings. (94,
  [deployment](deployment-railway.md#railway-settings-as-code-railwayrailwayts))
- **Stock balances from Excel** — download a sheet prefilled with every product and branch, count, upload, review
  Current → Uploaded → Difference for each row, apply (approval when required). (142,
  [module 7](modules/07-stock.md)) The same panel is in Settings → Onboarding. (144, [module 33](modules/33-onboarding.md))
- **Products keep your place** — filters, search, page and scroll position survive opening and editing a product.
  (143, [module 6](modules/06-products.md))
- **Price status filter** on Products (price set / missing), and **Products on the phone's bottom bar** (Orders
  moved to More, with its badge). (140, 141)

## 2026-10-18

- **Item barcodes & labels** — unique 7-digit item codes generated for individually tracked products, printable
  labels (printer, PDF, PNG, Zebra ZPL, TSC TSPL), masked codes for staff without reveal rights, scan-to-clear at
  sale, label settings and barcode hardware page. (133–138, [module 38](modules/38-item-barcodes-labels.md))
- **Fix:** barcode trace failed to open for some items (business date missing). (133)
- **Business website follows the visitor's light / dark setting**. (139, [module 23](modules/23-website.md))

## 2026-10-17

- **Face ID & biometric sign-in** with passkeys on trusted devices. (132, [module 37](modules/37-face-id-biometric-login.md))
- **Selling price optional** for products not yet priced; clearer product page layout. (130, 131)
- **Per-product price visibility** on the ordering link and website; hidden-price products can still be ordered,
  with prices shown after submission. (129, [module 5](modules/05-ordering-portal.md))

## 2026-10-16

- **iPhone home screen** — install tip on the sign-in page; the ordering link installs as the business. (128)
- Test suite runs without outside services. (127)

## 2026-10-15

- **Customer credit statements** (PDF, in the reader's language). (124, 126)
- **Closing the books** — no changes to closed periods; report date handling clarified; dependency scanning in CI;
  smaller first download. (120–123)
- Unknown API addresses answer a clear 404. (119)
- **Branch chosen at every sign-in**; device data kept per person; *new version available* banner. (118)
- **Editable sale date** and product-level dates. (115, [module 36](modules/36-pos-dates-receipts.md))
- POS stock count and products with images first; category strip; transparent receipt logos. (114, 116, 117)
- **Branding & white labelling** — the business logo where the business chooses (sign-in, sidebar, receipts,
  website, app icon). (112, 113, [module 35](modules/35-white-labelling.md))
- **Security:** jsPDF 4.2.1 (critical and high advisories fixed); dependency advisories fixed. (93, 125)
- Product User Manual 1.3.

## 2026-10-14

- **Invitation links** — users are invited and set their own PIN (link valid 10 minutes); **step-up sign-in** when a
  Quick PIN session does something sensitive. (105, 111)
- **Split payments** and partial payment left on credit. (107, [module 34](modules/34-loyalty-payments-pos.md))
- Loyalty earned on the amount paid; redemption window and 50 % cap. (106)
- **Landing page** after sign-in, chosen per person or per business. (108)
- POS display options; top-selling products with Gold / Silver / Bronze stars. (109, 110)
- Dialogs close only on purpose (no lost entries from a stray tap). (104)
- Platform: React Router 7, Tailwind CSS 4. (95)

## 2026-10-11

- **Business onboarding centre** — Excel templates, validation, approval and posting for products, stock,
  customers, sales history, credit balances, loyalty points, orders and transfers, with reconciliation. (99–101,
  [module 33](modules/33-onboarding.md))
- Branded loader and import progress. (102, 103)
- **Barcode settings**, inline entry, stock takes by barcode. (98)
- **M-Pesa per business** — each business's own Till / Paybill, STK Push at the till, Sandbox and Production kept
  apart, production activation checks. (91, 92, 96, 97, [module 32](modules/32-mpesa-stk.md))
- **Session timeout** and automatic logout; **Product User Manual** (PDF) linked from sign-in. (87–90,
  [module 30](modules/30-session-timeout.md), [module 31](modules/31-product-user-manual.md))

## 2026-10-10

- **Holiday & promotional campaigns** on business websites. (84–86, [module 29](modules/29-holiday-campaigns.md))
- **Quick Login PIN** on trusted devices. (83, [module 28](modules/28-quick-login-pin.md))

## 2026-10-09

- **Tenants and secure business access** — several businesses per tenant, business switch, platform support
  access with scope and time limit; custom domains for websites; premium storefront. (70–82,
  [module 27](modules/27-tenants-and-secure-access.md))

## 2026-10-08

- **Digital receipts** (stored 50 mm receipts, email, WhatsApp, secure link) and return / exchange reconciliation;
  sale ownership and data scopes; order alerts. (62–69, [module 25](modules/25-sales-ownership-and-visibility.md),
  [module 26](modules/26-receipts-and-reconciliation.md))
- **Onboarding emails**, one-time PINs, self-service PIN reset, applicant status page. (58–61,
  [module 24](modules/24-onboarding-and-recovery.md))
- **Website add-on** — business websites, management centre, domains, analytics. (51–57, [module 23](modules/23-website.md))
- Workflow changes reach approvals already waiting. (49)

## 2026-10-07

- **Platform owner** — tenant directory and monitoring, billing plans, invoices, Paystack payments, module-based
  packages, trials, grace and suspension. (34–46, [module 21](modules/21-platform-billing.md))
- **Action buttons** that show their state and refuse duplicates; floating quick sale. (47, 48,
  [module 22](modules/22-actions-and-quick-actions.md))
- **Live at https://s-shop.store.**

## 2026-10-06

- Combined **exchange screen**; offline POS; geofencing by branch; working days, trading hours and the business
  date; credit recall; strict barcode clearance; transfer receipt discrepancies; security headers; CI.
- Languages complete (English, Swahili, French, Arabic) including server messages; UI refinement pass. (9, 10)

## 2026-10-05

- Camera barcode scanning; leaderboards; My Dashboard and scoped data; display currency with live rates; request
  access instead of open sign-up; *Powered by S'Shop*; Pablo Niche demo business.
- **Fix:** cost prices no longer leak to roles without cost access; phone navigation crash.

## 2026-10-03 – 04

- **S'Shop first release** — retail, inventory, sales, credit, orders and ordering link, customers and loyalty,
  expenses, reports, approvals, roles, audit trail (Rust + React, one service on Railway).
- Custom product fields, configurable order statuses, multi-level approvals, threshold medals, deposits on credit
  sales. (1–4a)
