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

## Workflow changes and pending requests (roadmap 49)

**Audit (2026-10-08) — before:** 🟡 partial. Pending requests stored only a level number and read the chain live from
the workflow by position: new approvers and queues updated by themselves, but a step inserted before a completed one
shifted history onto the wrong step, removing the step a request waited at made it decidable by any approver,
nobody was notified and the sync was not audited.

**Now:**

* Every workflow step has a stable `id` (assigned on save; the editor keeps it; a step saved without an id keeps the id
  of an existing step with the same definition, so saving the same workflow again changes nothing). A request keeps its own chain
  (`approvals.steps`, copied when raised) and each decision records `step_id`.
* Saving a workflow (`PUT /settings/workflows/{action}`) reconciles every pending request of that action in the same
  transaction (`approvals::sync_pending` → `workflow::reconcile`):
  * steps already approved stay approved and are never asked again; history is never rewritten;
  * the request waits for the **first step of the new chain not yet approved** — never back to the start, and a new
    step is never approved automatically;
  * new / removed / reordered / re-assigned steps take effect at once; the old approver loses it from their queue and
    the newly responsible approvers are notified (*Approval needed — workflow updated*); open inboxes refresh live;
  * **exceptions are flagged on the request** (`sync_note`, shown in the inbox): a step placed before an approval
    already given (that approval is kept; the later step is skipped when reached), and a chain whose remaining steps were
    all removed (an **administrator step** is added for the final decision instead of the action running unattended);
  * each moved request is audited (`approvals.workflow_sync`: previous/new level, steps and next approvers), and the
    workflow change itself is audited with the **previous and new workflow** and the list of affected requests (who,
    when).
* The decision trail shows each approval at its step's position in the current chain (*Earlier step* when that step was
  removed), so it reads consistently after a change.
* The inbox shows **Next approver** — the people the step designates (administrators can decide any step and are listed
  only for administrator steps or when nobody else is designated).
* A workflow switched off keeps its steps: requests already pending finish under them (switching off stops new
  requests needing approval).
* Upgrade: migration 0016 gives existing steps ids, copies each pending request's chain and links its earlier
  decisions to their steps (tested on a request raised before the change).
