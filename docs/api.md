# HTTP API reference

Base path `/api`. Staff endpoints need `Authorization: Bearer <token>` (from `POST /auth/login`) and accept
`X-Branch-Id: <branch uuid>` to set the Current Branch. Portal endpoints use the portal token from `POST /portal/{slug}/session`.
Errors are `{"error": {"code", "message"}}` with HTTP 400 (bad input), 401, 403, 404, 409 (duplicate), 422 (business rule), 502 (M-Pesa/WhatsApp).
Actions that may need approval return `{ pending_approval, approval_id, result }`.
List endpoints accept `limit` (≤500) and `offset`; period filters accept `period=today|yesterday|week|month|year|last7|last30|all` or `from`/`to` (YYYY-MM-DD).

*Generated from `server/src/routes/*.rs`.*

**Geofencing:** staff requests may carry `X-Location: lat,lng,accuracy_m`; when the business restricts an area to the
branch it is required (422 when missing, imprecise or outside the radius) and it is stored in the audit trail.

## Authentication

| Method | Path |
|---|---|
| POST | `/api/auth/login` |
| PUT | `/api/auth/preferences` — `{ language, font, currency }` |
| GET | `/api/fx` — KES→USD/EUR rates (cached 1 h) |
| GET | `/api/auth/me` |
| POST | `/api/auth/change-pin` |

## Platform (platform admins)
| Method | Path |
|---|---|
| GET | `/api/platform/tenants` — directory with status, admin contacts and billing position |
| GET | `/api/platform/tenants/{id}` — users, branches, onboarding, status history, billing |
| POST | `/api/platform/tenants/{id}/open` |
| POST | `/api/platform/tenants/{id}/status` `{ status: active\|deactivated, reason }` |
| POST | `/api/platform/tenants/{id}/users/{user_id}/reset-pin` — one-time PIN |
| GET | `/api/platform/activity?tenant_id&branch_id&user_id&activity&period\|from&to&limit&offset` |
| GET/POST | `/api/platform/demo` — status / build `{ reset }` |

## Billing ([module 21](modules/21-platform-billing.md))
| Method | Path |
|---|---|
| GET | `/api/billing` — the business's own plan, status, documents, payments, vendor (masked) — `settings.billing` |
| GET | `/api/billing/documents/{id}` — document + payments + parties (PDF) |
| POST | `/api/billing/invoices/{id}/pay` — Paystack checkout for one open invoice |
| POST | `/api/billing/paystack/verify` `{ reference }` — server verifies with Paystack |
| POST | `/api/billing/quotations/{id}/accept` |
| GET | `/api/platform/billing` — dashboard |
| GET/PUT | `/api/platform/billing/vendor` |
| PUT | `/api/platform/tenants/{id}/billing-plan` |
| POST | `/api/platform/tenants/{id}/billing-documents` `{ kind, category: next_period\|one_off\|other, amount?, description?, due_date? }` |
| GET | `/api/platform/billing/documents/{id}` |
| POST | `/api/platform/billing/documents/{id}/void` `{ reason }` · `/invoice` (quotation → invoice) · `/payments` `{ method, reference, paid_at?, note? }` |
| POST | `/api/platform/billing/payments/{id}/verify` |

## Access requests
| Method | Path |
|---|---|
| POST | `/api/access-requests` (public) |
| GET | `/api/platform/access-requests` |
| POST | `/api/platform/access-requests/{id}/approve` |
| POST | `/api/platform/access-requests/{id}/reject` |

## Settings, branches, users & roles

| Method | Path |
|---|---|
| GET | `/api/settings` |
| PUT | `/api/settings` — `workspace: {hours: {days, open, close}, outside_hours: allow\|block}` among the sections |
| PUT | `/api/settings/profile` |
| POST | `/api/settings/logo` |
| PUT | `/api/settings/workflows/{action}` |
| GET | `/api/public/{slug}/logo` |
| GET | `/api/branches` |
| POST | `/api/branches` |
| PUT | `/api/branches/{id}` — `hours`: omit = unchanged, `null` = follow the business, `{days[7], open, close}` = own hours (needs `settings.workspace`); `geofence: {latitude, longitude, radius_m, enabled}` (omit = unchanged; needs `settings.workspace`) |
| GET | `/api/users` |
| POST | `/api/users` |
| PUT | `/api/users/{id}` |
| POST | `/api/users/{id}/reset-pin` |
| GET | `/api/roles` |
| POST | `/api/roles` |
| PUT | `/api/roles/{id}` |
| GET | `/api/permissions` |

