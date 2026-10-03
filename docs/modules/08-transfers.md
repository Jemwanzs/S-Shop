# 08 · Stock transfers

**Scope:** §13 · **Routes:** `/transfers`, `/transfers/new`, `/transfers/:id` · **API:** `/api/transfers…`

## Workflow
`Draft → Pending approval → Approved → Dispatched (In transit) → Received` · also `Cancelled`, `Rejected`.

| Step | Who | Stock effect |
|---|---|---|
| Create (from the Current Branch) | `stock.transfer` | none — availability checked |
| Submit | `stock.transfer` | none; goes to *Pending approval* if the *Stock transfer* workflow is on, else *Approved* |
| Approve / reject | approver | none |
| **Dispatch** | `stock.transfer` at the source | `transfer_out` movements; tracked items → *in transit* (re-checked: must still be in stock) |
| **Receive** | `stock.receive_transfer` at the destination | `transfer_in` movements; items → *in stock* at the destination — only now sellable there |
| Cancel | before dispatch | none |

With **Transfer receipt control** off (Settings → Stock) dispatching also receives immediately.
A dispatched transfer cannot be cancelled — receive it, then transfer it back.

## Creating a transfer
Destination branch, date, reason/notes, products with stock at the source (quantity steppers; tracked products are
scanned unit by unit). Products must allow transfers and be sold at the destination. Save as draft or submit.

## Notifications
Dispatch notifies the destination's receivers (“Transfer TRF-… on its way”); receipt notifies the creator.

## History
Each transfer shows items, route, every actor and time (created, approved, dispatched, received). Stock Transfer Report.
