# S'Shop release notes

What changed in S'Shop, newest first. Only changes that are **live and verified** are listed (CI green, deployed to
https://s-shop.store and checked). Numbers in brackets are roadmap items in [scope.md](scope.md); the module docs
hold the full behaviour.

**Adding an entry:** when a roadmap item is verified live, add one line under its date (create the date heading if
needed): what a user or operator notices, then the roadmap number and module link. Fixes and security changes count.

---

## 2026-11-26

- **Website publication requests** — people with *Request website publication* (but not *Publish*) send their website changes for approval with an automatic summary of what changed; the Tenant Administrator (or the configured chain) reviews, previews and approves & publishes, returns for correction or rejects. Publication history on the website overview. (188, [module 23](modules/23-website.md))
- **Role editor: Save only when something changed** — *Nothing to save* until a permission, the name or description changes; *Save changes* then. (189, [module 14](modules/14-users-roles.md))
- **Fix: Quick PIN setup did nothing for some new users** — the reason (wrong current PIN, a PIN that is too easy to guess, or the full PIN reused) was not shown. It is now shown in the dialog; easy-to-guess PINs keep Save off; the Quick PIN fields have show/hide eyes. Any action that is refused anywhere in the app now says why. (187, [module 28](modules/28-quick-login-pin.md))
- **Manage branch prices** — Settings → Products → *Manage branch prices* (Tenant Administrators): each branch's prices in one table; set prices, reset to 0 or restore the default for one product, the selected ones, a category or the whole branch. A reset price stays 0 until a new price is set. (186, [module 06](modules/06-products.md))
- **Hide out-of-stock products at the till** — new role permission *View out-of-stock products when recording sales*; off for the Salesperson role. (186, [module 02](modules/02-sales-pos.md))
- **Fix: a price of 0 is never sold** — a product priced 0 shows *Price not set* at the till and *Price on request* online, instead of selling for 0. (186)

## 2026-11-25

- **My profile** — tap your avatar (top right, also on phones) for My profile, Edit profile, Change PIN, Face ID & passkeys, User preferences and Sign out. Change your name and add a profile photo (crop, replace or remove); email, role and branches stay with your administrator. The audit trail keeps the name you had when you acted. (185, [module 14](modules/14-users-roles.md))
- **Find customers as you type at the till** — by any part of the name or mobile number (07…, 01…, 254…, +254…) from the first letter or digit; pick one from the list (their number fills the M-Pesa field). No match: *+ Add new customer* without leaving the sale; a number already registered is refused, naming its customer. (184, [module 02](modules/02-sales-pos.md))
- **Edit the Tenant Administrator role** — the same pencil as other roles: switch off permissions you don't use; users, roles, settings, branches, approvals and the audit trail stay locked on. (183, [module 14](modules/14-users-roles.md))
- **Platform support access, bounded and fully recorded** — full access never changes a business's people, roles, security, approval rules, payment settings or billing, and follows its approvals; ending, signing out, the time limit or the business's revocation closes it and returns to the Platform Owner Console. Every session and everything attempted is in the confidential, permanent *Security audit* (Settings → Platform); changes support makes show in the business's audit trail as *S'Shop Support*. (182, [module 27](modules/27-tenants-and-secure-access.md))
- **Website and ordering link show in-stock products only** — on for every business (switch it off in Website → Products or Settings → Orders to show out-of-stock products marked *Out of Stock*). Also: *Limit cart quantity to available stock* (on) and *Show available stock quantity* (off: *In stock*, no number). Out-of-stock products can never be ordered; the website checks the cart against stock before checkout. (181, [module 23](modules/23-website.md))
- **Sign in with Face ID first** — a device with Face ID, fingerprint or Windows Hello sign-in set up opens on *Continue with Face ID / Passkey*; *Sign in with Email Instead* is one tap away. Real devices still to be checked. (180, [module 37](modules/37-face-id-biometric-login.md))
- **Credit sales: one item each, returned items, swaps** — a credit sale holds one item (a cart of several becomes separate credit sales; switchable). *Record payment → Returned item* settles a credit with an item the customer had paid for (only money actually paid counts). *Swap item* exchanges a credit item for another product as a linked sale. (179, [module 03](modules/03-credit-sales.md))
- **Credit sale shows the product sold** — photo, name and code as sold, quantity, price, discount and amount, even after the product is renamed. (178, [module 03](modules/03-credit-sales.md))
- **Fix: import progress** — a finished import could show *Finalizing* for a moment. (179)

## 2026-11-19

