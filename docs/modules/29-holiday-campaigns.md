# 29 — Holiday & promotional campaigns (roadmap 84–86)

Seasonal greetings, occasion designs and featured products on a business's website. **Off by default**: the platform
offers the feature, and each business switches it on (Website → **Holiday & promotions**).

| Part | Where |
|---|---|
| Campaigns, schedule, lifecycle, insights, platform rules | `server/src/routes/campaigns.rs` |
| Schema | `migrations/0024_website_campaigns.sql` — `website_campaigns`, `websites.campaigns_enabled`, campaign events on `website_events`, `platform_settings['campaigns']` |
| Public banner | `web/src/site/campaign.tsx`, `site.css` (*Roadmap 84*), `site_data.campaign` (`routes/site.rs`) |
| Management | Website → Holiday & promotions (`web/src/pages/settings/website/CampaignsTab.tsx`); Platform → Security & features → *Website campaigns* |

## 84 — Campaigns

- **Occasions** (each with a ready-written greeting the business can edit):
  - Christmas, New Year, Valentine's Day, Easter, Eid, Diwali, Mother's Day, Father's Day;
  - Black Friday, Cyber Monday;
  - national holiday, business anniversary, customer appreciation, back to school, custom.
- **Greeting**:
  - headline (90), message (260) and an optional promotional line (120). Write only offers you actually make; the
    system never invents discounts;
  - button text and destination (a website page);
  - alignment and headline size.
- **Design**:
  - templates: Christmas, New Year, Hearts, Pastel, Crescent & lanterns, Celebration, Sale, Elegant gold, Minimal;
  - background and text colours (#RRGGBB, empty = the template's own);
  - an optional background image from the media library (darkened for readability, and protected from deletion while
    a non-archived campaign uses it);
  - decorations (CSS motifs, no images or libraries);
  - gentle animation (always still for reduced motion and when the website's animations are off);
  - height (compact / standard / tall) and button style (solid / outline).
- **Featured products** (up to 5 by default, at most the platform limit):
  - **Best sellers** — units actually sold (returns deducted, cancelled sales excluded) over 30 / 90 / 365 days or all
    time, optionally within one category. An empty period falls back to all time; with no sales at all the banner shows
    the greeting only.
  - **Manual** — products chosen from the catalogue.
  - **None**.
  - Only products the website publishes appear, with the website's prices, *was* prices, stock and ordering rules.
  - *Add* uses the normal cart and orders engine.
- **Placement**:
  - pages: Home, Products (including the Products landing page), Categories, All pages;
  - display: **Hero** (default; greeting beside the product row on desktop, stacked on phones), **Compact**, **Top
    strip** or **Floating card**;
  - *Visitors can close it*, remembered per campaign version (a changed campaign shows again).

## 85 — Schedule, lifecycle, preview

- **Status is computed on every request**, so publishing and expiry happen on time without a background job:

  | Status | Meaning |
  |---|---|
  | Draft | Not published |
  | Scheduled | Published, before its start |
  | Live | Published, within its window |
  | Expired | Published, past its end |
  | Archived | Kept with its results |

  Times are entered in the **business's timezone**. A campaign runs for at most a year.
- **One campaign at a time** on the website: the highest priority, then the latest start. Both switches must be on
  (platform and business).
- **Actions**:
  - Save draft and Save & preview — the existing mobile / tablet / desktop preview with `?campaign=`, for any status;
  - Publish, Unpublish, Duplicate (a copy whose dates move forward if the original's have passed), Archive;
  - Delete, for drafts that were never published.
  - Editing is version-checked, so a stale editor cannot overwrite a newer save.
- **Permissions**:
  - `website.content` creates and edits drafts;
  - `website.publish` switches the feature on or off, publishes, unpublishes, archives and edits published campaigns;
  - insights are visible with `website.analytics` too.
  - Every action is audited (`campaign_created` / `updated` / `duplicated` / `published` / `unpublished` /
    `archived` / `deleted`, `campaigns_enabled`).

## 86 — Insights

Insights come from the website's anonymous analytics, with the same consent rules as all website analytics:

| Measure | Rule |
|---|---|
| Views / visitors | The banner shown on a page (once per page per visit) / distinct visitors |
| Product clicks / button clicks | Clicks on a featured product (or its *Add*) / on the button |
| Orders / sales attributed | Website orders by a visitor who clicked a featured product or the button of this campaign within the 7 days before ordering; cancelled orders excluded |

Campaigns are listed side by side with these figures for comparison.

## Platform

Platform → **Security & features** → *Website campaigns*:
- the feature on or off (off hides every campaign at once and stops businesses switching it on);
- campaigns per business (1–100, default 20);
- featured products per campaign (1–24, default 12).

The platform's rules take precedence over the business's. The Website Add-On itself is already a paid, entitled
service.
