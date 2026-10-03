# 11 · Expenses

**Scope:** §22 · **Route:** `/expenses` · **API:** `/api/expenses…`, `/api/expense-categories…`

## Record an expense
Category · Amount · Branch (Current Branch) · Date · Description · Supplier/payee (optional) · Paid by (payment
methods) · Receipt/attachment (photo or PDF ≤ 5 MB; phones can open the camera; images are compressed) · Recorded by
(automatic).

## List
Period filter (default this month), category filter; approved total and a by-category breakdown; columns date,
category, description/payee, branch, paid by, recorded by, status, amount; open attachments.

## Statuses
Approved (default) · Pending (when the *Expense* workflow applies — optional amount threshold) · Rejected · Void.
Expenses are **voided with a reason**, never deleted; only the recorder or an approver can void.

## Settings
Settings → Expenses: categories (add, rename, deactivate; defaults Rent, Utilities, Salaries & Wages, Transport,
Supplies, Marketing, Repairs, Other), require description, require attachment. Approval rule: Settings → Workflow
engine → Expense — gate by minimum amount, expense category, branch and/or the requester's role, with one or more
approval levels (module 15).

## Permissions
`expenses.view`, `expenses.create`. Approved expenses feed the dashboard (Expenses, Net performance) and the Expenses
report.
