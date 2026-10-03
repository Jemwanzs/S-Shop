# 15 · Approval workflows (maker-checker)

**Scope:** §27 · **Routes:** `/approvals`, Settings → Workflow engine · **API:** `/api/approvals…`, `PUT /api/settings/workflows/{action}`

## Model
**Step 1 — Initiator:** anyone with the action's permission. **Then 1–5 approval levels, in order.** Each level's
approver is the Administrator (default), a role, a specific user, or the branch manager. A request moves to the next
level when the current one approves; the action is executed only when the **last** level approves. A rejection at any
level ends the request.

- A requester can **never** approve their own request, and **nobody approves two levels** of the same request.
- An Administrator may decide at any level.
- Each decision (level, approve/reject, person, time, comment) is kept on the request as a decision trail and in the
  audit log (`approve_level` for intermediate levels).
- When a level approves, the next level's approvers and the requester are notified (“Level 1 of 2 approved by …”).
- Counter discount approval stays single-level (supervisor email + PIN at checkout).

### Conditions (when the rule applies)
Each workflow is off by default. When on, it applies only when **all** configured conditions match; empty means “any”:

| Condition | Applies to |
|---|---|
| Minimum amount | actions with an amount (stock addition, returns, cancellations, credit write-off, expense) |
| Branches | all actions with a branch |
| Requester's role | all actions |
| Expense categories | Expense only |

Example: *Expense, from KES 1,000, category Rent → Branch manager → Administrator*.

Existing single-approver workflows were migrated to a one-level chain (migration `0003_approval_levels.sql`).

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
“Level X of Y” for multi-level requests, the decision trail, and *View*, *Approve* / *Reject* (optional comment) or
*Withdraw*. Only people who may decide the **current** level see Approve / Reject. Approval decisions are written to the audit trail
together with the executed action.
