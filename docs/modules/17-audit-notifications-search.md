# 17 · Audit trail, notifications & search

**Scope:** §29, §37, §38 · **Routes:** `/audit`, `/notifications`, search (Ctrl/⌘ K)

## Audit trail (`/audit`, needs `audit.view`)
Every critical action writes one row **in the same transaction** as the change: user, module, action, record, branch,
before and after values (JSON), linked approval and approver, comments, IP address and device (user agent), device location when shared (geofencing, module 16), time.
Covered: sign-ins, PIN changes, products, stock receipts/adjustments, transfers, sales, returns, cancellations, credit
repayments/write-offs, customers, loyalty adjustments/redemptions/referrals/awards, expenses, users, roles, branches,
settings, workflows, approvals. Filters: period, module. Tap a row for the before/after comparison.

## Notifications
In-app, per user, pushed live (Server-Sent Events) with a toast, and listed under the bell (unread badge) /
`/notifications`:

| Event | Who is notified |
|---|---|
| Low stock / out of stock (once per product per day) | `stock.add` at the branch |
| New order | `orders.manage` at the branch |
| Approval waiting / decided | eligible approvers / the requester |
| Transfer in transit / received | receivers at the destination / the creator |
| Overdue credit (once) | `credit.view` at the branch |
| Product deactivated | `products.view` |

Customer-facing messages go through WhatsApp (see [integrations/whatsapp.md](../integrations/whatsapp.md)).

## Universal search
Products (name, nickname, code, product barcode, item barcode — with availability at the Current Branch), customers
(name, nickname, any part of the mobile), orders (`ORD-…`) and receipts (`RCP-…`), Current Branch first. Results
respect permissions.
