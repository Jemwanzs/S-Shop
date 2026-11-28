# 06 · Products (master catalogue)

**Scope:** §3 · **Routes:** `/products`, `/products/new`, `/products/:id`, `/products/:id/edit` · **API:** `/api/products…`

Creating a product does **not** add stock — stock is received separately (module 07).

## Product fields
Name · Nickname/short name (optional) · Code (blank = auto `PRD0001`, prefix configurable) · Description (shown on the
ordering link) · Category (+ quick-add) · Supplier (+ quick-add) · Marked selling price (the **default price**; a branch may have its own — see below) · Maximum discount (optional) ·
Cost price (optional, needs financial permission) · Barcode: the product's tracking — the business default from
Settings → Products (roadmap 168), or the other mode chosen for this product as an exception — and the shared product
barcode, and its **other barcodes** (roadmap 173: the same product in other packaging; add and remove on Product details) · Active · Available on ordering link · Transfers allowed · All branches or selected branches ·
Low-stock alert level (optional) · Loyalty: eligible, and optional product rule “every X spent earns Y points”.

**Product fields** (Settings → Products → Product fields): your own fields such as Size, Colour, Brand or Expiry —
text, number, date, dropdown, yes/no or email; required or optional; active/inactive; display order. They appear on the
product form and product page and are validated by the server (also when a change waits for approval).

## Prices per branch (roadmap 176)
One product, one record and one code in every branch; its **default price** applies wherever a branch has no **own
price**. With several branches, Edit → Pricing shows *Marked selling price — {Current Branch}* (a change applies only
there; *Use the default price* removes it), the **Default selling price**, and the other branches the person works at.
Product details show *Price by branch*. Needs **Set selling prices** (`products.price`), a branch's price only at the
person's own branches, the default price only by someone who has every branch using it. A price changed by someone
else since the form was opened is refused (*Price changed meanwhile*); the *Product edit* approval applies when on;
every change is audited. The till, barcode scans, orders, the ordering link and website, search and stock values use
the price at the transaction's branch (server-side); completed sales keep their recorded price.

**Catalogue branch filter** (several branches): the Current Branch (products sold there, its stock and price), another
of the person's branches, or *All accessible branches* (each product once, stock added up, each branch's price — a
range when they differ). Search, category, price status, bulk selection and paging work with it.

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

## Product Name Appearance (roadmap 204)
Settings → Products → Display preferences → **Product Name Appearance**: *Automatic / Full name* (default — the whole
name, wrapping onto as many lines as the screen needs), *Single line*, *Two lines* or *Three lines*. With a limit, hovering
shows the full name, and tapping a name that is not itself a button reveals it (tapping a product card at the till still
opens or adds the product). One shared component shows names in the till (cards, list, cart and checkout), Edit Sale and
exchanges, sale and credit details (recall, swap, settle with a returned item), orders, transfers, stock (levels,
movements, adjustments, items), stock taking, scanned deliveries, bulk deactivation, the dashboard's products and the
global search; mobile list rows wrap instead of cutting titles. In-stock and out-of-stock products alike; light and dark.
The public website and ordering link keep their own card settings.

## Product page
Price, stock here, stock across branches, photo manager, recent stock movements, details, buttons for
*Performance* (dashboard filtered to the product), *Receive stock*, *Edit*, *Activate/Deactivate*, barcoded items.

## Rules
- Codes are unique per business; a barcode can belong to only one product or one active item.
- Products are never deleted — deactivate them (history stays intact). Inactive products cannot be sold or ordered.
- **Inactive products stay out of the way** (roadmap 200): they are left out of every operational search and list —
  Stock Receiving, stock searches and listings, stock taking, transfers, the till, Edit Sale, orders, the global search,
  the website and the ordering link. Only *Products → Inactive / All* (people with `products.view`) shows them. Active
  products with zero stock stay searchable for replenishment. Scanning an inactive product's barcode never reads as
  unknown and never offers *New product*: it shows *This product is inactive and must be reactivated before use.* and nothing is recorded. Its barcode still cannot be given to
  another product. Past sales, returns, receipts, credit sales, stock movements and audit records are kept, and earlier
  sales of it can still be returned. Activation and deactivation reach every open screen at once (live refresh).
- Approval workflows can gate *creation* (product stays inactive until approved, photos can still be added),
  *edits* (applied on approval) and *deactivation*.

## Permissions
`products.view`, `products.create`, `products.edit`, `products.deactivate`; cost price `sales.view_financials`.

## Price filters and bulk deactivation (roadmap 151)

**Price status** (Products → Catalogue), on the whole catalogue (not only the page shown) and together with Active /
Inactive / All, search, category and paging:

| Filter | Products |
|---|---|
| All prices | every product |
| Price set | a selling price is set, 0 included |
| Missing price | no selling price (empty) |
| Zero price | the selling price is exactly 0 |
| Zero or missing price | 0 or empty |
| Above zero | more than 0 |

**Missing product image** (roadmap 154), the last option of the same list: products with no stored photo holding image data, on the whole catalogue and with the other filters. A photo that fails to load in a browser does not count as missing.

A price of 0 (free) and no price (*Price on Request*) are stored and filtered differently.

**Bulk selection** (people who may deactivate products): a checkbox on every row (and a box for the whole page); the
selection is kept by product across pages and filter changes, and the bar always shows how many are selected —
*X products selected · Deactivate · Clear selection*. With the whole page selected, *Select all N matching* adds
every product matching the filters (up to 500 per bulk action; the bar says when more match).

**Bulk deactivation:** the confirmation lists every selected product as the server sees it — *will be deactivated*,
*already inactive*, *already waiting for approval* — with an optional reason. Each product then goes through the same
path as deactivating it alone, in its own transaction:

- Product deactivation needs no approval → deactivated.
- It needs approval → one request per product, products **stay active** until approved; the approvers are told once
  for the whole submission (*15 requests waiting for approval*); the reason is kept on each request.
- Never two waiting requests for the same product; already inactive / already waiting products are reported, not
  skipped silently; a retry never doubles anything.

The result lists every product's outcome (deactivated · waiting for approval · already inactive · already waiting ·
failed). All requests of one submission share a **bulk reference**; the audit trail has one `products /
bulk_deactivate` entry (counts, items, reason, session branch) besides each product's own entries.

## Manage branch prices (roadmap 186)

Settings → Products → **Manage branch prices** (also Settings → Branch prices) — Tenant Administrators only
(`pages/settings/BranchPrices.tsx`; `GET /api/branch-prices?branch_id=`, `POST /api/branch-prices`).

| Status | Meaning | Effective price |
|---|---|---|
| Default price | no own price at the branch | the product's current default |
| Branch price | the branch's own price | that price |
| Reset to 0 | own price 0 | **0** — never the default, never sold, *Price on request* online, until a price is set |
| No price | no own price and no (or a 0) default | none — set a price |
| Waiting for approval | a change at this branch awaits *Product edit* approval | unchanged until approved |

Actions on one product, the selected ones, a category or the whole branch: **Save prices** (typed in the table; the
switch turns the branch's own price on or off), **Reset … to 0**, **Restore default**. Each is confirmed with the branch
and the number of products, sent as an explicit list with the own price each was seen at (the whole change is refused
if any changed meanwhile), applied in one transaction — or sent as one *Product edit* approval when the business has it
on (applied once approved, no further step) — and audited per product (`branch_price`, before/after, approval) and as
a whole (`branch_prices_bulk`). Stock, products and other branches are never touched. A price of 0 is refused at
the till and by the server (*Price needed*); historical sales keep their prices.
