# 38 — Item barcodes, labels, concealment & scan-to-clear (roadmap 133–138)

This module builds on individual-item tracking ([module 19](19-barcode-scanning.md)). There, each physical unit of a
product set to *individual items* already has its own barcode in `stock_items`, carried through receiving, sales,
transfers, stock takes, returns and write-offs. This module adds:

- S'Shop's own seven-digit codes;
- printed stickers;
- masking in ordinary screens;
- scan-only clearing at the till;
- barcode hardware settings.

| Part | Where |
|---|---|
| Registry, generation, labels, replace, label existing stock, reveal, capacity | `server/src/routes/barcodes.rs` |
| Schema | `migrations/0039_item_barcode_registry.sql` — `barcode_registry`, `stock_item_barcode_history`, `mask_item()`, `sale_items.scan_source`, the barcode permissions |
| Receiving checks | `server/src/routes/stock.rs` (`check_receipt` → `barcodes::check_new_item_codes`; `execute_receipt` → `barcodes::assign`; `labels_attached` / `labels_verified`) |
| Till enforcement | `server/src/routes/sales.rs` (`prepare_lines`: `scan_source`), `orders.rs` (order completion `sources`) |
| Code 128 and stickers | `web/src/lib/code128.ts`, `web/src/lib/labels.ts` (SVG / print page / PDF / PNG / ZPL / TSPL), `web/src/components/LabelsDialog.tsx` |
| Screens | Stock → Receive (`ReceiveStock.tsx`), Product → *Barcode labels* and Stock → *Barcoded items* (`components/ItemBarcodes.tsx`), Settings → Stock (*Barcode settings*, *Barcode hardware* — `settings/BarcodeHardware.tsx`), Settings → Sales & payments (*Barcode controls*), Platform → Security & features (*Item barcodes*) |
| Capture source | `web/src/lib/scan.ts` (key timing; `useManualBarcodes`), `BarcodeScanner` / `BarcodeInput` `context="sale" \| "stock"` |

## 133 — Seven-digit item barcodes

**Issuing codes**
- Format: `1000000–9999999`, never starting with 0.
- Codes come from the operating system's cryptographically secure generator (`OsRng`). They are never sequential.
- Each code goes into `barcode_registry` (the primary key is the code) and is issued once, across every business on
  the platform.
- Before issuing, S'Shop also checks the code is not on any item, product or replaced label anywhere.
- When a random draw collides, S'Shop simply draws again.

**Never recycled.** A registry row stays forever: allocated → assigned → (retired when replaced). This holds after a
sale, return, write-off, product deletion or business deletion; in the deletion cases the row stays, detached.

**Codes the business enters**
- Any code typed or scanned by a business in the seven-digit range is registered too, so S'Shop never issues it.
- A new unit never takes a code that S'Shop issued to another business or to another product.
- A new unit never takes any code this business already put on an item, including retired ones.

**Where codes come from**
- Stock → Receive: *Generate barcodes*.
- Opening-stock import: leave Item Barcodes blank.
- *Label existing stock*.
- *Replace label*.
- Product-level (shared or manufacturer) barcodes and the product *Generate* (in-store EAN-13) are unchanged.

**Capacity** (Platform → Security & features)
- The card shows codes issued, on items and still available.
- It warns at 70 %.
- At 95 % S'Shop stops issuing seven-digit codes; past that point random draws would mostly collide.
- *Migration plan:* switch to eight digits (`10000000–99999999`). Widen the registry `CHECK` and `is_item_code`; existing
  seven-digit labels stay valid, since scanners read the full code and the registry keeps them unique.

**Permissions** (Settings → Roles → *Item barcodes*):

| Permission | Who gets it by default |
|---|---|
| *Generate item barcodes* | anyone who can receive stock |
| *Print & reprint labels* | anyone who can receive stock |
| *Reveal full item barcodes* | anyone who can write stock off |
| *Replace item barcodes* | anyone who can write stock off |
| *Type barcodes at the till* | anyone who can approve discount overrides |

## 134 — The S'Shop sticker

**Layout:**
- Top: the **business name**, in capitals, from the business profile — never the branch.
- Middle: **Code 128** bars with 10-module quiet zones.
- Below the bars: the **seven-digit number**.
- Bottom: the **product name**, small and shortened to fit.

The background is white with black bars. There is no price, internal id, branch or staff name.

**Sizes:**
- 40 × 25, 50 × 30 or 60 × 40 mm, or custom (25–100 × 15–80 mm).
- The default is set in Settings → Stock → *Barcode hardware*.
- The module width follows the label width, so the bars stay scannable at 40 × 25 mm (≈ 0.33 mm per module).

**Outputs:**
- Preview.
- **Print**: the system print dialog, one label per page for label rolls or A4 sheets.
- **PDF**: vector bars.
- **PNG**: about 300 dpi; a sheet when there are several labels.
- **ZPL** (Zebra) or **TSPL** (TSC) files for label printers.

Batch printing covers every unit at once, with no opening each label.

Tests decode the printed bars with the app's own scanner engine.

## 135 — Receiving with generated barcodes

**Stock → Receive, individually tracked product.** The flow is:

1. *Generate barcodes*, or *Scan / enter existing*.
2. Enter how many items arrived.
3. One code per unit is issued, and *N labels ready to print* appears.
4. **Print labels**.
5. Tick *I have attached a label to each item*.
6. When the business requires it, scan every label: *Verify labels* (k/N).
7. **Receive**, through the existing approval workflow.

