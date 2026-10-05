# 20 · Platform owner: businesses & the Pablo Niche demo

**Scope:** owner requests 2026-10-05 · **Routes:** Settings → Platform → Businesses · **API:** `/api/platform/tenants…`,
`/api/platform/demo` · **CLI:** `sshop seed-demo [--reset]`

## Branding
The business's own logo and name stay top-left. *Powered by S'Shop* (mark + wordmark) sits quietly top-right from
laptop width; on phones it is shown at the bottom of the More page instead of taking header space.

## Businesses (platform admins only)
Platform admins (`PLATFORM_ADMIN_EMAILS`, holding the Tenant Administrator role) see every business on the
installation with branches, users, sales and last sale. **Open business** gives a session inside that business with full
access; the token records the admin's home business and every request re-checks platform-admin status, so removing the
email from `PLATFORM_ADMIN_EMAILS` ends such sessions immediately. Opening is written to the audit trail of both
businesses. While inside another business a dark banner shows *Viewing … as platform owner · Return to …*; switching
clears all cached data and the branch choice so nothing carries across businesses.

## Pablo Niche demo business
A separate business, **Pablo Niche (Demo)** (`/pablo-niche-demo`, flagged `is_demo`, DEMO badge), never mixed with real
data. Build or reset it from Settings → Platform → Businesses (runs in the background with progress) or with
`sshop seed-demo [--reset]`.

**How it is built:** through the application's own HTTP API, called in-process — the same validation, permissions,
inventory ledger, loyalty, credit, order, transfer, approval and audit rules as real use. Each call's new rows are then
moved to the event's date, which spreads ~5 months of history. Analytics are therefore computed from real
transactions; nothing is hard-coded.

**Contents (deterministic):** 4 branches (Main Branch – CBD, Westlands, Village Market, Two Rivers) with managers and
salespeople · 19 products in Watches, Necklaces and Perfumes at premium KES prices with cost, max discount, supplier,
low-stock levels and one inactive line · uneven stock per branch (high, low, out, only-elsewhere, recently received,
transferred) · individually tracked watches/necklaces with unit labels · ~400 sales across today, yesterday, this
week/month and earlier months, at marked price, discounted (within limits) and slightly above, paid by M-Pesa
(simulated `DEMO…` references), cash and credit (some with deposits) · 48 customers with repeat buyers, referrals,
redemptions, Gold/Silver/Bronze tiers and a closed award round with winners plus the current one · credit sales
outstanding, partially paid, paid and overdue with payment history · returns · orders at every stage (new → completed,
cancelled) · transfers received, in transit and pending approval · ~100 expenses (rent, salaries, utilities, delivery,
packaging, supplies, transport, marketing) · a pending expense approval · a recent stock take with variances.

**Safety:** only a business flagged `is_demo` is ever touched; a rerun without reset changes nothing; reset deletes
only that business, in one transaction (all or nothing). Demo staff get random PINs — nobody can sign in as them;
platform admins open the business instead. WhatsApp is never sent for a demo business. Demo emails use the reserved
`.invalid` domain and phone numbers are 0700 000 xxx placeholders. Product barcodes are EAN-13 in the GS1
restricted-circulation range (prefix 2) and unit labels are `PN-…` — never real product codes.

**Photos:** set `PEXELS_API_KEY` (free key from pexels.com/api) and reset the demo: up to three photos per product are
downloaded from Pexels searches (watches, necklaces, perfume bottles), stored in S'Shop's own photo storage (no
hotlinking) with the source page and photographer credit (`product_photos.source`, `.attribution`). Without a key the
products are created without photos and the build report says so.

**Keep it growing:** when a module is finished, extend `server/src/demo.rs` with examples for it.