## Products, categories, suppliers, photos

| Method | Path |
|---|---|
| GET | `/api/products` |
| POST | `/api/products` |
| GET | `/api/products/lookup` |
| GET | `/api/products/{id}` |
| PUT | `/api/products/{id}` |
| POST | `/api/products/{id}/status` |
| POST | `/api/products/{id}/photos` — multipart `upload_ref` (uuid, makes retries safe) then `file` (JPEG/PNG/WebP ≤ 3 MB) |
| DELETE | `/api/products/{id}/photos/{photo_id}` |
| POST | `/api/products/{id}/photos/{photo_id}/primary` |
| GET | `/api/photos/{id}` |
| GET | `/api/categories` |
| POST | `/api/categories` |
| PUT | `/api/categories/{id}` |
| GET | `/api/suppliers` |
| POST | `/api/suppliers` |
| PUT | `/api/suppliers/{id}` |

## Custom fields (customers & products)

| Method | Path |
|---|---|
| GET | `/api/customer-fields` |
| POST | `/api/customer-fields` |
| PUT | `/api/customer-fields/{id}` |
| GET | `/api/product-fields` |
| POST | `/api/product-fields` |
| PUT | `/api/product-fields/{id}` |

## Stock & inventory

| Method | Path |
|---|---|
| GET | `/api/stock` |
| GET | `/api/stock/availability/{product_id}` |
| POST | `/api/stock/receive` |
| GET | `/api/stock/movements` |
| GET | `/api/stock/items` |
| GET | `/api/stock/barcode/{code}` |
| GET | `/api/stock/adjustments` |
| POST | `/api/stock/adjustments` |
| POST | `/api/stock/count` |
| GET | `/api/stock/position` |

## Transfers

| Method | Path |
|---|---|
| GET | `/api/transfers` |
| POST | `/api/transfers` |
| GET | `/api/transfers/{id}` |
| POST | `/api/transfers/{id}/submit` |
| POST | `/api/transfers/{id}/dispatch` |
| POST | `/api/transfers/{id}/receive` — optional `{lines: [{id, short, damaged}], reason}` (no body = all arrived) |
| POST | `/api/transfers/{id}/cancel` |

## Customers

| Method | Path |
|---|---|
| GET | `/api/customers` |
| POST | `/api/customers` |
| GET | `/api/customers/lookup` |
| GET | `/api/customers/{id}` |
| PUT | `/api/customers/{id}` |

## Loyalty, referrals & awards

| Method | Path |
|---|---|
| GET | `/api/loyalty/overview` |
| GET | `/api/customers/{id}/loyalty` |
| POST | `/api/customers/{id}/redeem` |
| POST | `/api/customers/{id}/points` |
| GET | `/api/referrals` |
| POST | `/api/referrals` |
| POST | `/api/referrals/{id}/deactivate` |
| GET | `/api/awards` |
| POST | `/api/awards` |
| POST | `/api/awards/{id}/close` |
| POST | `/api/awards/message/{customer_id}` |

## Sales / POS

| Method | Path |
|---|---|
| GET | `/api/pos/products` |
| GET | `/api/sales` |
| POST | `/api/sales` — offline sync: `client_ref` + `offline_at` (≤ 72 h, cash-style sales only) |
| GET | `/api/sales/{id}` |
| POST | `/api/sales/check-barcode` — `{product_id, barcode, branch_id?}`: same rules as checkout; refusals are 422 with `error.title` |
| POST | `/api/sales/{id}/return` |
| POST | `/api/sales/{id}/exchange` — `{return_items, items, payment, refund_method, reason, client_ref}` |
| POST | `/api/sales/{id}/cancel` |
| POST | `/api/sales/{id}/share` |

## M-Pesa STK

| Method | Path |
|---|---|
| POST | `/api/mpesa/stk` |
| GET | `/api/mpesa/stk/{id}` |

## Credit sales

