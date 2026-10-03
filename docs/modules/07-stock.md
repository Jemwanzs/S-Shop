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
