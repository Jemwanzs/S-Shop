# 02 · Sales / POS, receipts, returns

**Scope:** §6–8, §10, §35–36 · **Routes:** `/pos`, `/sales`, `/sales/:id` · **API:** `/api/pos/products`, `/api/sales…`

## Record a sale (POS)
1. **Find the product** — search (name, nickname, code, barcode) or **Scan** (camera or handheld scanner). Products with
   stock at the Current Branch come first; out-of-stock ones are greyed. Photos stay hidden until *View photos*.
2. **Item sheet** — quantity (default 1; locked to 1 when configured or for per-item products), marked price
   (read-only), **selling price** with live *Difference from marked price: −20 / +200*, optional **Discount applied**
   (typing it sets the selling price), barcode clearance, stock in other branches (view only).
3. **Cart** — one or many lines: quantity × price, discount, line total, ✓ barcode, 🌼 points per line.
4. **Customer** (optional, required for credit) — type the mobile; existing customers load with points/tier/balance;
   new ones need a first name (+ optional nickname).
5. **Payment** — M-Pesa (Push STK and/or manual code), Cash (change calculator), Credit Sale (due date, optional deposit now), or any custom
   method. Optional **points redemption** reduces the amount payable.
6. **Totals** — subtotal at marked prices, discounts, points redeemed, **Total payable**, 🌼 *+N Loyalty Points*.
7. **Complete sale** → success screen with receipt, WhatsApp share and *New sale*.

Phone: floating cart bar → checkout sheet. Desktop: product grid with a sticky cart/checkout panel. The cart survives
navigation and is kept per branch.

## Pricing model (one source of truth)
`discount = marked − selling`. A negative difference is a premium. Discounts are never applied twice.
- Selling below the marked price needs `sales.discount`.
- Above the product's **maximum discount** needs `sales.discount_override`, or a **supervisor** (another user who may
  approve) enters email + PIN at checkout. With the *Excessive discount* workflow on, a second person is always needed.
  The approval is recorded in the approvals register.

## What completing a sale does (one database transaction)
Validates products (active, sold in this branch), stock (available = on hand − reserved, row-locked), barcodes
(in stock here, not already sold) → creates the sale (`RCP-YYYY-NNNNNN`) and lines → stock movements, items marked
sold → payment (or credit record) → customer totals and tier → loyalty points (+ referrer bonus) → audit entry.
A retried submit with the same `client_ref` returns the original sale instead of selling twice.

## Receipts
Business, branch, receipt no., date, customer, items (qty, price, discount), totals, payment & reference, balance
(credit), points earned, salesperson, footer text. **Print** (browser), **PDF** (80 mm), **Share** (WhatsApp API or
`wa.me` link).

## Returns & cancellations
From the sale page (needs `sales.return` / `sales.cancel`; optionally approval-gated by amount):
- **Return items** — choose quantities, *return to stock* on/off (off for damaged goods), refund method.
- **Cancel sale** — full reversal (only before any return).
Reversed automatically: stock (movement + barcode back in stock), refund payment (credit sales reduce the balance
first), loyalty points of the returned lines (and the referrer's matching share), customer spend/visits, sale status
(`partially_returned`, `returned`, `cancelled`), linked order (→ returned).

## Sales history
Filters: period (default today), search (receipt, customer, mobile), payment method, status, branch, staff.
Summary cards: count, value, discounts.

## Permissions
`sales.view`, `sales.create`, `sales.discount`, `sales.discount_override`, `sales.change_branch`, `sales.return`,
`sales.cancel`, `sales.view_financials`, `customers.redeem_points`.

## Settings
Sales → quantity entry, barcode clearance, payment methods (add/rename/disable), manual M-Pesa, credit, receipt
footer. Loyalty rules (module 10).
