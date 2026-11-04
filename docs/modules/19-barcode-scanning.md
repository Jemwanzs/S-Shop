# 19 · Barcode scanning (camera, handheld, manual)

> Seven-digit item barcodes, printed stickers, masked item barcodes and scan-only clearing at the till: see [module 38](38-item-barcodes-labels.md) (roadmap 133–138, 162).

**Scope:** owner request 2026-10-05 · **Code:** `web/src/lib/scanner.ts` (engine), `web/src/components/BarcodeScanner.tsx`
(the one scanner UI) · **API:** `GET /api/products/lookup?code=&branch_id=`

## Two barcode tracking models (roadmap 153)

| | **Shared barcode — quantity tracking** | **Individual barcode tracking** |
|---|---|---|
| Stored as | `products.track_items = false`, `products.barcode` | `track_items = true`, one `stock_items` row per unit |
| A scan means | one more unit of the product | this one physical unit |
| Scanning the same code again | quantity + 1 | *Already scanned — 1 unit captured*, warning sound, still one unit |
| Corrections | − / typed quantity / + / remove (the count only) | remove the unit, scan another |
| Registered | the first receipt can save the code as the product barcode | only when stock is posted or finally approved |

Products → Barcode shows both side by side (the second is the old *Track each item individually*). **Changing a saved
product's model** is checked first (`GET /api/products/{id}/tracking-check?individual=`) and again on save and on
approval. Refused while stock requests for it wait for approval, transfers of it are open, units are reserved for
orders (to individual) or units carry their own barcode (to shared). Going to individual with stock on hand warns that
it needs *Label existing stock*. Switching barcode management off never changes a product's model.

## Scanning sessions (roadmap 153)

`ScanCounter` (`web/src/components/ScanCounter.tsx`) sits inside the camera scanner and under the inline field: product,
barcode, model, **N units captured**, − / field / + / remove, the other products and the session total. A correction
applies at once and focus returns to the scanning field; a code arriving at scanner speed in the quantity field is
treated as a scan, never as a quantity.

- **In order, none lost:** codes queue and run one at a time, each seeing the count the last one left; the field clears
  as soon as a code is submitted. The same code again within 120 ms is one trigger press reaching the page twice. The
  camera still counts a label once until it has been out of view for 1.5 seconds (*Move the barcode away and back*).
- **One sound per scan**, once the result is known (`scanFeedback` in `scanner.ts`): beep (accepted), two mid tones
  (already scanned), low tone (refused). A correction gives a visual confirmation only. Phones allow sound only after a
  touch, so the first touch of the page unlocks it.

## One scanner everywhere
Every flow calls the same `BarcodeScanner` with a handler that returns an outcome (`success` / `info` / `error` + optional
actions). Three inputs feed the same handler:
- **Phone camera** — opens full screen on phones (compact dialog on larger screens), rear camera by default, continuous
  decoding until a barcode is read. Torch and camera-switch buttons appear when the device supports them.
- **Handheld scanner** — keyboard-wedge scanners type into the code field and press Enter (focused automatically on
  desktop).
- **Manual entry** — type the code and tap Add.

Decoding: the browser's native `BarcodeDetector` when it supports retail 1D formats (Android Chrome, macOS), otherwise
ZXing (iPhone Safari, Firefox, Windows Chrome) on frames grabbed from the same camera stream, cropped to the central band
where the on-screen frame is. Formats: EAN-13, EAN-8, UPC-A, UPC-E, Code 128, Code 39, Code 93, ITF, QR.

Behaviour: the same label seen continuously counts once; success shows a short green confirmation and (in continuous
flows) keeps scanning; an error pauses the camera with **Scan again** plus flow-specific actions (a handheld scanner
keeps working). One sound and vibration per scan. The camera is requested only when the scanner opens and stops on close, on leaving the page, and when the app
goes to the background. Camera problems (blocked permission, no camera, camera busy, insecure connection, unsupported
browser) show a clear message while handheld/manual entry keep working. Production runs over HTTPS, which browsers
require for camera access.

## Barcode settings, the inline field and stock takes (roadmap 98)

**Settings → Stock → Barcode settings** (on the existing settings, no parallel ones):

| Switch | Setting |
|---|---|
| Enable barcode management | `stock.barcode_requirement` ≠ `disabled` (off: products, stock and sales work without barcodes) |
| Allow manual barcode entry (default on) | `stock.manual_barcode_entry` — off: typed / pasted codes are not offered; handheld scanners still work |
| Enable camera scanning (default on) | `stock.camera_scanning` |
| Require barcode during stock-in | `stock.barcode_requirement` = `required` |
| Require barcode during stocktaking | `stock.require_barcode_stocktake` — products with a barcode are counted by scanning or entering it (server-enforced) |
| Require barcode during sales | `sales.require_barcode_clearance` |