The server refuses a receipt containing freshly generated codes until the labels are confirmed attached, and verified
when *Require barcode verification before completing a receipt* is on. Printing is never taken as attaching.

**Product → Barcode labels:**
- *Print labels* for every unit in stock at the branch.
- *Label existing stock*: for a product switched to individual items while it already had stock, one code per
  unlabelled unit. The stock count does not change.
- A product cannot switch back to one shared barcode while units still carry their own.
- A stock take of an individually tracked product waits until stock from before tracking is labelled (*Label existing
  stock first*), so unlabelled units are never recorded as lost.

**Stock → Barcoded items → an item:**
- *Reprint label*.
- *Replace label*: a reason is required. The item gets a new code; the old one is retired, is never reused and can
  still be traced.
- *Reveal*.

## 136 — Concealed item barcodes

Item barcodes show as `****047` in these places:
- stock lists and movements;
- product details;
- stock takes;
- transfers;
- sales, receipts, credit sales and exchanges;
- Record Sale, after a scan;
- search results;
- reports and Excel or PDF exports;
- the receipt's audit entry;
- approval requests (receipts, transfers, adjustments waiting for a decision): masked when shown; the stored request
  keeps the full codes the approved action needs.

The server does the masking (`mask_item()` in SQL, `barcodes::mask` in Rust), so the full value never reaches an
ordinary screen. Product-level barcodes are printed on every pack, so they are not concealed.

Full values are released only through audited actions:
- printing or reprinting labels (*Print & reprint labels*);
- replacing a label;
- *Reveal* (*Reveal full item barcodes*), which records `barcodes.revealed`.

The printed sticker keeps the full code. Scanning works everywhere because the server compares the scanned code.
Masking protects privacy on screen; it is not a security barrier on its own.

## 137 — Scan-to-clear at the till

**The till setting.** Settings → Sales & payments → *Barcode controls* → **Allow manual barcode entry during sales**,
default **off**: *"When disabled, sales agents must scan the physical barcode attached to the item. Typing barcode
numbers manually is not permitted for barcode-controlled products."*

**When it is on**, only people with *Type barcodes at the till* may type codes. Every code is still validated: the
right business, product, branch and status, and not already sold or in the sale.

**Where it applies**
- It applies wherever stock is cleared: Record Sale (cash and credit), exchanges and order completion.
- Barcode-controlled means individually tracked products, plus products whose barcode must clear them (*Require
  barcode clearance*).
- Choosing a product and a quantity never clears a tracked unit; the server requires the unit's barcode.

**Capture source.** Each line records how its barcode arrived in `sale_items.scan_source`:
- `scanner`: keyboard-style handheld;
- `camera`;
- `manual`: typed.

**Telling a scanner from typing.** Handheld scanners "type" a whole code in a few milliseconds. The till measures key
timing: under 35 ms per character counts as a scanner. Pasting always counts as typed.

**Honest limit:** a keyboard-style scanner cannot be told apart from very fast typing with certainty. The source is
enforced as far as the till can tell and recorded as information, not proof. Camera scanning is the stronger option.

**Other refusals.** The server refuses a line marked as typed when typing is not allowed. It also refuses an item
twice in one sale (the message shows only the masked code), a sold item, another product's item, and another branch's
item.

**Offline.** Barcode-controlled sales need a connection (the existing offline rule). The unit is locked when the sale
is prepared and marked sold in the same transaction.

**Stock settings** (Settings → Stock → *Barcode settings*)

| Setting | Default | What it does |
|---|---|---|
| *Allow manual barcode entry during stock receiving* | on | Codes can be typed when receiving; format, uniqueness and never-reuse are still checked |
| *Generate unique barcodes automatically* | on | The *Generate barcodes* option when receiving |
| *Require barcode verification before completing a receipt* | off | Every new label must be scanned before the receipt completes |

These are separate from the till setting. With typing off for receiving, the server refuses typed codes as it does at
the till (`barcode_sources` on the receipt), and the receive, adjustment and shared-barcode fields accept scans only.

## 138 — Barcode hardware

Settings → Stock → *Barcode hardware*:
- the standard label size;
- the label printer: system / browser printing, a ZPL printer or a TSPL printer;
- **Print a test label**;
- **Test your scanner**: it shows what arrived, how many characters, and whether it came from the scanner, the camera
  or typing;
- connection guidance.

| Device | How it works with S'Shop |
|---|---|
| USB or Bluetooth handheld scanner | Acts as a keyboard in every barcode field; set it to send Enter after each code |
| Phone camera | Built-in scanning on S'Shop's secure address |
| Label printer (USB, Bluetooth, Wi-Fi) | Installed on the device and printed through the print dialog at 100 % scale; or a PDF / PNG |
| Zebra or TSC printer | The ZPL / TSPL file, sent with the printer's own utility |

iPhone and most browsers do not let web pages control printers directly. S'Shop never claims direct printer access; a
local print bridge is a possible later addition.

## Audit

Audit actions:
- `barcodes.generated`
- `barcodes.labels_printed` and `barcodes.labels_reprinted` (count, products, masked codes)
- `barcodes.replaced`
- `barcodes.existing_stock_labelled`
- `barcodes.revealed`
- `stock.receive` (masked codes)
- sales (the line keeps the full code and its capture source)
