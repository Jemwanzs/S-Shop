# 15 · Approval workflows (maker-checker)

**Scope:** §27 · **Routes:** `/approvals`, Settings → Workflow engine · **API:** `/api/approvals…`, `PUT /api/settings/workflows/{action}`

## Model
**Step 1 — Initiator:** anyone with the action's permission. **Then 1–5 approval levels, in order.** Each level's
approver is the Administrator (default), a role, a specific user, or the branch manager. A request moves to the next
level when the current one approves; the action is executed only when the **last** level approves. A rejection at any
level ends the request.

- **Several approvers per level (roadmap 150).** A level names one or more approvers — roles, specific people, the
  Administrator, the branch manager (up to 10). **Any one of them decides the level; the first approval or rejection
  finalises it.** The others can no longer act on it (*Already handled — approved by … at …*), are told who decided,
  when and what happens next (*… approved — no action needed*; *it now waits for level 2 of 3*), and keep read-only
  sight of it in the history. Add level / remove level keep sequential multi-level approval.
- **First decision wins, on the server.** The screen sends the level the person was looking at; the request row is
  locked for the decision, so of two simultaneous clicks one decides and the other is refused. A decision for a level
  that has moved on is refused — it is never applied to the next level. The business action (deactivation, stock,
  transfer, refund…) runs once, in the same transaction as the final approval.
- A requester can **never** approve their own request — **except a Tenant Administrator under the self-approval
  policy** (below) — and **nobody approves two levels** of the same request.
- An Administrator may decide at any level. *Administrator* means a person whose **effective** permissions include full
  access (`*`); a *role* step means people with that role; *branch manager* means the request branch's manager (or,
  for people at all branches, anyone holding *Approve requests*). The engine reads **effective** permissions (the role's
  plus the person's access exceptions) — the same source as every other check (roadmap 148). Access exceptions cannot
  grant approval rights, so approval rights come from the role.
- **Eligibility is decided per person, per request, every time**, from the request's own chain of steps, its current
  step, the person's identity, business, permissions and branch, the requester and the request's decisions so far.
  Having approved another request of the same kind never makes someone eligible for this one.
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

### Bulk decisions (roadmap 151)

Approvals → Waiting: every request you may decide has a checkbox (and *Select every request you can decide*). The bar
shows *X requests selected · Approve selected · Reject selected · Clear*. The review lists each request (type, record,
level) with one **comment saved on every request**. The server then decides each request **on its own**, through the
same single-request decision: its own level (first decision wins — a request someone else just decided is reported as
*already handled*), its own approvers, maker-checker and the self-approval policy, its own transaction and row lock.
A request that is no longer yours never stops the others. Levels are never skipped: approving a request at level 1 of
2 only moves it to level 2 (*Now at level 2 of 2*), and the business action runs only after the last level.

Results: approved (carried out) · approved this level (waits for the next) · rejected · already handled · not allowed
· failed, per request. Each decision keeps the comment, the approver and time, and the bulk reference; the audit trail
has each request's own entry plus one `approvals / bulk_approve` or `bulk_reject` entry (counts, items, comment,
session branch). Notifications go once per person: requesters (*3 of your requests were decided by …*), the other
approvers of the decided levels (*… no action needed*), and the next level's approvers.

### Tenant Administrator self-approval (roadmap 150)

Settings → Workflow engine → **Allow Tenant Administrator self-approval** — **on** unless the business switches it off
for strict separation of duties. While on, a **Tenant Administrator** may approve their own request when they are among
its approvers (administrators may decide any level):

- only the verified Tenant Administrator role — the business's system role, the only role with full access, whose name
  and permissions cannot be edited. A role merely named *Administrator* never qualifies, and full access cannot be
  given to any other role;
- only by pressing **Approve** — submitting never approves anything;
- recorded on the decision and in the audit trail as *self-approved (Tenant Administrator)*;
- **never** for money leaving or owed to the business: refunds (sale returns), sale cancellations, credit write-offs
  and recalls, expenses and counter discounts — these always need a second person;
- other people never approve their own requests, whatever their role.

Only a Tenant Administrator, fully signed in, changes the policy (audited `approval_policy`, before / after).

### Why you cannot approve, and requests nobody can approve (roadmap 148)

Every waiting card shows **Assigned to** (Administrator, *Role: …*, *Person: …*, Branch manager) and the next
approvers' names. When the signed-in person cannot decide it, the card says why instead of hiding the buttons:

| Reason | Shown |
|---|---|
| They made the request | *You made this request, so someone else must approve it* |
| They decided an earlier level | *You already decided an earlier level of this request — the next level needs another person* |
| The step is for someone else | *This step is assigned to another approver* |
| No approval permission (a request without a step) | *Your role does not have permission to approve requests* |
| Not a person of this business (e.g. a platform support session) | *Only people of this business can approve its requests* |

Approve / Reject answer with the same reason (403) if someone tries anyway.

**Requests nobody can approve.** When no active person may decide the current step, the card says *Nobody can approve
this request yet* and why: either the only person who qualifies made the request (for example the business's single
full-access administrator asking for a step assigned to *Administrator*), or no active person qualifies (a role with
no active members, a person who left). The requester and everyone who manages workflows are notified the moment it
happens (*Nobody can approve this request*). Nothing is approved, rejected or reassigned automatically. The ways
forward are controlled and audited:

1. an administrator chooses another approver for that step in Settings → Workflow engine — waiting requests follow
   the change step by step (roadmap 49, *pending requests updated*), keeping their requester, history and decisions; or
2. the requester withdraws it (My requests → *Withdraw*) and asks again once someone can approve.

Settings → Workflow engine warns on every Administrator step while only **one** person has full access (*Only one
person can approve the Administrator step*).

**View** opens the request: type, summary, requester, submitted date, branch, amount, status, current level of total,
who it is assigned to, the approval history, the reason (if you cannot decide) and Approve / Reject for those who may.
*Open* goes to the product, sale, transfer or other record.

**Product deactivation:** the product stays active while the request waits; it is deactivated once, when the last
level approves (the request row is locked and its status re-checked, so a second or simultaneous approval is refused
with *already approved*); a rejection leaves it unchanged. Only one deactivation request per product can wait at a
time (*Already waiting for approval*).

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
