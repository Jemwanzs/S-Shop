# 31 — Product User Manual (roadmap 88)

A downloadable PDF of S'Shop's features, one page per module. It is linked as **Manual ↓** under the sign-in form
(and under the Quick PIN keypad). No sign-in is needed.

| Part | Where |
|---|---|
| Content (title, intro, version, modules) | `docs/manual/manual.json` |
| Generator (HTML → A4 PDF with Chromium) | `scripts/manual/build.mjs` |
| Published file | `web/public/SShop_Product_User_Manual.pdf` → `https://s-shop.store/SShop_Product_User_Manual.pdf` |
| Link | `web/src/pages/auth/Login.tsx` (`ManualLink`) |

## The document

- **Format**: A4 portrait, Outfit, S'Shop colours (cream pages, warm brown text, soft orange accents, white cards).
  Every page has a page number and a small S'Shop footer. No screenshots.
- **Page 1**:
  - the title *S'Shop — Smart Retail & Business Management*;
  - the subtitle *Product Features & Capabilities Manual*;
  - a short introduction;
  - the numbered module directory with page numbers;
  - the version.
- **One page per module**: the module name, a one-line description and 5–8 feature bullets.
- **Accuracy**: only features that exist today are listed. Platform-owner functions are on their own page, marked
  *Platform owner only*. Every other page describes what a business sees.

## Updating it

1. Edit `docs/manual/manual.json` when a module ships or changes, keeping 5–8 bullets per module and bumping
   `version`.
2. Run `node scripts/manual/build.mjs`. It needs the `playwright` package and Chromium; point `PLAYWRIGHT_DIR` at a
   folder whose `node_modules` holds it, and allow internet access for the Outfit font. The build fails rather than
   fall back to another font. Set `MANUAL_HTML=<file>` to keep the HTML for a page-by-page preview.
3. Commit the JSON and the PDF together. The web build copies the PDF to the site root.

## Delivery

- The server serves the PDF from the web build like any static file (`Content-Type: application/pdf`); the smoke
  suite checks it.
- The service worker never stores a directly opened file as the offline app page. Only HTML responses become the
  shell, a fix made with this feature.
