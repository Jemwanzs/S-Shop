# 23 — Website Add-On (roadmap 51–57)

An optional, billable website for each business: its own brand, pages and online shop, with S'Shop as the engine
behind it. Products, prices, photos, stock, customers and orders all come from S'Shop, so there is nothing to sync.
The business edits a typed configuration document, never raw HTML or CSS. S'Shop keeps control of the architecture,
responsiveness, accessibility and security.

| Part | Where |
|---|---|
| Configuration document, defaults, validation, image checks | `server/src/website.rs` |
| Management API (draft, publish, media, access, domain, analytics, platform actions) | `server/src/routes/website.rs` |
| Public API, page rendering, SEO, host routing | `server/src/routes/site.rs` |
| Custom domains (DNS checks, Railway API) | `server/src/domains.rs` |
| Database | `migrations/0017_website_service.sql` |
| Public website (separate Vite entry, `site.html`) | `web/src/site/*` |
| Management Centre | `web/src/pages/settings/website/*` (Settings → Control → **Website**) |
| Platform owner card | `web/src/pages/settings/website/PlatformWebsite.tsx` (Businesses → business) |

## 51 — The service: request, activation, billing

- **Locked until activated.** *Settings → Website* shows the add-on as locked 🔒 with *Request Website Service*
  (permission `settings.integrations`). A request goes to the platform owner by email (Resend) and in the app, and
  shows as a *Website requested* pill in the business directory.
- **Platform owner decides:** on the business page, *Activate* (with or without a request), *Decline* (reason
  required) or *Disable* (reason required). Activating creates a starter draft from the business's details. Every
  decision is audited for both the business and the platform, and the business is notified.
- **Billing** reuses the platform billing engine ([module 21](21-platform-billing.md)) as a separate **service**
  (`billing_plans.service` / `billing_documents.service` = `platform | website`). That gives one-off, subscription,
  maintenance, trial, free, grace, tax and fixed / % / 100% discounts, with invoices, Paystack *Pay now*, receipts and
  PDFs. A website plan is always the whole website (no modules). Website revenue has its own line on the platform
  billing dashboard.
- **Suspension and disabling** affect only the public website, which then shows *Temporarily unavailable* (HTTP
  503, `noindex`). POS, stock, orders and everything else keep working. Content, media, products, domain and analytics
  are kept, and re-activating brings the website back exactly as it was.

## 52 — Management Centre

Tabs: **Overview · Content · Design · Products · Categories · Services · Testimonials · Media · Users & Access · Domain ·
SEO · Analytics**. Each tab is shown only to someone with the matching permission.

- **Draft → Preview → Publish.** Edits change a local copy, and *Save draft* stores it with optimistic concurrency
  (`base_updated_at`), so a stale editor cannot overwrite a colleague's newer draft. *Preview* first saves the draft,
  then opens the real website in an iframe at phone, tablet and desktop widths. The preview is read with the staff
  member's own session (`?preview=1`) and carries a preview banner. *Publish* (permission `website.publish`) copies
  the draft live as a new numbered version with an optional note. *Discard* returns the draft to the published
  version. *Version history* lists publications, and *Restore* republishes an earlier one as a new version, so
  nothing is lost.
- **Publishing is refused** while sample testimonials are marked published, or while the configuration has problems.
- **Content:** navigation (show, hide, rename, reorder; *Home* always shows); home sections (hero, categories,
  featured, new arrivals, most popular, offers, about, services, testimonials, call to action, contact), each with
  show, hide, heading, subheading, grid or carousel layout, and reorder; hero (headline, text, banner, overlay,
  alignment, two buttons); offers; About (intro, story, mission, vision, values, image); Contact (phone, WhatsApp,
  email, location, map link, hours, each with show or hide); social links; footer and policies; cookie consent and
  privacy link.