The card also lists products sharing a barcode (from before uniqueness was enforced), linked to their edit forms.

- **One inline field** — `web/src/components/BarcodeInput.tsx`, *Enter or scan barcode*: Enter submits, the outcome
  shows under the field with its sound (no popup: it would catch the next scan, roadmap 153), the field clears and keeps focus for the next code, and the
  camera button opens the shared scanner. Used on the stock take, stock-in (find the product; each unit of a tracked
  product) and, as Enter in the search box, at Record Sale (a barcode adds like a camera scan; anything else stays a
  search).
- **Product barcodes** — stored as text (leading zeros kept); 3–64 letters, digits or `- . _ / +` (check digits are not
  enforced, so manufacturers' codes are never wrongly refused); unique per business against products and live item
  labels (the clashing product is named), and by a database index (created at start-up once no duplicates remain);
  **Generate** gives a unique in-store EAN-13 (prefix 2, valid check digit).
- **Stock take** — counts by barcode (product-level: +1 per read, quantity editable afterwards; required-by-barcode rows
  stay locked until scanned) and, for individually tracked products, unit by unit: each unit's label counted once
  (duplicates and units not in stock here are flagged at once and refused by the server), expected / counted /
  variance per product, and units not found recorded as *loss* adjustments on submit (approvals as configured);
  *None found* counts a tracked product as empty, and a product not scanned is left alone. The session is kept on the
  device per branch until submitted or cleared; nothing changes live stock before submission.

## Flows
| Where | What a scan does |
|---|---|
| Sales → Record Sale → **Scan** | Adds to the cart immediately (continuous). Product barcode: qty +1 per scan (respects stock at the Current Branch unless negative stock is allowed; also clears *Require barcode clearance*). Item barcode (tracked products): that exact unit, only if in stock at the Current Branch and not already in the cart. Out of stock here → shows other branches with stock. Unknown code → *Barcode not found* with Scan again · Search product · Assign barcode (needs `products.edit`). Nothing is ever created from an unknown code during a sale. |
| Stock → Receive stock | **Scan delivery**: each shared barcode adds a unit to a provisional list of products; nothing reaches stock until *Receive* (one transaction; see [module 7](07-stock.md)). A tracked product opens on its own, where each unit's label is scanned; codes already in stock or product barcodes are refused. Unknown → Assign barcode / New product. |
| Transfers → New → **Scan** | Adds products (qty +1, capped at available) or specific units (must be in stock at the sending branch). |
| Stock take → **Scan to count** | Each read adds one to that product's counted quantity (corrected in the counter or the row). Tracked: each unit once; missing units and labels that do not belong are listed. |
| Sale → Return items → **Scan returned items** | Ticks the matching line (unit barcode, else product barcode); only items on that receipt. |
| Orders → completing an order | Orders with tracked products require scanning each unit handed over (checked at the order's branch); the server refuses completion until the scanned count matches. |
| Products → product form, Stock → trace / adjust, item sheet | Single scan fills the barcode field. |

**Assign barcode:** opens Products with a banner (*Choose the product for barcode …* or *New product*); picking a
product opens its edit form with the barcode filled in, saved through the normal edit flow (and its approval workflow).

## Strict validation at Record Sale
When an item needs clearance (tracked per unit, or *Require barcode clearance* with a product barcode), the item sheet
sends every scan — camera, handheld or typed — to `POST /api/sales/check-barcode` **before** *Add to cart*; *Add to
cart* stays disabled until the server confirms. The same check (`claim_item` / product-barcode check in
`routes/sales.rs`) runs again at checkout, so there is one set of rules:

| Refusal (title) | When |
|---|---|
| Barcode mismatch | The code belongs to another product (unit or product barcode) |
| Unknown barcode | Not registered to any unit or product |
| Wrong branch | The unit is in stock, but at another branch |
| Item already sold · Item in transit · Item reserved · Item written off · Returned to supplier | The unit's status |
| Already in this sale | The same unit twice in one cart (checked in the sheet and at checkout) |
| Scan required | Checkout without the scan |

Nothing is reserved or cleared at scan time. At checkout the unit's row is **locked** for the transaction, marked
*sold*, and a `sale` movement records that exact unit; a second till selling the same unit at the same moment waits
for the lock and is refused (*Item already sold*). Products without any barcode on record cannot be cleared by
scanning (*No barcode registered*); they sell without clearance.

## Lookup API
`GET /api/products/lookup?code=` returns the product (cost price hidden without financial access), the stock item when
the code is a unit barcode (`status`, `branch_name`, `in_current_branch`) and `other_branches` with available stock
elsewhere. Unknown codes return 404.
