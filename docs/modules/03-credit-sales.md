# 03 · Credit sales

**Scope:** §8 Credit Sale, §9 · **Routes:** `/credit`, `/credit/:id` · **API:** `/api/credit…`

## How credit is created
At checkout choose **Credit Sale** → a customer is required (found by mobile or created with first name) and a due
date (default *today + credit days*, Settings → Sales). Completing the sale creates the sale **and** a credit record.

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

## Statuses
Outstanding · Partially paid · Paid · **Overdue** (open and past due — derived, not stored) · Written off · Cancelled
(sale cancelled). Returns on a credit sale reduce the balance first (`adjustments`).

## Automation
Every 15 minutes overdue credits notify users with `credit.view` at that branch once; with *Overdue credit reminders*
on, the customer also gets one WhatsApp reminder.

## Permissions
`credit.view`, `credit.collect`, `credit.write_off`; balances in customer screens need `customers.view_credit`.

## Reports
Credit Sales Report, Credit Aging Report (module 12).