- **Design:** website name, tagline and logo (falls back to the business logo); theme Light, Dark or Both (Light by
  default; *Both* gives visitors a switch); style modern, minimal, elegant or bold; spacing compact, balanced or
  spacious; heading and body font (Outfit, Poppins, Inter, Roboto, Nunito); light and dark palettes.
  - **Readability** is enforced with WCAG contrast. Text needs 4.5:1 on the background and cards, muted text 3:1,
    buttons 3:1 (button text is picked automatically) and the primary colour 2:1. The editor shows each ratio live;
    the server refuses unreadable combinations.
  - **Product cards:** per row on phones 1–3 (default 2), tablets 2–4 and desktops 3–6; card size; photo shape and
    fit; corners; shadow; 1 or 2 name lines; badges; quick add; availability.
  - **Categories:** per row, card size, grid or carousel, images.
- **Buttons and links** can point to a page key (products, about, contact, order …), a product or category path, or
  an `https:`, `tel:` or `mailto:` link. Anything else, `javascript:` included, is refused.
- **Users & Access** (permission `users.manage`): website permissions are granted to existing users individually
  (`users.extra_permissions`). Only the 15 `website.*` permissions can be granted this way, and a database CHECK
  enforces it, so website access never opens sales, stock, finance or settings. Permissions that come from a role
  are shown and locked.

| Permission | Allows |
|---|---|
| `website.view` | Open the centre (read) |
| `website.content` | Sections, hero, offers, about, contact, social, footer, cookies |
| `website.navigation` | Menu |
| `website.design` | Brand, theme, fonts, cards, categories display |
| `website.products` / `website.photos` | Product presentation, prices, photos |
| `website.categories` | Category visibility, order, images |
| `website.services` / `website.testimonials` | Those lists |
| `website.media` | Media library |
| `website.seo` / `website.domain` / `website.analytics` | Those tabs |
| `website.preview` / `website.publish` | Preview / publish, discard, restore |

The server checks which parts of the document changed (`changed_parts`) against these permissions. Hiding a tab in
the interface is not the only guard.

## 53 — Public website

- **Where:** `https://{PUBLIC_URL}/s/{slug}/…`, or the business's own verified domain (56). The server
  (`host_pages`) sends those requests to the website; on a custom domain only the website is served, and `/api` and
  hashed assets pass through.
- **Pages:** home (the sections in the chosen order), `/products` (search and category chips), `/products/{slug}`
  (gallery, availability, quantity, related products), `/categories`, `/categories/{id}`, `/services`, `/about`,
  `/contact`, `/testimonials`, `/order` (cart and checkout), `/track/{token}`. Unknown paths show a not-found page.
- **The server renders SEO:** each page's title, description, canonical URL, Open Graph / Twitter tags and share
  image, plus the business's fonts and icon, are filled in before the page loads. `/sitemap.xml` lists visible pages
  and every published product; `/robots.txt` points to it. Checkout and tracking pages are `noindex`.
- **Ordering** uses the existing engine. The customer gives a mobile number, then a WhatsApp code when verification is
  on, then their first name if they are new; they are the same S'Shop customer as on the ordering link. The order is
  placed with `create_order(…, "website")`, so it lands in **Orders** with source *website* and goes through the
  usual branch, stock reservation, notifications and loyalty. Only published, orderable products are accepted. The
  WhatsApp confirmation links to the website's own tracking page.
- **Experience:** sticky header, mobile menu drawer, search panel with suggestions (thumbnail, category, price), cart
  drawer, a floating cart button on phones, light/dark switch (*Both*), skip link, keyboard focus styles and reduced
  motion.
- **Testimonials** scroll right to left on their own, pause on hover or keyboard focus, run at slow, normal or fast
  speed, and stay still for visitors who prefer reduced motion.
- **Security:** a strict CSP (`script-src 'self'`; frames only from the same origin, for the preview). Published
  content is embedded as a JSON data block, never as script, with `<` escaped.

## 54 — Products & prices

