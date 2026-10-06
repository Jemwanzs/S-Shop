# 19 · Barcode scanning (camera, handheld, manual)

**Scope:** owner request 2026-10-05 · **Code:** `web/src/lib/scanner.ts` (engine), `web/src/components/BarcodeScanner.tsx`
(the one scanner UI) · **API:** `GET /api/products/lookup?code=&branch_id=`

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
flows) keeps scanning; an error pauses scanning with **Scan again** plus flow-specific actions. Beep + vibration give
feedback. The camera is requested only when the scanner opens and stops on close, on leaving the page, and when the app
goes to the background. Camera problems (blocked permission, no camera, camera busy, insecure connection, unsupported
browser) show a clear message while handheld/manual entry keep working. Production runs over HTTPS, which browsers
require for camera access.

## Flows
| Where | What a scan does |
|---|---|
| Sales → Record Sale → **Scan** | Adds to the cart immediately (continuous). Product barcode: qty +1 per scan (respects stock at the Current Branch unless negative stock is allowed; also clears *Require barcode clearance*). Item barcode (tracked products): that exact unit, only if in stock at the Current Branch and not already in the cart. Out of stock here → shows other branches with stock. Unknown code → *Barcode not found* with Scan again · Search product · Assign barcode (needs `products.edit`). Nothing is ever created from an unknown code during a sale. |
| Stock → Receive stock | *Find by scan* picks the product (unknown → Assign barcode / New product). Tracked products: scan each unit's label; codes already in stock or product barcodes are refused. |
| Transfers → New → **Scan** | Adds products (qty +1, capped at available) or specific units (must be in stock at the sending branch). |
| Stock take → **Scan to count** | Each read adds one to that product's counted quantity. |
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