| Method | Path |
|---|---|
| GET | `/api/credit` |
| GET | `/api/credit/aging` |
| GET | `/api/credit/{id}` |
| POST | `/api/credit/{id}/payments` |
| POST | `/api/credit/{id}/write-off` |
| POST | `/api/credit/{id}/recall` — `{items: [{sale_item_id, quantity, barcodes}], reason, settle: refund\|credit, refund_method}` |
| POST | `/api/credit/{id}/remind` |

## Orders (staff)

| Method | Path |
|---|---|
| GET | `/api/orders` |
| POST | `/api/orders` |
| GET | `/api/orders/summary` |
| GET | `/api/orders/{id}` |
| POST | `/api/orders/{id}/status` |

## Ordering portal (public)

| Method | Path |
|---|---|
| GET | `/api/portal/{slug}` |
| POST | `/api/portal/{slug}/identify` |
| POST | `/api/portal/{slug}/session` |
| GET | `/api/portal/{slug}/me` |
| GET | `/api/portal/{slug}/catalogue` |
| GET | `/api/portal/{slug}/products/{id}` |
| GET | `/api/portal/{slug}/orders` |
| POST | `/api/portal/{slug}/orders` |
| GET | `/api/portal/track/{token}` |

## Expenses

| Method | Path |
|---|---|
| GET | `/api/expenses` |
| POST | `/api/expenses` |
| GET | `/api/expenses/{id}/attachment` |
| POST | `/api/expenses/{id}/void` |
| GET | `/api/expense-categories` |
| POST | `/api/expense-categories` |
| PUT | `/api/expense-categories/{id}` |

## Approvals

| Method | Path |
|---|---|
| GET | `/api/approvals` |
| POST | `/api/approvals/{id}/approve` |
| POST | `/api/approvals/{id}/reject` |
| POST | `/api/approvals/{id}/withdraw` |

## Dashboard

| Method | Path |
|---|---|
| GET | `/api/dashboard` — `mine=true` for My Dashboard |
| GET | `/api/dashboard/activity` — `branch_id`, `mine` |
| GET | `/api/leaderboards/products` — period, `branch_id`, `category_id`, `metric` (revenue, units, sales, orders, profit, margin), `limit` |
| GET | `/api/leaderboards/staff` — period, `branch_id`, `metric` (revenue, units, transactions, avg_sale, orders, customers, new_customers, discounts, credit), `limit` — needs `staff.view_others` |

## Reports

| Method | Path |
|---|---|
| GET | `/api/reports` |
| GET | `/api/reports/{key}` |

## Notifications & live events

| Method | Path |
|---|---|
| GET | `/api/notifications` |
| POST | `/api/notifications/read-all` |
| POST | `/api/notifications/{id}/read` |
| GET | `/api/events` |

## Audit trail

| Method | Path |
|---|---|
| GET | `/api/audit` |

## Search

| Method | Path |
|---|---|
| GET | `/api/search` |

## Sales ownership & data visibility ([module 25](modules/25-sales-ownership-and-visibility.md))

| Method | Path | Notes |
|---|---|---|
| POST | `/api/sales` (+ `owner_id`) | another owner needs `sales.assign_owner`; eligibility re-checked |
| GET | `/api/sales/owners?branch_id=` | eligible Sale Owners |
| POST | `/api/sales/{id}/owner-change` (`new_owner_id`, `reason`) | `sales.request_owner_change`; returns an approval outcome |
| GET | `/api/sales` | `scope`, `branches`; items carry `owner_id`, `user_name` (owner), `recorded_by_name` |
| GET | `/api/permissions/scopes` | areas and scopes for the role / user editors |
| GET / PUT | `/api/users/{id}/access` (`overrides`, `default_branch_id`) | `users.manage` |

## Website domains & photos (roadmap 74–75)