- **Source:** active S'Shop products. Each one can be published or hidden (*Publish new products automatically* is on
  by default), featured, reordered and given a website name, description, badge (new, featured, offer), category
  placement, button text and search title and description.
- **Photos:** the product's own photos (hide individual ones) or a website gallery from the media library, up to 5.
- **Price visibility:** *Show product prices* is the same setting as the ordering link's, applied immediately and
  audited. Each product can inherit it, always show, or hide.
- **When a price is hidden**, the visitor is offered *Enquire* (contact page), *Ask on WhatsApp*, *Call to order* or
  *Order — price confirmed later*. In the last case the order total is withheld and the customer is told the price
  will be confirmed before payment.
- **Hidden prices never leave the server.** That covers product lists, suggestions, product pages, site data, order
  confirmations, order history and tracking. A product hidden per-product on the website is also price-less on the
  ordering link (`portal::hidden_prices`), so the two channels never contradict each other. The smoke suite checks
  every one of these.

## 55 — Media library & image quality

Images are per business: listing, renaming, archiving and deleting only ever touch the business's own rows, and a
draft may only reference its own media and products. The browser resizes each upload to its use (banner up to
2400 px, product 1600 px, …), makes a 480 px thumbnail and estimates sharpness (variance of the Laplacian). The server
then checks the real file:

- **✕ refused:** not JPEG, PNG or WebP; smaller than 200 px on a side; larger than 6000 px; over 8 MB.
- **⚠ warning** (kept, warnings shown): low resolution for its use (banners, offers and about images under 800 px;
  products under 600 px), unusual proportions for product cards, a non-landscape banner, heavy compression, or a
  blurry look.
- **✓ good** otherwise.

Uploads carry an `upload_ref`, so a retried upload is stored once. An image used by the draft, the published version
or any earlier version cannot be deleted or archived.

## 56 — Custom domains

1. **Add** (`website.domain`). Input is normalised (`https://Shop.Example.com/x` → `shop.example.com`; `www` is kept
   as its own host). The platform's own hosts, `*.railway.app` and `localhost` are refused, and one domain belongs to
   one business only (unique index).
2. **Prove ownership** with a TXT record: `_sshop-verify.<domain>` = `sshop-verify=<token>`. It is checked over
   DNS-over-HTTPS. Nothing is attached to S'Shop before this, so nobody can claim a domain they do not control.
   - Records are shown with the **host** to type at the provider, relative to the domain (`_sshop-verify`, `www`, `@`),
     plus the full name for providers that ask for it (roadmap 68).
   - The most common mistake — typing the full name into a provider that adds the domain itself, which creates
     `_sshop-verify.example.com.example.com` — is detected by *Check now*: the record shows *Saved under the wrong name*
     and the message says which Name to use instead.
3. **Attach and route.**
   - With `RAILWAY_API_TOKEN` set, the domain is added to this Railway service automatically (`customDomainCreate`),
     and Railway's routing record (CNAME, or ALIAS / flattened CNAME for a root domain) and its own verification TXT
     are shown to copy.
   - Without the token, the platform owner is emailed and notified, adds the domain in Railway, and records the CNAME
     target on the business page. The business is notified to copy it.
4. **Live:** the domain becomes `active` only when `https://<domain>/api/site/whoami` answers from this server for
   that host, which proves routing and the certificate both work. Only then does the domain decide the business.

| Status | Meaning |
|---|---|
| `dns_required` | Ownership or routing records are missing |
| `verifying` | Ownership confirmed; waiting for the routing target |
| `points_elsewhere` | The domain still points to another host |
| `ssl_pending` | Routing OK; certificate not ready yet |
| `active` | Serving the website |
| `misconfigured` | An active domain stopped reaching S'Shop (no longer served until fixed) |

*Check now* is rate-limited (20 per 10 minutes per user). Removing the domain detaches it from Railway, and the website
stays on its S'Shop address.

## 74 – 77 — Custom domains everywhere, premium storefront

**Audit before building** (owner checklist):

