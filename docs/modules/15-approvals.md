# 15 · Approval workflows (maker-checker)

**Scope:** §27 · **Routes:** `/approvals`, Settings → Workflow engine · **API:** `/api/approvals…`, `PUT /api/settings/workflows/{action}`

## Two-stage model
**Step 1 — Initiator:** anyone with the action's permission. **Step 2 — Approver:** Administrator (default), a role,
a specific user, or the branch manager. A requester can **never** approve their own request. Each workflow is off by
default and can have a minimum amount.

| Action | When gated, the request… | Executed on approval |
|---|---|---|
| Product creation | creates the product inactive | product activated |
| Product edit | is stored, product unchanged | changes applied |
| Product deactivation | is stored | product deactivated |
| Stock addition (by value) | is stored, no stock moves | receipt applied (barcodes re-checked) |
| Stock adjustment / count | adjustment *pending* | applied (recount re-derives variance) |
| Stock write-off | adjustment *pending* | applied |
| Stock transfer | transfer *pending approval* | transfer *approved* |
| Excessive discount | supervisor email + PIN at the counter | recorded as approved instantly |
| Sale return (by amount) | is stored | return processed |
| Sale cancellation (by amount) | is stored | sale cancelled |
| Credit write-off (by amount) | is stored | written off |
| Expense (by amount) | expense *pending* | expense *approved* |

The requester sees “Sent for approval”; approvers are notified (bell + live toast). Rejecting restores parked records
(adjustment rejected, transfer rejected, expense rejected). Requesters can withdraw pending requests.

## Approvals page
Tabs: Waiting · My requests · Approved · Rejected. Cards show the action, summary, amount, requester, branch and age,
with *View*, *Approve* / *Reject* (optional comment) or *Withdraw*. Approval decisions are written to the audit trail
together with the executed action.
