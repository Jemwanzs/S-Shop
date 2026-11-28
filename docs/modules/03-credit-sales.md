# 03 · Credit sales

**Scope:** §8 Credit Sale, §9 · **Routes:** `/credit`, `/credit/:id` · **API:** `/api/credit…`

## How credit is created
At checkout choose **Credit Sale** → a customer is required (found by mobile or created with first name) and a due
date (default *today + credit days*, Settings → Sales). Completing the sale creates the sale **and** a credit record.

### Deposit at the counter
Optionally enter **Deposit now** and choose how it was paid: Cash (with change), M-Pesa (Push STK for the deposit
amount, or the confirmation code), or another enabled method. The deposit must be less than the total. The credit then
opens as **Partially paid** at the remaining balance. The deposit is one payment that appears on the receipt (amount
paid + balance) and as the first entry in the credit's payment history. The WhatsApp receipt shows *Deposit paid* and
*On credit — balance*. API: `deposit: { amount, method, reference?, mpesa_request_id? }` on `POST /api/sales` with
`payment.method = "credit"`.

## Credit list
Tabs: **Open** (default) · Overdue · Paid · Written off · All; search by customer, mobile or receipt.
Header cards: total outstanding and overdue; **aging** bars (not due · 1–30 · 31–60 · 61–90 · 90+ days).
Columns: customer, receipt, branch, salesperson, amount, paid, **balance**, due date, days outstanding, status.

## Credit detail
**Product sold** (roadmap 178), below the outstanding balance: each item as recorded on the sale — photo, name and code at the time of sale, barcode (item barcodes masked), quantity, marked price, discount and the amount on credit; the name opens the product (with product permission); renamed or deactivated products are noted. Sale lines keep the name and code they were sold under (`sale_items.product_name` / `product_code`).

Balance with progress bar, payment history (method, reference, user, time), audit trail, links to the sale and
customer. Actions:
- **Record payment** — partial or full; Cash, M-Pesa, other methods. Roadmap 201: the **M-Pesa confirmation code is
  optional** (*M-Pesa confirmation code (Optional)*) for a payment recorded by hand; when given it must look like a code
  (8–12 letters and digits) and is refused if already recorded on any sale or repayment (also enforced by the database).
  A confirmed STK request is still checked as before (status, amount, not used twice). The payment history marks each
  M-Pesa payment *Verified by M-Pesa* (STK) or *Recorded manually* (with or without a code); the audit records
  `verification` `stk_verified` / `manual`. After saving — or from any payment in the history — *Send on WhatsApp* sends
  the customer a payment confirmation (amount, method and code, balance remaining, the sale's secure receipt link) through
  the same WhatsApp dialog as receipts.
  Over-payment is refused. Status becomes *Partially paid* → *Paid*.
- **Remind** — WhatsApp reminder (API or `wa.me`).
- **Write off** — reason required; optionally approval-gated (*Credit write-off* workflow, threshold by amount).

- **Recall sale** (`credit.recall`) — bring the goods back: all of the sale or chosen items/quantities, with a
  mandatory reason. The dialog shows customer, products, quantities, barcodes, prices, outstanding balance and the
  **original branch** — stock always returns there (move it afterwards with a transfer if it is physically elsewhere).
  Tracked units must be **scanned again** and be the exact unit sold on this sale (not already returned). Runs on the
  same engine as returns: the exact unit back *in stock*, a `customer_return` movement (“Credit sale recall …”),
  loyalty points reversed, the balance reduced. If the customer had paid more than the revised amount, the user
  chooses **refund now** (a refund payment) or **keep as customer credit** (recorded on the recall for follow-up);
  payment history is never changed. Full recall with nothing owed → status **Recalled**; otherwise the credit shows
  **Part recalled** and collection continues. Audited (balance before/after, items, barcodes, branch); approval via
  the *Credit sale recall* workflow when configured.

## One item per credit sale, returned-item credit and swaps (roadmap 179)
- **One item per credit sale** (Settings → Sales, default on): a new credit sale holds one item; the till records a cart
  of several items as separate credit sales (same customer and due date). Earlier credit sales are unchanged.
- **Record payment → Returned item**: an item of a sale the customer **paid for** comes back and its paid value settles
  this credit (or a return credit already held is used). Preview: item value, the part that only cancels what was still
  owed on its sale, the eligible paid value, the most that can be applied, the new balance. Partial amounts; what is left
  stays available as a return credit or is paid out. An item from an unpaid credit sale gives no credit. Follows *Sale
  return / refund* approval. Recorded as a `return_credit` payment, linked to the return and the original sale.
- **Swap item** (*Recall* + *Record sales*): the item comes back, what is owed on it is cancelled, any money paid beyond
  that carries over; the replacement is a new linked sale (*Replacement for …*) on credit or paid now. Follows *Credit
  sale recall* approval.

## Statuses
Outstanding · Partially paid · Paid · **Overdue** (open and past due — derived, not stored) · Written off · Cancelled
(sale cancelled) · Recalled (goods all back, nothing owed); *Part recalled* is shown alongside. Returns on a credit sale reduce the balance first (`adjustments`).

## Automation
Every 15 minutes overdue credits notify users with `credit.view` at that branch once; with *Overdue credit reminders*
on, the customer also gets one WhatsApp reminder.

## Permissions
`credit.view`, `credit.collect`, `credit.write_off`, `credit.recall`; balances in customer screens need `customers.view_credit`.

## Reports
Credit Sales Report, Credit Aging Report (module 12).

## A credit record per item owing (roadmap 196)

A sale of several units left (partly) on credit — split payment with *Leave the rest on credit*, or a credit sale with or
without a deposit — is one sale and one receipt. The money received pays the cheapest units first (selling price after
discounts and the unit's share of points redeemed; equal prices in cart order), and every unit still owing gets its own
credit record (`credit_sales.sale_item_id`, `unit_no`, `item_price`, `paid_at_sale`): same sale, receipt, customer,
branch, salesperson and due date. Each is repaid, recalled (that unit only) or written off on its own. Returns and
cancellations close the returned units' credits and refund only money received. A one-unit sale keeps one credit for the
whole sale; *One item per credit sale* off keeps one credit per sale. The till previews the allocation (**Payment
Allocation Summary**).
