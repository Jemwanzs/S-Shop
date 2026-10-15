# 36 — POS refinements, sale dates & receipt logo (roadmap 114–117)

Briefs (James Sammy, SyncScore): *POS Stock Visibility & Image-Based Product Prioritization*, *Editable Sale Date &
Product-Level Transaction Dates*, *Receipt Logo Transparency & Export Consistency*, *New Sale Category Navigation & UI
Refinement*. All four enhance existing screens only (Sales → New Sale, the cart, receipts) and reuse the existing
settings, permissions, audit trail, stock engine (`record_sale`) and receipt pipeline.

| Phase | Roadmap | Status |
|---|---|---|
| POS Display: show available stock count; products with images first | 114 | ✅ built |
| Editable sale date (whole sale and per product), backdating rules | 115 | ⏳ planned |
| Receipt logo transparency, same look in preview / PDF / image / print | 116 | ✅ built |
| New Sale category strip: cream background, overflow arrows, smart scrolling | 117 | ✅ built |

## 114 — POS stock visibility & image-based prioritisation

Settings → Sales & Payments → POS Display (`sales.pos_display`):

- **Show available stock count** (default **on**) — on: *100 left* / *1 left* on each card as today; off: the number is
  hidden. Out-of-stock cards still say *Out of stock* and cannot be sold, low stock still warns, and stock validation,
  alerts and balances are unchanged.
- **Show products with images first** (default **on**) — products with a primary photo (the existing photo record; no
  image is downloaded to decide) come before products without one; nothing is hidden. Off: the previous order.
- Ordering (server-side, before the result is cut to the page, so it holds for every product shown):
  1. search and category decide which products qualify;
  2. top-selling ranking first, when enabled (roadmap 110);
  3. products that can be sold before out-of-stock ones (existing rule — an out-of-stock picture never pushes sellable
     products down);
  4. products with images first, when enabled;
  5. the existing order (name).
- Applies the same on phones, tablets and desktops; settings take effect on the next product list load.
- As built: `PosDisplay.show_stock` / `images_first` (both default on); with the count off a card shows *Low stock*
  below the low-stock threshold and *Out of stock* when none is left, nothing otherwise. "Has an image" = a
  `product_photos` row exists (no image request).

## 115 — Editable sale date & product-level dates

- **Sale Date** on New Sale, next to Sale Owner: defaults to today in the business's time zone; people with the new
  permission **Record sales with an earlier date** (`sales.backdate`) may pick any earlier date; future dates are
  refused (app and server). It applies to every cart line unless that line has its own date.
- **Product Sale Date** in the Add to Cart and Edit cart item dialogs: defaults to the Sale Date, can be changed per
  line (never in the future), shows the effective date, **Use Sale Date** resets it. Lines
  with their own date keep it when the Sale Date changes. The cart shows a line's date only when it differs.
- Saved separately: the sale's effective date (`sales.business_date`) and each line's date (`sale_items.sale_date`);
  the real entry time (`created_at`) is never changed.
- Stock: one movement per line, dated the line's date (`occurred_on` / `business_date`), recorded once. A backdated
  line is refused when it would make the stock balance negative on any day from that date to today (*Not enough
  stock on 10 Oct 2026*).
- Payments: cash, card, bank, credit and manually entered references follow the sale date; M-Pesa STK confirmations
  keep their real confirmation time and day.
- Reports: product-level reports (sales by product, top products, stock movement) use line dates; receipts, payments
  and sale totals use the sale date; the Sales report shows the effective date and the recorded time.
- Audit: backdated sales are recorded with the effective date, line dates and the real entry time. Financial-period
  locks: S'Shop has none yet; when added they apply here.
- Dialogs keep entered data (no closing by tapping outside — roadmap 104).

## 116 — Receipt logo transparency & export consistency

- The downloaded PDF showed a black box behind transparent logos (the alpha channel was lost when the logo was
  converted for the PDF). One shared logo preparation for every format: transparent pixels stay transparent, or are
  flattened onto the receipt's own white where a format needs it — never black, never a box or border.
- The same look in the receipt preview, PDF, image, print, shared receipt links and email / WhatsApp sharing;
  proportions, sharpness, width and dynamic height unchanged; the S'Shop footer mark rendered the same way.
- Tested with transparent, solid-background and dark logos.
- As built: the cause was the WebP logo (stored that way since roadmap 112) being embedded by jsPDF as a JPEG
  (`/DCTDecode`, no alpha) — transparent pixels became black. `receiptImage()` (lib/receipt.ts) now redraws every
  receipt image (logo and S'Shop mark) on a transparent canvas and embeds it as PNG with its alpha mask (`/SMask`,
  lossless), never enlarged (≤ 600 px). Preview, print and the image export draw the same logo in the page (white
  receipt background), so all formats match.

## 117 — New Sale category navigation

- The category pills sit on a subtle warm-cream strip (a theme token, with a dark-mode equivalent), distinct from the
  product cards; pills, spacing, typography and the active (dark) *All* pill unchanged.
- One horizontal row, no visible scrollbar, swipe on touch; small left / right arrows appear only when categories are
  hidden on that side and scroll smoothly; the selected category scrolls into view and stays selected while the cart
  changes. Never overlaps the search, grid or cart.
- As built: the shared `ScrollRow` (roadmap 79) on a `bg-strip` token (`--strip`, light 36 58% 93%, dark 26 18% 13%);
  `arrowsOnPhones` shows the compact arrows on phones too.