- **Camera scanning of gold, silver and coloured barcodes** — iPhones now read light barcodes on dark boxes (gold on black) and every phone tries enhanced pictures for gold, bronze, silver and faint print; *Zoom* where the camera has it, a tip when a barcode will not read (light, reflections, distance), and *Scan from photo*. S'Shop never guesses a code. Real phones and boxes still to be checked by the owner. (177, [module 19](modules/19-barcode-scanning.md))
- **One product, a price per branch** — Product → Edit → Pricing: *Marked selling price — {your branch}* changes only that branch; the default price covers the rest. The till, scans, orders, the ordering link and stock values use the branch's price; finished sales keep theirs. New permission *Set selling prices*. (176, [module 06](modules/06-products.md))
- **Branch filter on Stock and Products** — your branch, another of yours, or all of them added up; viewing only, receiving and transfers stay at your working branch. (176, [module 07](modules/07-stock.md))
- **Transfers confirm the destination's selling price** — both branches' prices shown; *Keep Destination Price* by default, or use the source's or a new one (confirmed, authorised by someone who sets prices there, applied when the stock arrives). Transfer approvals now list every product, quantity and price. (176, [module 08](modules/08-transfers.md))
- **Fix: a short reason is enough to delete stock** — *Request inventory deletion* accepts any reason, such as "Error" or "Duplicate"; the button now reads *Request Deletion*. It still goes for approval first. (175, [module 07](modules/07-stock.md))
- **Allow Multiple Barcodes per Product** — Settings → Products: on by default. Off keeps one barcode per product; barcodes already given keep working and can be removed. (174, [module 19](modules/19-barcode-scanning.md))
- **One product, several barcodes** — the same product in other packaging can carry another barcode: each finds it at the till and when receiving, counted on its own line and added up. An unknown code offers *Add barcode to this product*; another product's code is refused with its name. Receive stock has a large *Scan Barcode* button. (173, [module 19](modules/19-barcode-scanning.md))
- **Products with stock can move to one shared barcode** — no need to sell or write anything off first. Stock stays, old item labels still scan to the product, and the preview says *Ready to convert*, *Conversion requires review* (work in progress, with the reason) or *Blocked*. (170, [module 19](modules/19-barcode-scanning.md))
- **Stock adjustment: Others** — the last type, with *Added to stock* / *Removed from stock* and a required explanation, shown to approvers and kept in the history. (171, [module 07](modules/07-stock.md))
- **Delete stock received by mistake** — Stock → Movements → ⋯ → *Request inventory deletion* (or from a barcoded item). It goes for approval (on by default); after approval the stock is taken off, the original receipt stays marked *Reversed*, and the item's barcode can be used again. Sold or moved stock is not deleted. (172, [module 07](modules/07-stock.md))
- **Product Barcode Nature** — Settings → Products → *Barcode & Quantity Tracking* counts products using *Individual Items* and *Product Barcode (Shared)*. *Review and change products* switches many at once: select, preview each one (will change, already, or cannot change yet with the reason), confirm. Barcodes, stock and history stay; no product barcode has to be added. (169, [module 19](modules/19-barcode-scanning.md))
- **Fix: Excel Stock Tracking on update** — *Update existing products* ignored the Stock Tracking column; it now changes a product's tracking after the same checks, and refuses a product that cannot change yet with the reason. (169, [module 33](modules/33-onboarding.md))

## 2026-11-18

- **Camera scanning** — the same barcode counts again only 3 seconds after it was last counted, each barcode on its own clock; another barcode counts at once. While waiting the scanner shows *Please wait — same barcode ready in 2.1s*, with no sound. (163, [module 19](modules/19-barcode-scanning.md))
- **Stock taking on or off** — Settings → Stock → Stock settings → *Enable stock taking* (on by default, every branch). Off hides *Stock take*; past counts and counts waiting for approval are kept. (164, [module 07](modules/07-stock.md))
- **Quantity controls** — after a scan: a large quantity, green **+**, **−** and Delete; tap the quantity to set it or add to it. Products that give each item its own barcode keep + and − off and explain why. Also fixed: tapping a quantity no longer restarts the camera. (165, [module 19](modules/19-barcode-scanning.md))
- **Barcode sharing set once for the business** — Settings → Products → *Barcode & Quantity Tracking* → *Allow Shared Barcodes Across Multiple Items* (on by default) decides how new products are tracked. The product form shows one line instead of two cards, with *Use … for this product* for an exception. Existing products keep their tracking. (168, [module 19](modules/19-barcode-scanning.md))
- **Passkey login, named for your device** — Preferences shows *Face ID / Touch ID & Passkey Login* on iPhone and iPad, *Biometric & Passkey Login* on Android, *Windows Hello & Passkey Login* on Windows and *Device Authentication & Passkeys* elsewhere, with clear states (Ready to set up, Setting up, Enabled, Credential removed, Setup failed). A sign-in that does not go through says *Authentication unsuccessful* with *Try again* and *Use another sign-in method*. (167, [module 37](modules/37-face-id-biometric-login.md))
- **Security: PostgreSQL only** — SQLite and MySQL code removed from the server's dependencies (RUSTSEC-2026-0459 and RUSTSEC-2023-0071 gone), `zip` updated (RUSTSEC-2026-0548); no advisory is ignored any more. (166, [architecture](architecture.md))

## 2026-11-05