| Method | Path | Notes |
|---|---|---|
| PUT | `/api/website/domain/primary` (`primary`) | the domain is the main address (S'Shop address forwards) or not; `website.domain` |
| GET | `/api/website/domain` | adds `is_primary`, `awaiting_platform` |
| POST | `/api/products/{id}/photos` (+ optional `thumb` part) | small copy for grids (≤ 400 KB, JPEG / PNG / WebP) |
| GET | `/api/photos/{id}?size=thumb` | the thumbnail, or the original when there is none |
| GET | `/api/site/products` (+ `sort` recommended/newest/name_asc/name_desc/price_asc/price_desc, `stock=in`, `limit`, `offset`) | roadmap 81; `total` = full filtered count; items carry `compare_at` |

## Quick Login PIN ([module 28](modules/28-quick-login-pin.md))

| Method | Path | Notes |
|---|---|---|
| GET | `/api/auth/quick-pin` | available / reason, set, minimum length, trusted devices, `quick_session` |
| PUT / DELETE | `/api/auth/quick-pin` (`current_pin`, `quick_pin`, `device_name`) | full sign-in; returns `device_token` once · switch off (devices revoked) |
| POST | `/api/auth/quick-pin/devices` (`current_pin`, `device_name`) · DELETE `/{id}` · POST `/revoke-all` | trust this device · remove one · sign out everywhere |
| POST | `/api/auth/quick-login` (`device_token`, `pin`) | public, rate-limited; 400 wrong PIN, 403 locked, 422 *Full sign-in needed* |
| POST | `/api/users/{id}/quick-pin/reset` | `users.manage`, full sign-in |
| GET / PUT | `/api/security/quick-pin` (`enabled`, `role_ids`) | business rule (administrators, full sign-in) |
| GET / PUT | `/api/platform/security` | platform rules |

Any endpoint needing a full sign-in answers a Quick session with 422 *Full sign-in needed*.

## Tenants & secure access ([module 27](modules/27-tenants-and-secure-access.md))

| Method | Path | Notes |
|---|---|---|
| GET | `/api/platform/accounts` | platform owner: tenants with totals, status, billing status, next due |
| GET / PUT | `/api/platform/accounts/{id}` | tenant detail (businesses, people, sessions, websites, policies); edit name, notes, primary administrator |
| POST | `/api/platform/accounts/{id}/status` (`status`, `reason`) | every business of the tenant; platform-owned refused |
| POST | `/api/platform/accounts/{id}/businesses` (`name`) | another business; the primary administrator is linked as its administrator |
| POST | `/api/platform/tenants/{id}/account` (`account_id`) | group a business under another tenant |
| POST | `/api/platform/tenants/{id}/support` (`pin`, `reason`, `scope` view/full, `minutes` 15–480) | `{status: "active", token, profile}` or `{status: "requested", id}` |
| POST | `/api/platform/support/{id}/start` (`pin`) · `/end` | start an approved session · end / withdraw (returns the home session) |
| GET | `/api/platform/support?account_id=` | the platform owner's sessions |
| POST | `/api/platform/tenants/{id}/open` | own business only (others: 422 *Support access needed*) |
| GET | `/api/support-access` · PUT `/policy` (`notify` / `approval`) | business administrators |
| POST | `/api/support-access/{id}/approve` · `/deny` · `/revoke` | `users.manage`; never from a support session |
| POST | `/api/auth/switch-business` (`tenant_id`) | same tenant only; the profile lists `businesses` and `linked_from` |
| GET | `/api/users/linkable` · POST `/api/users/link` (`user_id`, `role_id`, `all_branches`, `branch_ids`) | people of the tenant's other businesses |
| GET | `/api/platform/activity?account_id=` | adds the tenant filter and the `support` activity |

## Order alerts (roadmap 69)

| Method | Path | Notes |
|---|---|---|
| GET | `/api/notifications` | adds `new_orders`: orders still *New* in the user's orders scope (0 without `orders.view`) |
| PUT | `/api/auth/preferences` | adds `notify_new_orders` (default on), `in_app_alerts` (on), `sound_alerts` (off) |

## Receipts ([module 26](modules/26-receipts-and-reconciliation.md))

| Method | Path | Notes |
|---|---|---|
| GET | `/api/sales/{id}/receipts` | `sales.print` + visibility; original + adjustment receipts (snapshots), customer mobile / email |
| POST | `/api/receipts/{id}/link` | secure share URL `…/r/{token}` |
| POST | `/api/receipts/{id}/email` (`to`, `pdf` base64) | PDF ≤ 2 MB; sent from the business's name; 20 per user / 10 min; returns `email_status` |
| POST | `/api/sales/{id}/share` | WhatsApp: short message with the secure link; `sent` only when the Business API confirmed |
| GET | `/api/r/{token}` | public, rate-limited; one receipt's snapshot |
| GET | `/api/receipt-assets/{sha256}` | logos as issued (immutable, cached) |
| POST | `/api/sales/{id}/exchange` | when approval is needed: `{pending_approval, approval_id}`, executed on final approval |

## Onboarding & account recovery ([module 24](modules/24-onboarding-and-recovery.md))

| Method | Path | Who |
|---|---|---|
| POST | `/api/access-requests` (+ `estimated_users`) | public |
| GET | `/api/platform/access-requests?status=` (each item: `emails`, `activation`) | platform owner |
| GET | `/api/platform/access-requests/{id}` (login details, activation, email history) | platform owner |
| POST | `/api/platform/access-requests/{id}/approve` → `temporary_pin` (once), `email_status`, `wa_phone`, `message` | platform owner |
| POST | `/api/platform/access-requests/{id}/reject` (`reason` for the applicant, `note` internal) | platform owner |
| POST | `/api/platform/access-requests/{id}/resend-welcome` · `/issue-pin` | platform owner |
| POST | `/api/platform/emails/{id}/retry` | platform owner |
| POST | `/api/auth/forgot` (`email`) — always the same answer | public |
| POST | `/api/auth/link` (`token`) — purpose of a set-up / reset link | public |
| POST | `/api/auth/set-pin` (`token`, `pin`) | public |
| POST | `/api/auth/request-status` · `/api/auth/request-status/resend-setup` (`token`) | public |
| POST | `/api/auth/change-pin` → `token` (a fresh session; other sessions end) | signed in |
| POST | `/api/webhooks/resend` (Svix-signed) | Resend |

## Website Add-On — management ([module 23](modules/23-website.md))

| Method | Path | Permission |
|---|---|---|
| GET | `/api/website` | `settings.integrations` or any `website.*` |
| POST | `/api/website/request` | `settings.integrations` |
| PUT | `/api/website/draft` (`config`, `base_updated_at`) | per changed part |
| POST | `/api/website/publish` · `/api/website/discard` | `website.publish` |
| GET | `/api/website/versions` | any website access |
| POST | `/api/website/versions/{version}/restore` | `website.publish` |
| PUT | `/api/website/prices` (`show_prices`) | `website.products` |
| GET | `/api/website/catalogue` | `website.products` / `categories` / `content` / `view` |
| GET | `/api/website/access` · PUT `/api/website/access/{user_id}` | `users.manage` (read: also `website.view`) |
| GET/POST | `/api/website/media` (multipart: `kind`, `name`, `blurry`, `upload_ref`, `file`, `thumb`) | `website.media` / `website.photos` |
| PATCH/DELETE | `/api/website/media/{id}` | `website.media` / `website.photos` |
| GET/PUT/DELETE | `/api/website/domain` · POST `/api/website/domain/check` | `website.domain` (read: also `website.view`) |
| GET | `/api/website/analytics?period=today\|7d\|30d\|90d\|custom&from&to` | `website.analytics` |
| POST | `/api/platform/tenants/{id}/website` (`activate` \| `decline` \| `disable`, `reason`) | platform owner |
| PUT | `/api/platform/tenants/{id}/website/domain` (`routing_target`) | platform owner |

Website billing uses the billing endpoints with `"service": "website"` (`PUT /platform/tenants/{id}/billing-plan`,
`POST /platform/tenants/{id}/billing-documents`).

## Website — public

On a custom domain the host identifies the business; on the S'Shop host pass `slug`. `preview=true` with a staff token
of the same business reads the draft.

| Method | Path |
|---|---|
| GET | `/api/site` |
| GET | `/api/site/products?q&category&section=featured\|new_arrivals\|popular&limit&suggest` |
| GET | `/api/site/products/{slug}` |
| GET | `/api/site/media/{id}?size=thumb` |
| POST | `/api/site/identify` · `/api/site/session` |
| POST | `/api/site/orders` (customer session) |
| POST | `/api/site/events` (`visit`, `product_view`, `add_to_cart`, `order_start`) |
| GET | `/api/site/whoami` (domain check) |

Pages: `/s/{slug}/…` or `/…` on a verified domain, incl. `/sitemap.xml` and `/robots.txt`.

## Webhooks (public)

| Method | Path |
|---|---|
| POST | `/api/webhooks/mpesa/{token}` |
| POST | `/api/webhooks/paystack` — `x-paystack-signature` (HMAC-SHA512) required |
| GET | `/api/webhooks/whatsapp` |
| POST | `/api/webhooks/whatsapp` |
