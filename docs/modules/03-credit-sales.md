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
Balance with progress bar, payment history (method, reference, user, time), audit trail, links to the sale and
customer. Actions:
- **Record payment** — partial or full; Cash, M-Pesa (code required, or a confirmed STK request), other methods.
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