- **Replacing a barcode label** — *Replace label* now shows the current barcode and the new one first, and the item changes only when you tap *Save Barcode* or *Save & Print*. Until then the old label keeps working, and Cancel leaves everything as it was. Printing and downloads never change a barcode, and print-only screens have *Done*. Businesses can require approval (*Item barcode replacement*, off by default). Product details can print a shared barcode's label, and the product form shows the current barcode before Generate replaces it. (162, [module 38](modules/38-item-barcodes-labels.md))

## 2026-11-04 (evening)

- **Each website's own favicon** — Settings → Website → *Branding & Appearance* (the former Design tab) → Favicon: upload PNG, ICO or SVG, see it as a browser tab and a phone icon, then Publish. With none set a website uses its logo or first letter, never S'Shop's icon, including on its own domain, and it can be added to a phone with its own name and icon. (161, [module 23](modules/23-website.md))

## 2026-11-04 (late afternoon)

- **SEO health** — SEO & Organic Growth → *Analytics & SEO Health* lists what to fix, counted from the live website: missing photos and descriptions, duplicate titles, dead links in articles
  and more. It never estimates rankings or traffic; it links to Search Console, PageSpeed Insights and the Rich Results Test instead. (160, [module 23](modules/23-website.md))
- **Search engines** — guided Search Console and Bing set-up with verification codes, Google Analytics that runs only for visitors who accept, and Merchant Center guidance. (160)
- **AI-assisted article drafts, off until enabled** — a first draft from a business's own details, on demand or on a schedule it chooses, waiting for a person's approval by default, with a monthly limit and
  costs visible to the platform owner. Needs the platform owner to switch it on with an API key. (159, [module 23](modules/23-website.md))
- **Articles** — a business can write and publish articles on its website: drafts, review, scheduling, its own search details and a place in the sitemap. Thin articles and near-copies are
  stopped before publishing. Share buttons on articles and products open WhatsApp, Facebook or LinkedIn for the visitor to post. (158, [module 23](modules/23-website.md))
- **SEO & Organic Growth** — Settings → Website has a new section for each business's search presence: its title and description with a
  preview, what it sells and where it serves in its own words, the address customers can visit, which branches to list, and a checklist for
  Google Business Profile. Products and categories get suggestions made only from what is on record. Nothing is published until you do. (157,
  [module 23](modules/23-website.md))
- **Websites search engines can read** — every page of a business's website now carries its real content, links and structured
  data in the first HTML, so search engines and link previews read it without JavaScript. Unknown pages answer *not found*, each page
  has one address, a hidden price stays hidden there too, and categories are in the sitemap. The private app stays out of search.
  (156, [module 23](modules/23-website.md))

## 2026-11-04 (afternoon)

- **Face ID, fingerprint and Windows Hello, made reliable** — set-up now has a separate tap for your device's check, because
  iPhones refuse the prompt after the page has waited on the network; sign-in does the same. The sign-in button no longer
  depends on what the browser remembers, so a passkey set up in Safari also works from S'Shop on the Home Screen. Automated
  tests pass; **real iPhone and Android checks are still to be done**. (155, [module 37](modules/37-face-id-biometric-login.md))
- **Says what each device does** — *Enable Face ID / Passkey Login* on iPhone, *Biometric / Passkey* on Android, *Windows Hello /
  Passkey* on Windows, with a clear state: ready, enabled here or on another device, unavailable or unsupported. (155)
- **Fixes:** a wrong PIN during set-up now says so (it showed nothing); a cancelled or refused check says why; a passkey
  already on the device or removed from S'Shop is explained. (155)
- **Security:** removing a passkey or switching them off now needs a full sign-in. (155)

## 2026-11-04

- **Scan the same barcode again and again** — at Record Sale, Stock → Receive and Stock take, each scan of a shared
  barcode counts one more unit, with a live counter beside the scanner, − / + and typed corrections, and a session total
  across products. The till never goes above what is available. (153, [module 19](modules/19-barcode-scanning.md))
- **Scan a whole delivery** — Stock → Receive → *Scan delivery* builds a list of several products; nothing reaches stock
  until you receive it, all of it or none of it, and approvals still apply line by line. (153,
  [module 7](modules/07-stock.md))
- **Individual barcodes stay one unit** — scanned again they say *Already scanned — 1 unit captured*. A stock take now
  shows units still missing and labels that are misplaced, unexpected or unknown. (153)
- **Choosing and changing how a product is tracked** — Products → Barcode shows *Shared barcode* and *Individual barcode*
  side by side; changing a saved product is checked first against its stock, waiting requests and open transfers. (153)
- **Fixes:** quick repeat scans are no longer lost in the barcode field; a refused scan no longer sounds twice, and each
  result has its own sound; a scan error in the barcode field no longer opens a popup that catches the next scan. (153)
- **Find products without a photo** — *Missing product image*, the last option of the Products price filter. (154,
  [module 6](modules/06-products.md))
- **Security:** Excel libraries updated (`calamine`, RUSTSEC-2026-0436; `zip`, RUSTSEC-2026-0429).
- Product User Manual 1.9.

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