| # | Area | Before | Now |
|---|---|---|---|
| 1 | Mobile product-card responsiveness | 🟡 floating *Add* overlapped name / price | ✅ own action row; compact card at 3 per row |
| 2 | Product image loading & persistence | 🟡 photos persist in PostgreSQL (Railway redeploys never lose them); full-size images in grids; plain icon when missing | ✅ thumbnails, fade-in, branded placeholder, broken-image fallback |
| 3 | Product grid configuration | ✅ phones 1–3 (default 2), tablets 2–4, desktops 3–6 | ✅ |
| 4 | Navigation & search | ✅ Menu · Logo · Search · Cart, sticky header, drawer, suggestions | ✅ |
| 5 | Cart interactions & animations | 🟡 tick on the button only | ✅ badge bump, confirmation with *View cart* |
| 6 | Branding & appearance controls | ✅ colours, fonts, style, spacing, cards, ratios | ✅ + animation intensity |
| 7 | Publishing & live preview | ✅ draft / publish, mobile / tablet / desktop preview | ✅ |
| 8 | Railway custom-domain configuration | 🟡 automatic with `RAILWAY_API_TOKEN`, manual otherwise | ✅ manual set-up shown as *Awaiting platform configuration* |
| 9 | DNS & HTTPS verification | 🟡 records named with the full domain (roadmap 68) | ✅ host names, doubled-name detection, HTTPS check before *Active* |
| 10 | Domain-to-tenant routing | ✅ host → business, unknown hosts never show a website | ✅ + `www.` / bare twin, main-address redirect |
| 11 | Ordering through custom domains | ✅ same orders engine | ✅ + `/orders` |
| 12 | Mobile performance & accessibility | ✅ lazy images, reduced motion respected | ✅ thumbnails, `sizes` |

**Main website address.** When the domain is active, *Main website address* (default on) makes it the address
everywhere:
- canonical and social links and the sitemap use it;
- `/s/{slug}` on the S'Shop host forwards to the same path on the domain. The redirect is temporary so the choice can be
  undone, and `?preview=1` drafts stay on the S'Shop host.
Turned off, the domain shows the same website while links keep the S'Shop address. `www.example.com` and
`example.com` forward to whichever of the two was connected, provided both reach S'Shop (each host must be added to the
service on Railway).

**Railway.** Adding a domain row in PostgreSQL does not configure Railway networking:
- With `RAILWAY_API_TOKEN` set, S'Shop attaches the domain to the service and shows Railway's routing record and
  `_railway-verify` TXT.
- Without it, the platform owner adds the custom domain on the Railway service and records the CNAME target, and the
  business sees *Awaiting platform configuration* until then.
- The domain becomes *Active* only after `https://<domain>/api/site/whoami` answers from this server for that host.

**Product cards** (`web/src/site/parts.tsx`, `site.css`): image frame (configurable ratio and fit), name (1–2 lines),
availability dot, then the action row (price + *Add* pill). Rows line up; nothing overlaps. With three per row on a
phone the card switches to smaller type and an icon-only *Add* under the price.

**Photos.**
- Thumbnails: 480 px, made in the browser at upload (as website media are), stored with the photo and served with
  `?size=thumb`.
- Images have responsive `sizes`, load lazily and fade in.
- A product with no photo, or with a photo that fails to load, shows its initial on the brand colours. Real product
  photos are never replaced.

**Animations** (`theme.motion`): `off` | `subtle` (default) | `standard`.
- Effects: page fade, sections fading in as they scroll into view, card hover shadow (*standard*: lift), button press,
  cart badge bump, and an *Added to cart · View cart* confirmation.
- CSS and one IntersectionObserver only, with no animation library.
- `prefers-reduced-motion` always wins.

## 78 — Railway automatic domain attachment

With `RAILWAY_API_TOKEN` (account or team token) on the service — Railway injects `RAILWAY_PROJECT_ID`,
`RAILWAY_ENVIRONMENT_ID` and `RAILWAY_SERVICE_ID` itself — *Test domain connection* does, in order:

