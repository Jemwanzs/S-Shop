# 07 · Stock & inventory

**Scope:** §4, §5, §14, §15, §33–34, §41 · **Routes:** `/stock`, `/stock/receive`, `/stock/count` · **API:** `/api/stock…`

## Principle: the inventory ledger
Stock is never an editable number. Every change is a **movement** (received, opening, sale, order completion, transfer
in/out, customer return, sale reversal, supplier return, damage, loss, write-off, adjustment, count variance) and
**current stock = Σ movements**. Physical − reserved (for confirmed orders) = **available to sell**.

## Stock page tabs
| Tab | Shows |
|---|---|
| **Levels** | Per product at the Current Branch: physical, reserved, available, status (in / low / out), value*. Filters: status, search, category. |
| **Position** | For a period: opening, added, transfers in/out, sold, returns, adjustments, damaged/written-off, closing, reserved, value*. |
| **Movements** | The ledger: when, product, barcode, kind, ±qty, branch, reference, user. Rows link to the sale/transfer. |
| **Adjustments** | Every adjustment with before → after, reason, requester, approver, status. |
| **Barcoded items** | Individually tracked units by status; **Trace barcode** shows the full life of a code. |

*Values need financial permission unless valuation is “at selling price”.

## Receive stock (Add stock)
Pick a product (search or **scan**) → shown: name, current quantity and value at the chosen branch, marked price, photos.
Capture: quantity (default 1; locked to 1 when configured) · barcodes (tracked products: scan each unit — continuous
scanning; shared-barcode products: scan to confirm/save) · cost price · marked selling price · maximum discount ·
branch · supplier · reference/notes · date received · *Activate product* · *Opening stock*.
Price changes update the catalogue and are audited. Optionally approval-gated (*Stock addition*, threshold by value).

## Adjustments
Types: Damaged · Lost · Write-off · Customer return (back to stock) · Returned to supplier · Recount (set exact
quantity) · Manual correction (±). A **reason is always required**; user, time, previous quantity, change, new quantity
and approver are recorded. Tracked items are adjusted one scanned barcode at a time. A recount re-calculates the
variance at the moment it is applied (sales in between are respected). Approval workflows: *Stock adjustment* and
*Stock write-off*.

## Stock take
`/stock/count`: a count sheet of all quantity-tracked products; enter counted quantities, see variances live, submit →
one recount adjustment per variance (applied or sent for approval).

## Barcodes
Camera scanning (native BarcodeDetector, ZXing fallback) and keyboard-wedge handheld scanners. Settings → Stock →
**Barcode requirement**: Required / Optional / Disabled. Settings → Sales → **Require barcode clearance**. A barcode can
identify only one active unit (database-enforced); sold units cannot be sold again.

## Alerts
After sales and transfers, products at/below their threshold notify users with `stock.add` at that branch (once a day
per product): *Low stock* / *Out of stock*.

## Permissions
`stock.view`, `stock.add`, `stock.adjust`, `stock.write_off`, `stock.transfer`, `stock.receive_transfer`.

## Settings
Stock → barcode requirement, quantity entry, capture cost, valuation (cost/selling), default low-stock level, transfer
receipt control, allow negative stock (off by default).

## Stock balances from Excel (roadmap 142)

Stock → **Stock balances** (and Settings → Business → Onboarding → *Stock balances*), permission *Adjust & count stock*:

| Action | What it does |
|---|---|
| **Download template** | A workbook already filled with this business's products × the branches the person works in (one branch or all): Product Code, Product Name, Branch, **Available Stock / QTY** (today's balance), **Unit Cost** (recorded cost), **Date As Of** (today), Item Barcode, **Notes**; reference columns shaded, editable ones white; hidden Product ID, Branch ID and *System QTY* (the balance at download). An **Item Barcode Details** sheet lists every labelled unit — masked (`****047`) unless the person may reveal item barcodes (the download is audited). |
| **Upload stock balances** | The same import engine as onboarding: read → validate → **preview** (Product · Branch · Current · Uploaded · Difference · Unit cost · Status, with +/− totals) → apply (approval may apply) with real progress → results and an Excel error report. |
| **Download current balances** | The stock position report as Excel. |

Rules:
- The uploaded quantity is the **target balance** (a count): 12 → 15 records +3; it is never added on top.
- Refused rows (the upload applies completely or not at all): an unknown product or a changed code or branch (checked
  against the hidden ids); a branch the person does not work at; a duplicate product × branch; a negative quantity or
  cost; a future date or one in a closed period; **stock that moved since the sheet was downloaded** (or, for a sheet
  without the hidden column, since it was uploaded) — download a fresh sheet; a past *Date As Of* when the stock moved
  after it (a balance is never applied over later movements); an individually tracked product whose quantity differs
  from its labelled items (reconcile item by item — stock take by scanning, receive with labels, write off) or that
  still has unlabelled units (label existing stock first). Item barcodes in the sheet are reference only — never
  imported.
- Unit Cost values only the units an upload adds; a cost different from the recorded one is flagged and the recorded
  cost is never revalued (change it in Products).
- Applying re-checks live balances inside the transaction; each change is a **count adjustment** (reason *Stock balance
  upload IMP-…* + notes) with a `count_variance` movement on the *Date As Of*, linked to the upload
  (`stock_adjustments.import_batch_id`, `effective_date`); audit `stock.balance_upload` (old, new, difference, date, upload).
  Unchanged rows change nothing. The same workbook is never applied twice.
- Approval: when the business's *Data import* or *Stock adjustment* workflow needs approval, the upload waits for it.
- Stock balance uploads are operational: they are not part of the migration reconciliation.
