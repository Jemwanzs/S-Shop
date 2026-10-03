# UI design system & responsive layout

S'Shop keeps the warm identity of the original Pablo app (cream surfaces, orange primary, Outfit type, monospaced
figures) and adds the **S'Shop brand gradient** (magenta → orange) only for brand moments.

## Brand assets

| File | Use |
|---|---|
| `web/public/favicon.ico`, `favicon-32.png` | Browser tab icons |
| `web/public/apple-touch-icon.png` | iOS home-screen icon (cream background — iOS fills transparency black) |
| `web/public/icon-192.png`, `icon-512.png` + `manifest.webmanifest` | Installable app (PWA) icons |
| `web/src/assets/sshop-mark.png` | S mark: sidebar/top bar fallback, phone login, “Powered by S'Shop” |
| `web/src/assets/sshop-logo-stacked.png` | Desktop login brand panel |
| `web/src/assets/sshop-logo-horizontal.png` | Horizontal logo (available for receipts/marketing surfaces) |

Icons and favicons were **derived** from the supplied `SShop_Icon_1024x1024.png`: the supplied icon/favicon files contain
a caption band (“Transparent Background (PNG)”) baked into the image, so the mark was cropped below it and re-exported.
The untouched originals are kept locally in `_archive/brand-originals/`. A vector (SVG) master from the designer would
allow sharper large icons.

A business's own logo (Settings → Business profile) replaces the S mark in its staff app and leads its ordering portal.

## Tokens (`web/src/index.css`)

| Token | Light | Use |
|---|---|---|
| `--background` | cream `36 52% 96%` | page |
| `--card` | `36 60% 99%` | surfaces |
| `--primary` | orange `22 88% 48%` | actions, active nav, focus |
| `--points` | amber-orange | loyalty points |
| `--success / --warning / --destructive` | green / amber / red | status |
| `--brand-from → --brand-to` | magenta `336 84% 52%` → orange `22 95% 52%` | `.bg-brand`, `.text-brand` (brand moments only) |
| `--gold / --silver / --bronze` | medals | award tiers |

Dark mode redefines every token (`.dark` on `<html>`, toggled from the top bar; first visit follows the OS).
Fonts: **Outfit** (UI) and **JetBrains Mono** (`.num` — tabular figures that never wrap).

## Layout by screen width

| Width | Navigation | Lists | Dialogs |
|---|---|---|---|
| < 768 (phones) | Top bar + bottom bar **Home · Sales · Stock · Orders · More** | Cards | Bottom sheets |
| 768–1023 (tablets) | Same, wider content | Tables (secondary columns hidden) | Centred dialogs |
| ≥ 1024 (laptops) | Grouped **sidebar** + search bar (Ctrl/⌘ K) | Full tables | Centred dialogs |
| ≥ 1536 / 1920 | Sidebar; grids grow to 4–6 columns; max content width 1680 px | Extra columns appear | — |

Rules: no horizontal page scroll at any width (checked at 390, 820, 1440, 1920 px); touch targets ≥ 44 px; product
photos hidden on operational screens until “View photos”; the ordering portal is image-rich.

## Building blocks (`web/src/components`)

`AppShell` (sidebar, top bar, bottom nav, live events) · `PageHeader`, `Section`, `KV`, `EmptyState` · `DataList`
(table ↔ cards from one column definition) + `Pager` · `ResponsiveDialog` (drawer ↔ dialog) · `PeriodFilter`,
`Segments`, `SearchInput` · `StatCard` · `StatusBadge`, `PointsPill`, `Medal`, `StockIndicator` · `BarcodeScanner`
· `PhotoGallery` · `Field`, `ToggleRow`, `NativeSelect`, `ConfirmDialog` · `ui/*` Radix primitives (shadcn).
