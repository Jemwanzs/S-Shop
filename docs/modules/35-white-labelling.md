# 35 — Tenant partial white labelling & business logo (roadmap 112–113)

Brief: *S'Shop — Tenant Partial White Labelling & Business Logo Configuration* (James Sammy, SyncScore). Reuses the
existing business logo (`tenants.logo`, uploaded in Settings → Business, served publicly at
`/api/public/{slug}/logo`), settings, audit, sessions and receipts — no second logo store.

| Phase | Roadmap | Status |
|---|---|---|
| Branding settings, logo management, placement controls, preview, *Business Branding Updated* (Log Out) | 112 | ✅ built |
| Logo applied: login (tenant context), sidebar / header, dashboards, POS header, receipts & documents, customer pages & emails, website sync, favicon / PWA | 113 | ⏳ planned |

## 112 — Settings → Business → Branding & White Labelling

- **Enable business branding** — off by default (S'Shop branding unchanged).
- **Logo**: the current business logo (no second upload) with upload / replace, remove, preview on light and dark
  backgrounds, transparency kept; PNG, WebP, JPEG and SVG with the existing upload validation (size, type sniffing;
  SVG served only as an image with a strict content-type and no scripts). Proportions kept, never stretched; no logo
  → S'Shop branding.
- **Where to display the business logo** (each on / off):
  A login & authentication (on) · B app sidebar & header (on) · C Dashboard & My Dashboard (on) · D POS / Record
  Sale header (on) · E receipts & business documents (on) · F customer orders & notifications (on) · G public website &
  online store (on, kept in sync unless the website has its own logo) · H browser favicon & installed-app icon
  (advanced, off). The platform owner's administration is always S'Shop.
- **Preview branding**: login, sidebar & header, POS, receipt and mobile navigation, light and dark.
- **Save** — confirmed by the server first; then **Business Branding Updated** — *Your business logo has been updated
  successfully. Please log out and sign in again to synchronize your new branding across S'Shop.* — one orange **Log
  Out** button (tooltip *Log out to refresh your business logo across the application.*), no Cancel / Continue; it
  never logs out by itself. On a failed save the actual error shows and no prompt appears.
- Only administrators with business-settings rights change it; the audit trail records the previous and new logo and
  placements, who, when and the tenant.

### As built (112)

- `settings.branding` (`BrandingSettings`, defaults as above) is saved with the other settings; the section needs
  `settings.business` and every change is in the audit trail (before / after).
- `POST /api/settings/logo` accepts PNG, JPEG or WebP **by their bytes** (a file claiming to be an image is refused
  otherwise), up to 2 MB; the app converts SVG (drawn as an image, so scripts never run) and other formats to a 512 px
  WebP first. `DELETE /api/settings/logo` removes it. Both are audited (`logo_updated` with the previous and new
  fingerprint and size, `logo_removed`).
- `tenants.logo_etag` (migration 0035) fingerprints the logo; `/api/public/{slug}/logo` answers with that ETag,
  `Cache-Control: public, no-cache` and `nosniff`, and 304 when unchanged — browsers re-check rather than keep a stale
  logo.
- The placement switches are disabled (not just dimmed) while branding is off. *Business Branding Updated* appears only
  after the server confirms a branding change (or a logo change while branding is on); it has no ×, Escape or swipe —
  only **Log Out** (`ResponsiveDialog required`). A failed save shows the server's error and no prompt.

## 113 — Applying the logo

- **Login**: the shared login page keeps the large S'Shop marketing identity. The small icon above the form / Quick PIN
  pad becomes the business logo only where the business is known from trusted context — a trusted Quick PIN device,
  the business's own login address (`/login/{business}`), or a session that expired. Optionally (separate switch) the
  business's own login address may also replace the large marketing logo. Never chosen from an unverified identifier.
- **Sidebar / mobile header / tablet**, **dashboard brand mark**, **POS header**: the business logo in the existing
  brand position; tenant name, branch selector and **Powered by S'Shop** stay.
- **Receipts & documents** (receipts, credit statements, invoices where supported, order receipts), **customer
  pages & emails** (order confirmation, tracking, payment confirmations): the business logo; content, delivery and
  payment handling unchanged.
- **Website**: follows the business logo unless the website has its own logo (never overridden without asking).
- **Favicon / PWA icon** (advanced): resolved from the verified business context only, never shared across tenants.
- **Caching**: the logo URL carries a version, so a new logo shows after the next sign-in and stale logos never
  linger; no layout shift while loading (reserved size); alt text is the business name.
- **Attribution**: *Powered by S'Shop* stays on the login form footer, the app header / sidebar footer and customer
  pages; receipts are not crowded with repeated branding.
