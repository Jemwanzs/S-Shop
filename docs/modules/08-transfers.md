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
| **Receive** | `stock.receive_transfer` at the destination | Confirm what arrived: per line *short* and *damaged* (tracked units: Received / Missing / Damaged), reason required when anything is off. `transfer_in` for everything sent, then `loss` (short) and `damage` (damaged) at the destination — only good units become sellable; short/damaged tracked units are written off |
| Cancel | before dispatch | none |

With **Transfer receipt control** off (Settings → Stock) dispatching also receives immediately.
A dispatched transfer cannot be cancelled — receive it, then transfer it back.

## Creating a transfer
Source branch (*From branch*, the person's branches; default the working branch), destination branch, date,
reason/notes, products with stock at the source (quantity steppers; tracked products are scanned unit by unit).
Products must allow transfers and be sold at the destination. Save as draft or submit.

## Selling prices at the destination (roadmap 176)
*Selling prices at {destination}* shows each product's price at both branches. Per product: **Keep Destination Price**
(default — a transfer never changes a price by itself), **Use Source Branch Price** or **Set Custom Destination
Price**; any change needs the confirmation tick. A transfer is never blocked because the prices differ.

Stock approval and price authorisation are separate. A change is *authorised* when the person can set prices at the
destination (*Set selling prices* and that branch); otherwise it is *requested* — the people who can are notified and
*Authorise price* or *Decline* on the transfer. An authorised change applies when the stock is **received** (at once if
authorised after receipt), as the destination's own price, audited. If the destination's price changed after the
transfer was made, it is not overwritten: the change returns to *requested* with the reason. Cancelled or rejected
transfers change no price. Statuses: unchanged · requested · authorised · applied · declined · not applied.

## Approval
The request reads *Transfer TRF-… · 3 × Product (+N more) · From → To*; the card lists every product with its quantity,
source and destination price and the proposed price action (*Show all* for long transfers).

## Notifications
Dispatch notifies the destination's receivers (“Transfer TRF-… on its way”); receipt notifies the creator and the
dispatcher — *received with discrepancies* (with the counts and reason) when anything was short or damaged. The
transfer keeps the per-line received/short/damaged figures and the reason; the receipt is audited with them.

## History
Each transfer shows items, route, every actor and time (created, approved, dispatched, received). Stock Transfer Report.
