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

*Values need financial permission unless valuation is “at selling price”. At selling price, each branch's stock is valued at
that branch's price (roadmap 176).

**Branch filter** (roadmap 176, people with several branches): every tab shows the Current Branch by default, another of
the person's branches, or *All accessible branches* (Levels adds the branches up and lists those holding the stock).
Viewing only — receiving, adjusting and transfers keep the working branch, as the page says while another is shown.

**Receiving and prices** (roadmap 176): the price shown is the receiving branch's; typing a different one changes that
branch's own price (several branches) or the default price (one branch), and needs *Set selling prices*.

## Receive stock (Add stock)
Pick a product (search or **scan**) → shown: name, current quantity and value at the chosen branch, marked price, photos.
Capture: quantity (default 1; locked to 1 when configured) · barcodes (tracked products: scan each unit — continuous
scanning; shared-barcode products: scan to confirm/save) · cost price · marked selling price · maximum discount ·
branch · supplier · reference/notes · date received · *Activate product* · *Opening stock*.
Roadmap 200: the product search lists active products only (zero stock included). An inactive product can only be
received by reactivating it with the receipt — *Activate product* (people with `products.deactivate`), opened from the
product's own page; otherwise the receipt is refused with *This product is inactive and must be reactivated before use.* (also on the server).
Price changes update the catalogue and are audited. Optionally approval-gated (*Stock addition*, threshold by value).

### Receipts waiting for approval (roadmap 203)
When a product is chosen, a unit's label scanned, and again when the receipt is submitted (on the server, inside the
same transaction, after locking the product and each unique label so two people cannot submit the same delivery at the
same moment), S'Shop looks for **stock receipts still waiting for approval** (at any approval level):

| Situation | Result |
|---|---|
| A unique item barcode (individually tracked) already in a waiting receipt — **any branch** | Always refused |
| The same delivery under the same delivery note (*Reference / notes*), same branch, product, quantity, barcodes and supplier | Always refused |
| The same branch, product, quantity and barcodes (no delivery note to tell them apart) | Refused with *Block duplicate pending stock receiving submissions* (on by default) |
| Another delivery of the product waiting at the same branch (other quantity, barcodes or delivery note) | A notice; refused only with *Block additional receiving for products pending approval in the same branch* (off by default) |
| A shared barcode waiting at **another** branch | Never blocks — each branch receives its own stock |
| Other barcodes of a product with several barcodes | Compared as a set — a different barcode is a different delivery |

The notice (**Stock Receiving Pending Approval**) shows the product name and code, the barcode (item labels masked unless
*Reveal barcodes*), branch, pending quantity, request reference (REQ-…), delivery note, who submitted it and when,
status and level, and — for the requester, approvers and auditors — who may approve it. Actions: *View pending request*,
*Notify approver* (reminds the people who may decide the current step; once every 10 minutes per request and person;
audited as `approver_reminded`), *Close*. *Receive* stays disabled while a refusal applies. Approving posts the stock
once through the normal checks; rejecting or cancelling releases everything at once (live refresh, no reload).
Settings → Stock → **Stock Receiving Controls**: *Check pending stock receiving requests* (on), *Block duplicate pending
stock receiving submissions* (on), *Block additional receiving for products pending approval in the same branch* (off),
*Allow users to notify assigned approvers* (on). Imports (opening stock from Excel) are approved as a batch and are not
checked against single receipts.

## Adjustments
Types: Damaged · Lost · Write-off · Customer return (back to stock) · Returned to supplier · Recount (set exact
quantity) · Manual correction (±) · **Others** (roadmap 171: last; *Added to stock* or *Removed from stock*, and
*Specify Adjustment Reason* — at least 10 characters — kept as the reason; shown as *Others* followed by it). A **reason is always required**; user, time, previous quantity, change, new quantity
and approver are recorded. Tracked items are adjusted one scanned barcode at a time. A recount re-calculates the
variance at the moment it is applied (sales in between are respected). Approval workflows: *Stock adjustment* and
*Stock write-off*.

### Scan a delivery (roadmap 153)
On Stock → Receive, **Scan delivery** (or the barcode field) adds one unit per scan of a shared barcode to a list of
products with live counts and corrections. The list is **provisional**: it stays on this device per branch, *Clear
delivery* leaves stock untouched, and *Receive N units* (with branch, supplier, reference and date) posts it through
`POST /api/stock/receive/batch` in **one transaction** — every line under the same checks as a single receipt; one line
refused (named in the message) means nothing is received and the list stays. A line needing approval becomes its own
request and adds stock only on final approval; rejected, it adds nothing. Prices are unchanged by a delivery scan: open
a product on its own to change them with the receipt. With quantity entry locked each product stays at 1.

## Deleting inventory captured in error (roadmap 172)
Stock → Movements → **⋯** on a receipt line, or Barcoded items → an item → *Request inventory deletion* (*stock.adjust*).
Shows product, branch, barcode, quantity and stock before → after; a reason is required (any non-blank reason, e.g.
"Error" — roadmap 175; the button is *Request Deletion*). Goes through **Inventory
deletion** approval (on by default); nothing changes while it waits; one request per line. On final approval it is
checked again and reversed by a `receipt_reversal` movement — the original stays, marked **Reversed** — and an item's
barcode is released for reuse unless something still needs it (an active unit, a waiting request, a shared product
barcode). Sold, transferred, written-off or reserved stock is corrected by a return or an adjustment instead.

## Stock take
**On or off (roadmap 164):** Settings → Stock → Stock settings → *Enable stock taking* (default on, every branch,
*settings.stock*). Off hides the button and the server refuses new stock takes (`POST /stock/count`, `/stock/count/compare`:
*Stock taking is off*). Past counts, balances, the audit trail and counts waiting for approval are kept; approvers still
decide those in Approvals. Receiving, transfers, adjustments (including a recount in *Adjust*) and sales are unchanged.

`/stock/count`: a count sheet of all quantity-tracked products; enter counted quantities, see variances live, submit →
one recount adjustment per variance (applied or sent for approval).


**Scanning (roadmap 153).** Scans of a shared barcode count up, corrected in the counter or the row; zero is a real count
and a negative one is refused. Individually tracked units count once each: *Labels to check* lists labels that are
misplaced (in stock at another branch), unexpected (not in stock) or unknown, none of them counted, and each product shows
the units still **missing** (`POST /api/stock/count/compare`, masked; nothing changes). A quantity correction never creates
or removes a physical barcode record.

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

## Receiving across branches and new barcodes (roadmap 197)

A product limited to some branches can be received at any branch: it becomes available there when the receipt is posted
(audited); other branches' stock never changes. A new barcode for a known product is confirmed (*Add barcode & continue*),
sent with the receipt (`add_barcodes`) and registered only when the receipt is posted or approved; without *Edit
products* the receipt goes for approval. One barcode per product (setting), another product's barcode blocked, unique
item barcodes unchanged.

Roadmap 198: the same applies to stock transfers — a transfer into a branch that does not offer the product yet is received there and makes it available.