1. **Ownership** — the `_sshop-verify` TXT (nothing is attached before this, so nobody claims a domain they do not hold).
2. **Attach** — `domains(projectId, environmentId, serviceId)` first: an existing attachment on this service is reused;
   otherwise `customDomainCreate`. Audited (`website.domain_attached`). One domain belongs to one business (database
   unique index), so a second business can never attach it.
3. **Railway's records** — `customDomain(id).status`: each `dnsRecords` entry with its `recordType`, `hostlabel`
   (`@` for the root), `purpose` (`TRAFFIC_ROUTE` = routing, `ACME_DNS01_CHALLENGE` = certificate) and whether it has
   `DNS_RECORD_STATUS_PROPAGATED`; Railway's `_railway-verify` TXT until `verified`; the certificate state
   (`…_VALID`, `…_ISSUING`, `…_VALIDATING_OWNERSHIP`, `…_ISSUE_FAILED` with Railway's message).
4. **Active** — only when `https://<domain>/api/site/whoami` answers from S'Shop for that host (DNS, certificate and
   routing proven together).

Removing a domain (or connecting another) detaches it from Railway (`customDomainDelete`). The token stays on the
server: it is read from the environment, never stored, logged or sent to a browser. Without the token the platform
owner adds the domain in Railway and records the target (*Awaiting platform configuration*).

## 80 – 82 — Portrait cards, listings, landing page

- **Cards** (`site/parts.tsx`, one component for products, categories, search, featured, new arrivals and related):
  3:4 frame, `contain` with 6 % padding on a neutral frame by default; badges New / Featured / Offer (set per product),
  −X % (from the product's *Was price* on the website, only when it is above a visible price), Out of stock (only when
  availability is shown); hover (fine pointers): image zoom 4 %, the second photo, lift, border and *Add* emphasis;
  touch: press feedback. Motion setting and reduced motion apply.
- **Listings** (`ProductsPage`): `GET /api/site/products?sort=&stock=in&limit=&offset=` — sorting and filtering on the
  server from real records; `total` always the full filtered count; pages from `products.per_page`; *Load more* when
  `products.pagination` is off. Home sections (featured, new arrivals, popular) keep their carousels / grids.
- **Ordering link** (`/order/{slug}`): portrait thumbnails, branded placeholder, sort, pages of 12.
- **Landing page** (`landing`): what `/` shows — `products` (default) | `home` | `categories` | `services`; Home is at
  `/home` otherwise. Same on S'Shop addresses and custom domains; the page title follows.

## 57 — Analytics

Events: `visit`, `product_view`, `add_to_cart`, `order_start` from the browser, and `order_complete` from the server
only (visitors cannot fake it). Visitors are anonymous: a random id per browser tab, or per browser after consent when
the business turns on *Ask visitors for analytics consent* (visitors who decline are not counted).

`GET /website/analytics?period=today|7d|30d|90d|custom&from&to` (permission `website.analytics`, business time zone)
returns:

- visitors, visits, product views, add-to-carts and order starts;
- orders and completed orders, counted from **Orders** (`source = 'website'`) so they always reconcile;
- conversion (orders ÷ visitors) and completion;
- visitors and orders per day, and the 10 most viewed products;
- completed sales value, but only to people with `reports.view`.

## Tests

- **Unit tests** (`cargo test`): validation and contrast, link safety, image dimensions and grading, permissions per
  part, domain normalisation and routing records.
- **Smoke suite**: sections *Roadmap 51–52* (service, request, activation, draft concurrency, sample testimonials,
  publish, discard, rollback, shared price setting, website-only permissions, separate billing, disable / re-activate,
  isolation) and *Roadmap 53–57* (public site, hidden prices everywhere, per-product overrides, ordering into Orders,
  events and analytics, media grading and isolation, domain guards and uniqueness).
