# HTTP API reference

Base path `/api`. Staff endpoints need `Authorization: Bearer <token>` (from `POST /auth/login`) and accept
`X-Branch-Id: <branch uuid>` to set the Current Branch. Portal endpoints use the portal token from `POST /portal/{slug}/session`.
Errors are `{"error": {"code", "message"}}` with HTTP 400 (bad input), 401, 403, 404, 409 (duplicate), 422 (business rule), 502 (M-Pesa/WhatsApp).
Actions that may need approval return `{ pending_approval, approval_id, result }`.
List endpoints accept `limit` (≤500) and `offset`; period filters accept `period=today|yesterday|week|month|year|last7|last30|all` or `from`/`to` (YYYY-MM-DD).

*Generated from `server/src/routes/*.rs`.*

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
| GET | `/api/platform/tenants` |
| POST | `/api/platform/tenants/{id}/open` |
| GET/POST | `/api/platform/demo` — status / build `{ reset }` |

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
| PUT | `/api/settings` |
| PUT | `/api/settings/profile` |
| POST | `/api/settings/logo` |
| PUT | `/api/settings/workflows/{action}` |
| GET | `/api/public/{slug}/logo` |
| GET | `/api/branches` |
| POST | `/api/branches` |
| PUT | `/api/branches/{id}` |
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
| POST | `/api/products/{id}/photos` |
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
| POST | `/api/transfers/{id}/receive` |
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
| POST | `/api/sales` |
| GET | `/api/sales/{id}` |
| POST | `/api/sales/{id}/return` |
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

## Webhooks (public)

| Method | Path |
|---|---|
| POST | `/api/webhooks/mpesa/{token}` |
| GET | `/api/webhooks/whatsapp` |
| POST | `/api/webhooks/whatsapp` |
