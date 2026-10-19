# 06 · Products (master catalogue)

**Scope:** §3 · **Routes:** `/products`, `/products/new`, `/products/:id`, `/products/:id/edit` · **API:** `/api/products…`

Creating a product does **not** add stock — stock is received separately (module 07).

## Product fields
Name · Nickname/short name (optional) · Code (blank = auto `PRD0001`, prefix configurable) · Description (shown on the
ordering link) · Category (+ quick-add) · Supplier (+ quick-add) · Marked selling price · Maximum discount (optional) ·
Cost price (optional, needs financial permission) · Barcode: **shared product barcode** or **track each item
individually** · Active · Available on ordering link · Transfers allowed · All branches or selected branches ·
Low-stock alert level (optional) · Loyalty: eligible, and optional product rule “every X spent earns Y points”.

**Product fields** (Settings → Products → Product fields): your own fields such as Size, Colour, Brand or Expiry —
text, number, date, dropdown, yes/no or email; required or optional; active/inactive; display order. They appear on the
product form and product page and are validated by the server (also when a change waits for approval).

## Photos
Up to **5** per product by default (Settings → Products → Photos per product). Images are resized to WebP in the
browser before upload (≤ 1400 px). One is the **primary photo** (used on the ordering link); others can be promoted or
removed. Photos are hidden on operational screens until *View photos*.

**Adding photos** (new product form, product page, Stock → Receive stock) uses one picker (`components/PhotoPicker`):
select → preview thumbnails with × → add/remove → save. Above the limit every photo stays visible, the extras are
marked *Over limit* (*7 selected · Maximum 5*) and saving is blocked until enough are removed — nothing is dropped
silently. Only images are accepted (the server checks the bytes: JPEG, PNG or WebP, ≤ 3 MB after resizing).
Each photo is saved on its own: the screen reports exactly how many were saved and why any were not, and a saved
product is never reported as failed or created twice. Every pending photo carries an upload id, so a retried or
repeated upload is stored once.

## Catalogue filters
*Active / Inactive / All* · search (name, nickname, code, barcode) · *All categories* · **Price status** (roadmap 140):
*All prices* (default), *Price set* (any selling price, a genuine 0 included) or *Missing price* (no selling price yet —
imported blank or never set). The filters combine; the product count and pages follow them across the whole
catalogue. Open a product with a missing price and set it with *Edit* (or while receiving stock). While *Missing price*
is chosen, the price column shows *Not set* even when prices are hidden from the list.

### Browsing context (roadmap 143)
The catalogue keeps its filters in the address — `/products?status=inactive&category=…&price=missing&q=…&page=2`
(*Active* is the default and not written) — so refreshing, sharing the link, the browser's Back and the page's
**Back** all return to the same view. Opening a product, editing it and saving (or cancelling) returns to the same
filters, search, page and scroll position: after giving a product its price under *Missing price*, Back shows the
remaining products without it and the count drops by one. If an edit empties the current page, the list moves to
the nearest page with results. Only the catalogue address to return to and its scroll position are remembered,
for the browser tab and per business — another business's catalogue context is never used. (The catalogue has
no separate sort control; its order is fixed: active first, then by name.)

## Product page
Price, stock here, stock across branches, photo manager, recent stock movements, details, buttons for
*Performance* (dashboard filtered to the product), *Receive stock*, *Edit*, *Activate/Deactivate*, barcoded items.

## Rules
- Codes are unique per business; a barcode can belong to only one product or one active item.
- Products are never deleted — deactivate them (history stays intact). Inactive products cannot be sold or ordered.
- Approval workflows can gate *creation* (product stays inactive until approved, photos can still be added),
  *edits* (applied on approval) and *deactivation*.

## Permissions
`products.view`, `products.create`, `products.edit`, `products.deactivate`; cost price `sales.view_financials`.
