# 34 — Loyalty rewards, split payments, credit and the configurable POS (roadmap 106–110)

Brief: *S'Shop — Loyalty Rewards, Split Payments, Credit Sales & Configurable POS Interface* (James Sammy, SyncScore).
Everything reuses the existing engines — one sale engine (`routes/sales.rs::record_sale`), one stock ledger
(`inventory`), one loyalty ledger (`loyalty.rs`), one payment table (`payments`), one credit module (`credit_sales`) —
no parallel engines.

| Phase | Roadmap | Status |
|---|---|---|
| Loyalty earning base & redemption controls | 106 | ✅ live |
| Split payments, partial payment → credit, receipts & reporting | 107 | ⏳ planned |
| Landing page after sign-in | 108 | ⏳ planned |
| POS display: styles, images, columns, prices; product list images & prices | 109 | ⏳ planned |
| Top-selling products with Gold / Silver / Bronze ranking | 110 | ⏳ planned |

## 106 — Loyalty earning & redemption

**Settings → Loyalty & Rewards.**

*Points configuration*
- **Earning base**: **Net payable (default)** — after discounts — or *Gross selling amount* (before discounts, only when
  explicitly chosen). Products already carry *Earns points* and their own rate (eligible products / categories).
- **Points on value paid with points**: *excluded by default* (explicit setting) — the part of a sale settled with
  redeemed points never earns points again.
- **Rounding**: *down* (default) or *nearest*. Points are worked out on the sale's eligible amount per earning rate
  (not rounded line by line), e.g. 100,000 − 5,000 discount = 95,000 at 1 point per 1,000 → **95 points**.
- Points are written only when the sale is recorded; returns reverse exactly what each line earned; a sale cannot
  award twice (one award per sale in the ledger). The receipt shows points earned.

*Redemption settings*
- **Enable points redemption** (on/off), **minimum points required**, **redemption rate** (money per point).
- **Redemption window**: open / closed by hand, with optional **scheduled opening and closing** date-times (the
  business's time zone). Closed: points are visible, not redeemable.
- **Maximum redemption per sale**: a percentage of the net sale value, default and **hard maximum 50 %** — enforced
  by the server on every sale, so no screen, API call or adjustment can exceed it.
- **Redemption needs a supervisor** (optional): the existing counter approval (supervisor email + PIN).
- Only people with loyalty settings rights change these; every change is audited.
- Where: `settings.rs` (`LoyaltySettings::redemption_open`, `redemption_percent`, `MAX_REDEMPTION_PERCENT`),
  `loyalty.rs::sale_points` (mirrored by `web/src/pages/sales/pos/cart.ts::salePoints`), `routes/sales.rs::record_sale`
  (window, cap, approval, points), `loyalty::redeem` (window for redemptions outside a sale), `web/src/lib/loyalty.ts`.
- A cashier holding *Manage loyalty* needs no second person; a supervisor approving a redemption cannot thereby
  approve an excessive discount.
- Tests: unit (`sale_points`, window, cap) and the smoke section *Roadmap 106*.

## 107 — Split payments & partial payment → credit (planned)

- Several payment lines on one sale (cash, M-PESA incl. STK Push, card, bank, other enabled methods) plus points
  (within the 106 cap) — *Total payable · Allocated · Remaining*. STK lines count only once Safaricom confirms.
- A remainder can be left on **credit** (customer required, due date, credit permissions / approvals): one credit
  record linked to the sale and receipt, the paid parts as its payment history, aging and reports as today.
- Stock leaves **once** when the goods are handed over; repayments never touch stock. Goods not handed over use
  Orders. Returns reconcile stock, payments, points and the credit balance together.
- Receipt and Sales History show gross, discounts, payable, each payment line, points redeemed (and value), credit
  outstanding, points earned and the payment status (*Paid · Partially paid · Credit outstanding · Refunded*, from
  the existing sale / credit statuses). Reports separate gross / net sales, money collected, loyalty value redeemed,
  receivables and repayments (repayments are not revenue).
- Each payment line is its own `payments` row; duplicate submission is refused (client reference); offline POS keeps
  its restrictions (no points, STK or credit offline).

## 108 — Landing page after sign-in (planned)

**Settings → Business → User experience**: *Default page after sign-in* — Dashboard (default), My Dashboard, Record
Sale, Sales History, other permitted pages; a business default and an optional personal choice. Sign-in → pick the
branch (when several) → that page; not permitted → the normal default.

## 109 — POS display & product list (planned)

- **Settings → Sales → POS display**: display style *Compact cards (existing)* · *Image cards* · *List rows*; show
  product images; tablet columns (default 2) and desktop columns (default 4) within what the screen and cart allow;
  phones stay automatic; show prices in Record Sale (the cart, totals and payment always show amounts).
- **Settings → Products & inventory → Display**: primary image thumbnail in the product list; show prices in the
  product list. Independent of the public website's price settings. Rounded square thumbnails, no stretching, a clean
  fallback.

## 110 — Top-selling products (planned)

**Settings → Sales → Product performance**: *Show top-selling products first* (default on); metric *Quantity sold*
(default) · *Sales value* (net of discounts and returns) · *Number of completed sales*; period *Today* · *This week*
(default, Monday start) · *This month*, in the business time zone. Completed sales only (imported history counts when
dated in the period). Rank 1 ★★★★★ gold, rank 2 ★★★ silver, rank 3 ★★ bronze — performance, not reviews — with a
tap / hover explanation. Ordering: search results first; categories limit the list; ranking orders it when on;
otherwise the existing order; favourites unchanged; saved manual order never rewritten.
