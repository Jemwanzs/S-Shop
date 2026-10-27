# S'Shop — Smart Retail & Business Management — Product Features & Capabilities Manual

*Version 1.8 · October 2026. Generated from [manual/manual.json](manual/manual.json) by `scripts/manual/build.mjs` — edit the JSON, not this file. The same content is the PDF at https://s-shop.store/SShop_Product_User_Manual.pdf.*

S'Shop runs a retail business end to end, from the till and stock room to the customer's phone and the business's own website. It works on phones, tablets and computers, across branches, online and offline. Each module below describes features available today.

## Contents

**Part 1 — Features by module:** [1. Dashboard & Analytics](#module-1) · [2. Sales & Point of Sale](#module-2) · [3. Digital Receipts](#module-3) · [4. Credit Sales](#module-4) · [5. Orders & Customer Ordering Link](#module-5) · [6. Products & Catalogue](#module-6) · [7. Stock & Inventory](#module-7) · [8. Stock Transfers](#module-8) · [9. Customers](#module-9) · [10. Loyalty & Rewards](#module-10) · [11. Expenses](#module-11) · [12. Reports](#module-12) · [13. Branches & Workspace](#module-13) · [14. Users, Roles & Approvals](#module-14) · [15. Security & Sign-In](#module-15) · [16. Audit Trail, Notifications & Search](#module-16) · [17. Settings & Preferences](#module-17) · [18. Business Website](#module-18) · [19. Holiday & Promotional Campaigns](#module-19) · [20. Business Onboarding & Data Migration](#module-20) · [21. Billing & Subscription](#module-21) · [22. Platform Administration](#module-22)

**Part 2 — Step-by-step guides:** [Everyone](#guide-everyone) · [Cashier / Salesperson](#guide-cashier-salesperson) · [Branch manager / Supervisor](#guide-branch-manager-supervisor) · [Business administrator](#guide-business-administrator) · [Platform owner](#guide-platform-owner)

## Part 1 — Features by module

<a id="module-1"></a>

### 1. Dashboard & Analytics

A clear, visual view of how the business is performing for any period, branch, product or team member.

- Sales, transactions, average sale, units sold and orders, each compared with the previous period
- Gross profit, expenses, net performance and stock value for people allowed to see financial figures
- Filters for period, branch, category, product and staff member
- Best-selling products and top staff ranked with Gold, Silver and Bronze medals
- Personal dashboard and leaderboards so every salesperson can follow their own results
- A quick-action home for roles that do not need analytics

<a id="module-2"></a>

### 2. Sales & Point of Sale

A fast till for the counter: find or scan products, build the cart and take payment in a few taps.

- Search by name, nickname, code or barcode, or scan with the phone camera or a handheld scanner
- Product display your way: compact cards, image cards or a list; top sellers first with Gold, Silver and Bronze stars; products with photos first; stock counts shown or hidden
- Selling price and discounts within each product's limits; an optional customer on every sale, with loyalty points shown before completing
- Cash with change, credit and custom methods; split one sale across several methods, leaving any remainder on credit
- M-Pesa into the business's own Till or Paybill: STK Push to the customer's phone completes the sale once paid, or a manual confirmation code
- Sale Date: today by default; authorised staff can record an earlier date for the whole sale or for one product, never a future date
- Offline selling: walk-in cash sales are saved on the device and synced when the connection returns
- Sales history with ownership, returns, exchanges and cancellations under approval rules
- Barcode-controlled items are cleared only by scanning their sticker; typing barcodes at the till is off by default

<a id="module-3"></a>

### 3. Digital Receipts

One consistent receipt for every channel, issued once and kept unchanged.

- Receipts on screen, as PDF, printed, as an image, by email, on WhatsApp or as a secure link
- Business logo (transparent logos stay transparent in every format), branch, items, each payment, balance and loyalty points
- Returns, exchanges and cancellations produce a linked adjustment receipt; the original is always kept
- Sales recorded with an earlier date show the sale date and when they were recorded
- Receipts available from the sales history at any time
- Clearly business receipts, never labelled as tax invoices

<a id="module-4"></a>

### 4. Credit Sales

Sell on credit with clear balances, due dates and repayment history for every customer.

- Credit sales require an identified customer and a due date
- Optional deposit at the counter in cash, M-Pesa or another method
- Repayments recorded against each credit, with a full payment history
- Credit statement per customer: opening balance, every charge, payment, return and write-off with a running balance, closing balance and what is still to pay, as an A4 PDF
- Statuses for open, partially paid, paid, overdue and written-off credit
- Overdue reminders to the team, plus credit aging and credit sales reports
- Write-offs controlled through approval workflows

<a id="module-5"></a>

### 5. Orders & Customer Ordering Link

Customers order from their phones without an account; the team handles every order from one list.

- A branded, image-rich ordering link for each business, with no customer sign-up
- Optional WhatsApp code to verify customers before they see their details
- Customers see past orders, loyalty points and live order tracking
- Products with hidden prices can be ordered; the prices appear on the order confirmation and tracking
- Order statuses from new to delivered or completed, with live alerts for new orders
- Stock reserved for confirmed orders and deducted only when the order is completed
- Notifications go only to the people responsible for the order's branch

<a id="module-6"></a>

### 6. Products & Catalogue

One master catalogue shared by all branches, with photos, pricing rules and your own fields.

- Automatic or custom product codes, categories and suppliers with quick-add
- Marked price, maximum discount and optional cost price for profit reporting
- The selling price can be left blank (Price on Request) and set later; a product is never sold or ordered at an invented price
- A shared barcode per product, or individual tracking of each item
- Several photos per product and your own fields such as size, colour or brand
- Control over which branches stock a product and whether it appears online
- Low-stock alert levels and loyalty rules per product
- Item barcodes stay concealed (****047) on screens, reports and exports; printing, reprinting, replacing and revealing them are permission-controlled and recorded
- Filter the catalogue by price status (price set or missing) to find products that still need a selling price
- Your filters, search, page and scroll position are kept while you open and edit products — fix missing prices one after another without choosing the filters again

<a id="module-7"></a>

### 7. Stock & Inventory

Accurate stock built from a complete ledger of movements, never a number someone can overwrite.

- Every change recorded as a movement: received, sold, returned, transferred, damaged, adjusted or counted
- Physical, reserved and available-to-sell quantities for each branch
- Stock position for any period, from opening to closing stock
- Receiving stock, stock counts and adjustments with reasons
- Low-stock and out-of-stock alerts to the right people
- Stock valuation for people allowed to see financial figures
- Seven-digit barcodes generated for each individually tracked item, printed on stickers (business name, barcode, number, product) and confirmed attached before stock is received
- Stock balances in Excel: download a sheet prefilled with every product and branch, count and correct it, upload it, review each difference and apply — quantities are the counted balances, never additions

<a id="module-8"></a>

### 8. Stock Transfers

Move stock between branches with a controlled, traceable workflow.

- Workflow: draft, approval, dispatch, in transit and received
- Availability checked when the transfer is created and again on dispatch
- The receiving branch confirms what arrived, including short or damaged items
- Individually tracked items followed unit by unit
- Notifications to receivers and to the person who created the transfer

<a id="module-9"></a>

### 9. Customers

A customer book that grows with every sale, with spending, loyalty and credit in one place.

- Customers identified by mobile number, with first name and optional nickname
- Search by name, nickname or any part of the mobile number
- Total spend, purchase count, last purchase, loyalty tier and amount owed
- Your own customer fields: text, number, date, dropdown, yes/no or email
- Purchase history for each customer
- Loyalty and credit figures shown only to people allowed to see them

<a id="module-10"></a>

### 10. Loyalty & Rewards

Reward repeat customers with points, referrals and tiers, all kept in a complete ledger.

- Points earned on the amount actually paid (net of discounts by default), with product-specific rules
- Points redeemed at the till within the business's minimum, redemption window and maximum share of the sale (never more than 50%), with optional supervisor approval
- Referral rewards for customers who bring new customers
- Gold, Silver and Bronze tiers and awards for top customers
- Point balances shown on receipts and on the ordering link
- Manual adjustments controlled and recorded in the audit trail

<a id="module-11"></a>

### 11. Expenses

Record and control spending for each branch, with receipts attached.

- Category, amount, date, payee, payment method and description
- Receipt photos or PDFs attached from the phone camera or files
- Approval workflows for expenses above chosen amounts or in chosen categories
- Totals and a breakdown by category for any period
- Expenses included in net performance and profitability reports

<a id="module-12"></a>

### 12. Reports

Twenty-four standard reports covering sales, payments, stock, customers, loyalty, credit, finance and staff.

- Sales by product, branch and user; order, stock, customer, loyalty and credit reports
- Payments collected by method, including split payments; sales recorded with an earlier date counted on their sale date, with the recorded time alongside
- Stock position, movement, valuation, transfers, adjustments and barcode inventory
- Expenses, profitability and user performance
- Consistent filters for period, branch, category, product and user
- Export to branded A4 PDF or formatted Excel
- Reports shown only to people allowed to see them

<a id="module-13"></a>

### 13. Branches & Workspace

Run several branches as one business, each with its own stock, sales and hours.

- Stock, sales, orders, expenses and credit kept per branch; products and customers shared
- A branch manager for each branch, used in approval workflows
- Business or branch-specific trading hours, with sales outside hours allowed or blocked
- Optional location rules so staff work only from the branch area
- A clear Current Branch switcher for people who work at several branches
- Your business has its own workspace address — s-shop.store/your-business — shown with Copy link in Settings → Business profile; bookmark it or share it with staff (they still sign in, and see only what their role allows)

<a id="module-14"></a>

### 14. Users, Roles & Approvals

Give every person exactly the access they need, with maker-checker approval for sensitive actions.

- Roles with detailed permissions, plus exceptions for individual users
- Access to all branches or selected branches, and to own or all records
- Approval workflows with one to five levels, by administrator, role, user or branch manager
- Requesters never approve their own requests, and nobody approves two levels
- Approval conditions by amount, branch, requester role or expense category
- Sale ownership changes handled through approvals
- New users are invited by email and set their own PIN; an invitation link works once and expires after 10 minutes
- Several approvers per level — the first decision counts and the others are told; Tenant Administrators may approve their own requests (switchable), never refunds, expenses or write-offs

<a id="module-15"></a>

### 15. Security & Sign-In

Secure sign-in and session controls that protect the business on shared and personal devices.

- Email and PIN sign-in, with temporary lockout after repeated wrong attempts
- Optional Quick Login PIN on trusted devices; sensitive actions still need the full sign-in
- Optional Face ID & biometric login (passkeys) on your own phone or computer; the device checks your face or fingerprint and S'Shop never receives biometric data
- The branch is chosen at every sign-in (only branch, default branch or the picker); on a shared phone nobody sees the previous person's cart
- Automatic logout after inactivity, with a warning first and stricter timeouts for sensitive roles
- Self-service PIN reset by email; new users set their own PIN from their invitation
- Sign out everywhere and trusted-device management
- Control over whether S'Shop support may enter the business, always time-limited and recorded
- S'Shop tells you when a new version is ready and refreshes without losing unsaved work

<a id="module-16"></a>

### 16. Audit Trail, Notifications & Search

Know who did what and when, receive the alerts that matter, and find anything quickly.

- Every critical action recorded with user, branch, time, device and before/after values
- Audit filters by period and module, with a side-by-side comparison of changes
- Notifications for low stock, new orders, approvals, transfers and overdue credit
- Personal notification preferences
- Universal search across products, customers, receipts and orders (Ctrl / ⌘ K)

<a id="module-17"></a>

### 17. Settings & Preferences

Configure the business your way, with every change recorded.

- Business profile, logo, contact details, currency and time zone
- Branding & white labelling: your logo on sign-in, the app, dashboards, POS, receipts, customer pages and emails, each switchable; “Powered by S'Shop” stays
- Product, customer, sales, loyalty and payment settings
- Workflow engine and workspace hours
- Closing the books: lock finished periods so their sales, stock and expenses can no longer change
- Personal preferences: display currency, language, theme and font
- The page each person lands on after signing in; dialogs never close by accident and ask before discarding changes
- English, Kiswahili, French and Arabic, with right-to-left support
- Every settings change saved with before and after values
- Barcode hardware: label sizes, label printers (system printing, ZPL, TSPL), a test label and a scanner test
- On phones the bottom bar gives Home, Sales, Stock and Products in one tap; Orders and everything else are under More

<a id="module-18"></a>

### 18. Business Website

An optional website for the business, powered directly by its S'Shop products, stock and orders.

- Brand colours, pages, menus and a premium product storefront without writing code
- Products, prices, photos and stock come from S'Shop, so there is nothing to sync
- Show or hide prices for the whole site or per product; customers can still order hidden-price products and see the prices once the order is submitted
- Website orders arrive in the Orders list with alerts
- Draft, preview and publish, with a media library and image-quality checks
- Custom domains with guided DNS checks and secure HTTPS
- Visitor and order analytics

<a id="module-19"></a>

### 19. Holiday & Promotional Campaigns

Seasonal greetings, promotional banners and featured products on the business website.

- Fifteen occasions with ready-written greetings and designed templates
- Featured products from real best sellers or a manual selection
- Hero, compact, top strip or floating card on chosen pages
- Scheduling in the business's time zone, with draft, live and expired states
- Preview on mobile, tablet and desktop before publishing
- Campaign insights: views, clicks, orders and sales

<a id="module-20"></a>

### 20. Business Onboarding & Data Migration

Bring an existing business into S'Shop from Excel, check every figure, then trade in S'Shop.

- Excel templates for products, customers, opening stock, expenses, sales history, credit balances, loyalty points, orders and transfers
- Selling Price is optional in the product import: blank stays not set, never 0
- Column mapping, a colour-coded preview and an error report of only the rows to fix
- Imports run in the background with live progress and a clear result
- A migration cutover date that keeps history and S'Shop's own records apart
- History imported with its original dates and receipt numbers, never moving stock, awarding points or sending messages
- Outstanding orders hold stock and in-transit transfers are received in S'Shop
- Optional approval by a second administrator; every import posts completely or not at all
- Migration reconciliation report (also in Excel); onboarding completes only when every figure matches

<a id="module-21"></a>

### 21. Billing & Subscription

Each business sees its own plan, invoices and payments in one place.

- Plan, amount, billing frequency, status and next due date
- Invoices with online payment, quotations to accept, and receipts
- A4 PDF invoices, quotations and receipts
- Billing history and outstanding amounts
- Modules available according to the business's package

<a id="module-22"></a>

### 22. Platform Administration *(platform owner only)*

For the S'Shop platform owner only: manage tenants and businesses centrally and securely.

- Tenant and business directory with status, activity and billing
- Review access requests and onboard new businesses with welcome emails
- Activate, suspend or reactivate tenants, with reasons recorded
- Billing models, tenant-specific pricing, trials and the platform billing dashboard
- Platform-wide limits for sign-in, sessions, websites and campaigns
- Support access to a business only through audited, time-limited sessions
- Error alerts: server faults, failing outside services, failed background jobs and restarts reach the owner in the app and by email, grouped, with a test alert

## Part 2 — Step-by-step guides

Step-by-step instructions for the everyday work of each role. What you see depends on your role and branches: if a button or menu named here is missing, your administrator has not given you that permission. Sensitive actions may wait for approval before they take effect (Approvals).

<a id="guide-everyone"></a>

### Guide: Everyone

Signing in, choosing your branch and keeping your account secure.

#### Sign in

1. Open your business's workspace address (for example s-shop.store/your-business) or s-shop.store.
2. Enter your Email and PIN, then tap Sign in.
3. If you work at several branches, choose the branch under "Where are you working today?".
4. You land on your chosen first page. Bookmark the address — it always leads to your business.

#### Set up a Quick PIN on your own device

1. Open your name / avatar menu and choose Preferences.
2. Under Quick Login PIN & Face ID, tap Create Quick PIN.
3. Enter your current full PIN, choose a Quick PIN, repeat it and give the device a name (e.g. My phone), then Save.
4. Next time on this device, sign in with the keypad. Use password instead returns to the full sign-in.

> Only on a device you alone use. Your administrator decides whether Quick PIN is available.

#### Turn on Face ID or fingerprint sign-in

1. Open Preferences and find Face ID & Biometric Login.
2. Tap Set up this device, enter your current PIN and tap Continue.
3. Confirm with your face or fingerprint when the device asks.
4. Next time, tap Sign in with Face ID / Biometrics on the sign-in page.

#### Change your PIN, forgotten PIN, sign out

1. Open your name / avatar menu.
2. Change PIN: enter your current PIN, the new PIN twice, then Save PIN.
3. Forgot your PIN? On the sign-in page tap Forgot PIN / Password? and follow the email link, or ask your administrator to Reset PIN (Settings → Users).
4. Sign out ends the session on this device. Sessions also end by themselves after a period without activity.

<a id="guide-cashier-salesperson"></a>

### Guide: Cashier / Salesperson

Recording sales, payments, receipts, returns, credit and customer orders.

#### Record a sale

1. Open New Sale (phone: the Sales tab or the floating Record sale button).
2. Find a product by name, nickname, code or barcode, or tap Scan to use the camera or a handheld scanner.
3. Tap the product, set the Quantity (and the Selling price if you give a discount), then Add to cart.
4. Optional: enter the Customer phone number so the customer earns loyalty points (first name for a new customer).
5. Choose the payment: Cash, M-Pesa, Credit Sale or Split payment.
6. Cash: enter Cash received to see the change. Tap Complete sale.

> A discount above the product's maximum needs a supervisor's email and PIN at the counter.

#### Take an M-Pesa payment

1. Choose M-Pesa as the payment.
2. STK Push: enter the customer's M-Pesa number and tap Send STK Push. The customer enters their M-Pesa PIN on their phone; the sale completes by itself when Safaricom confirms.
3. If the prompt fails or times out, tap Retry, or use Manual M-PESA: enter the M-Pesa confirmation code from the customer's message and tap Complete sale.

#### Send or print the receipt

1. After Sale complete, choose how to send the receipt: WhatsApp, Email, Print, or copy the secure Link.
2. Tap New sale to start the next customer.

#### Sell on credit

1. Add the items and the customer (credit needs a customer).
2. Choose Credit Sale, set the Due date and, if the customer pays part now, the deposit (Deposit now).
3. Tap Complete sale. The balance appears under Credit Sales.

#### Record a credit repayment

1. Open Credit Sales and the customer's credit sale.
2. Tap Record payment, enter the Amount received and the method (M-Pesa needs the confirmation code).
3. Tap Save payment. The outstanding balance updates at once.

#### Return items or exchange them

1. Open Sales and the sale.
2. Return: tap Return items / refund, choose what is coming back and how many, keep Return to stock on (off for damaged goods), choose Refund via, give the Reason, then Process return.
3. Exchange: tap Exchange items, choose what is coming back, add what the customer takes instead, then Complete exchange — only the difference is paid or refunded.

#### Handle a customer order

1. Open Orders (phone: More → Orders). New orders show a counter.
2. Open the order and, under Next step, tap the next status as you prepare, dispatch and deliver it.
3. For individually tracked items, scan each item handed over when asked.
4. At the configured stage the order becomes a sale automatically. Phone orders: New phone order.

<a id="guide-branch-manager-supervisor"></a>

### Guide: Branch manager / Supervisor

Stock, transfers, approvals, expenses, customers and reports for your branches.

#### Receive a delivery

1. Open Stock and tap Receive stock.
2. Find or scan the product and enter the quantity received.
3. Individually tracked products: tap Generate barcodes, Print labels, stick one label on each item, tick I have attached a label to each item, then Verify labels by scanning them.
4. Enter the Cost / purchase price, Marked selling price, Supplier and the delivery note in Reference / notes; change Date received if needed.
5. Tap Receive. The stock and its value update immediately (or after approval when required).

#### Count stock (stock take)

1. Open Stock and tap Stock take.
2. Enter the Reason for the count, then scan or type what is on the shelf for each product.
3. Check Expected, Counted and Variance, then Submit count. Variances wait for approval where required.

#### Correct stock for damage, loss or returns

1. Open Stock and tap Adjust.
2. Find the product, choose the Type (Damaged, Lost / missing, Write-off, Customer return, Returned to supplier or Recount), the quantity and the Reason.
3. Tap Save adjustment. Nothing is overwritten — every adjustment is kept with who, when and why.

#### Update stock balances from Excel

1. Open Stock and tap Stock balances, then Download template — it lists every product and branch with today's balance.
2. Count, and type the counted balance in the sheet (not the difference).
3. Upload stock balances and review Current, Uploaded and Difference for every row.
4. Tap Import to apply. Approval applies when stock adjustments need it.

#### Move stock to another branch

1. Open Transfers and tap New transfer. Add products and quantities (or scan them).
2. Choose the To branch, the Transfer date and a reason, then Submit for approval (or Save draft).
3. After approval the sending branch taps Dispatch; the receiving branch taps Confirm receipt, or Receive with discrepancies to record anything short or damaged.

#### Approve or reject requests

1. Open Approvals. Waiting lists what needs a decision; each card shows who it is Assigned to.
2. Tap View for the details (requester, level, history), then Approve or Reject (add a Comment).
3. If you cannot decide it, the card says why — for example you made the request, or it is assigned to another approver.

> If a card says Nobody can approve this request yet, an administrator chooses another approver for that step in Settings → Workflow engine (the request follows the change), or the requester withdraws it.

#### Record an expense

1. Open Expenses and tap Record expense.
2. Choose the Category, enter the Amount and Description, the Supplier / payee, and attach the receipt photo.
3. Tap Save expense. Large or sensitive expenses wait for approval.

#### Look after customers and loyalty

1. Customers: search by name, nickname or mobile; open a customer for purchases, points, credit and Statement.
2. Redeem points: on the customer, tap Redeem points and enter the points and what was given.
3. Award periods: in Loyalty & Rewards → Awards, Open award period, and Close & award at the end to recognise Gold, Silver and Bronze customers.

#### Run a report

1. Open Reports and choose a report.
2. Set the period and filters (branch, category, product, user).
3. Download PDF or Excel.

<a id="guide-business-administrator"></a>

### Guide: Business administrator

Setting up the business, people, rules, payments and the website.

#### Business profile and workspace address

1. Settings → Business profile: name, logo, tagline, phone, email, address, currency and time zone; Save changes.
2. S'Shop Workspace URL: tap Copy link to share your team's address. Staff still sign in and see only what their role allows.
3. The business address (in the workspace URL, ordering link and website) is permanent; Change it only when necessary — old links keep working.

#### Add a user

1. Settings → Users → Add user.
2. Enter the Full name, Email address, Mobile number, Role and branches (or All branches).
3. Tap Create & invite, then send the invitation by email or Share via WhatsApp, or Copy activation link. The link is valid for 10 minutes; Send email again if it expires.
4. Later: Reset PIN, Reset Quick PIN or Disable user from the user's row.

#### Roles, permissions and branches

1. Settings → Roles & permissions: adjust a role or create a New role; tick what it may see and do.
2. Settings → Branches → Add branch: name, code, location, phone and Branch manager; optionally its own trading hours and geofencing (only at the branch).

#### Approval rules

1. Settings → Workflow engine: choose an action (e.g. product deactivation, stock adjustment, expense).
2. Turn on Require approval. In each level, Add approver… for every role or person who may decide it (Administrator, Branch manager, a role, a person) — any one of them decides the level; the first decision counts.
3. Add level for a second or third approval in order; optionally limit when it applies (amount, branches, requester's role, categories), then Save.
4. Allow Tenant Administrator self-approval (top of the page): on, you can approve your own requests by pressing Approve; switch it off for strict separation of duties. Refunds, cancellations, credit write-offs, expenses and discounts always need a second person.

#### Connect M-Pesa (STK Push)

1. Settings → M-Pesa & WhatsApp → Add account.
2. Choose the Type (Buy Goods Till or Paybill), enter the numbers, the Environment and the Daraja Consumer key, Consumer secret and Passkey (encrypted when saved), then Save.
3. Tap Test OAuth connection, then Test STK Push with a small amount to your own phone.
4. When it shows Verified, switch STK Push on — cashiers can now send payment prompts.

#### Security and sign-in rules

1. Settings → Security: allow or switch off Quick PIN and Face ID for chosen roles, set the automatic logout time, and decide whether S'Shop support needs your approval before opening your business.
2. Settings → Closing the books: set Books closed up to so finished periods can no longer change.

#### Branding

1. Settings → Branding & White Labelling → Enable business branding.
2. Upload logo (PNG, SVG, WebP or JPEG, up to 2 MB) and choose where it shows: sign-in, sidebar, POS, receipts, customer pages, website, app icon.
3. Preview branding, then Save changes.

#### Import your existing data

1. Settings → Onboarding: follow the Recommended order (branches, products, customers, history, credit, loyalty, stock).
2. For each dataset, download the Template, fill it in Excel and Upload Excel.
3. Fix any invalid rows shown, then Import the rows (approval may apply).
4. Check Migration reconciliation, then Complete onboarding.

#### Website

1. Settings → Website → Request Website Service (S'Shop sets it up and contacts you).
2. Once active: edit Content, Design, Products and the other sections, Save draft and Preview.
3. Tap Publish, say What changed?, then Publish now — visitors see the new version straight away.

#### Billing

1. Settings → Billing shows your plan, what is due and your invoices.
2. Pay now (through Paystack) or Pay by bank using the invoice number as the reference; download receipts there.

<a id="guide-platform-owner"></a>

### Guide: Platform owner *(platform owner only)*

Approving new businesses, supporting tenants and platform billing.

#### Approve a new business

1. Settings → Access requests → Pending.
2. Open a request and tap Approve & create — the business, its administrator and a one-time PIN are created.
3. The welcome email is sent automatically; Resend Welcome Email or Copy Message (WhatsApp) if needed. Reject sends a courteous email with your reason.

#### Support a tenant

1. Settings → Tenants → open the tenant.
2. Next to the business, tap Open business and choose Platform support access.
3. Give the Reason, choose View only or Full access, enter Your PIN and tap Start support access. If the tenant requires approval, their administrator approves first; then Start approved support session.
4. A banner shows the business, access level and time left while you work. Tap End session when done. Everything is audited.

#### Billing, status and activity

1. Settings → Platform billing: plans, invoices and payments across tenants.
2. Settings → Tenants → a tenant → Access & status: Deactivate or Reactivate (platform-owned tenants are never deactivated).
3. Settings → Activity and Security & features: platform-wide activity and sign-in rules (businesses can only make them stricter).

#### Errors and alerts

1. When an alert arrives (bell or email), open Settings → Errors & alerts.
2. Each entry shows what failed, where, how often and when; repeats are counted rather than sent again.
3. Fix the cause (e.g. free a Railway domain slot, renew a credential), then tap Resolve. If it happens again you are alerted again.
4. Send test alert checks that the bell and your email both receive alerts.
