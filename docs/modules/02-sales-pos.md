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

## M-Pesa at the till
Two separate flows inside the M-Pesa payment section:
- **Manual M-Pesa** — *M-Pesa number* and *M-Pesa confirmation code* are both **optional**; the sale completes with
  M-Pesa as the method either way. A code, when entered, must look like one (8–12 letters/digits) and must not have
  been used on another sale (*M-Pesa code already used*). Allowed when Settings → Sales → *Allow manual M-Pesa
  confirmation* is on (default); also works offline.
- **Push STK** — always shown; enabled only when the M-Pesa STK integration is configured and active (otherwise
  greyed out with *STK not configured*, never blocking a manual sale). The number is required for the push; the till
  waits for the confirmed result (completing the sale is held while the prompt is pending) and records the M-Pesa
  receipt from the confirmed payment, which can be used once.

## Offline selling (installable app)
The app installs to the home screen and opens without a connection (service worker: app shell and build files only —
never business data or API responses). While offline:
- The till sells from this branch's product list kept on the device (*Offline — stock as of …*).
- Walk-in **cash-style sales** of products that need no scan verification are saved on the device with their time,
  location and `client_ref` (*Saved offline*; the receipt number is given when it syncs).
- Anything needing a live check waits for the connection, with a clear reason: credit, Push STK, new customers,
  points, deposits, supervisor approval, tracked/barcode-cleared items.
- Saved sales sync automatically when the connection returns (and every 30 s): the server records each once
  (`client_ref`), at the **moment it was made** (`offline_at` → sale, stock movement and payment times, so the business
  day is right; `synced_at` marks it), checks trading hours at that moment, and audits the sync. Only sales up to
  72 hours old are accepted.
- A sale the server refuses (e.g. the stock was sold meanwhile) is listed under *need attention* with the reason:
  fix the cause and **Retry**, or **Discard** one that should not be recorded. Nothing is dropped silently.

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
A compact 50 mm digital receipt, issued once and stored (never changes), with updated receipts after returns and
exchanges; PDF, print, image, email (PDF attached), WhatsApp and a secure link — see
[module 26](26-receipts-and-reconciliation.md).

## Returns & cancellations
From the sale page (needs `sales.return` / `sales.cancel`; optionally approval-gated by amount):
- **Return items** — choose quantities, *return to stock* on/off (off for damaged goods), refund method.
- **Cancel sale** — full reversal (only before any return).
Reversed automatically: stock (movement + barcode back in stock), refund payment (credit sales reduce the balance
first), loyalty points of the returned lines (and the referrer's matching share), customer spend/visits, sale status
(`partially_returned`, `returned`, `cancelled`), linked order (→ returned).

## Exchange (Sale → Exchange items)
One screen, one transaction: items **come back** from the sale and others **go out** in their place
(`POST /api/sales/{id}/exchange`, needs `sales.return` and `sales.create`).
- *Coming back* — quantities per line of the original sale. *Taking instead* — search this branch's products; tracked
  items are scanned and verified exactly as at the till.
- The full value of the returned goods (net of any points redemption on the original) moves to the new sale as an
  **exchange** payment (negative on the old sale, positive on the new one — they net to zero). The customer pays only
  the **difference** with the chosen method (M-Pesa code optional as at the till), or is **refunded** the surplus by the
  chosen refund method.
- Stock comes back to and goes out of the original sale's branch; the ledger, loyalty, customer totals and receipts
  follow from the existing return and sale engines. The new receipt notes *Exchange for RCP-… (RTN-…)* and an *Exchange receipt* is issued on the original; audited as one
  exchange. A retried submit returns the same exchange (`client_ref`).
- Credit sales use **Credit Sales → Recall** and a new sale instead. When returns of this value need approval, the
  whole exchange goes for approval and nothing changes until it is approved (roadmap 67).

## Sales history
Filters: period (default today), search (receipt, customer, mobile), payment method, status, branch, staff.
Summary cards: count, value, discounts.

## Permissions
`sales.view`, `sales.create`, `sales.discount`, `sales.discount_override`, `sales.change_branch`, `sales.return`,
`sales.cancel`, `sales.view_financials`, `customers.redeem_points`.

## Settings
Sales → quantity entry, barcode clearance, payment methods (add/rename/disable), manual M-Pesa, credit, receipt
footer. Loyalty rules (module 10).
