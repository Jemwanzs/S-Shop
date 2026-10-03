# HTTP API reference

Base path `/api`. Staff endpoints need `Authorization: Bearer <token>` (from `POST /auth/login`) and accept
`X-Branch-Id: <branch uuid>` to set the Current Branch. Portal endpoints use the portal token from `POST /portal/{slug}/session`.
Errors are `{"error": {"code", "message"}}` with HTTP 400 (bad input), 401, 403, 404, 409 (duplicate), 422 (business rule), 502 (M-Pesa/WhatsApp).
Actions that may need approval return `{ pending_approval, approval_id, result }`.
List endpoints accept `limit` (≤500) and `offset`; period filters accept `period=today|yesterday|week|month|year|last7|last30|all` or `from`/`to` (YYYY-MM-DD).

*Generated from `server/src/routes/*.rs`.*

## Authentication

| Method | Path | Handler |
|---|---|---|
| POST | `/api/auth/login` | `auth::login` |
| GET | `/api/auth/me` | `auth::me` |
| POST | `/api/auth/change-pin` | `auth::change_pin` |

## Settings, branches, users & roles

| Method | Path | Handler |
|---|---|---|
| GET | `/api/settings` | `admin::get_settings` |
| PUT | `/api/settings` | `admin::put_settings` |
| PUT | `/api/settings/profile` | `admin::put_profile` |
| POST | `/api/settings/logo` | `admin::upload_logo` |
| PUT | `/api/settings/workflows/{action}` | `admin::put_workflow` |
| GET | `/api/public/{slug}/logo` | `admin::logo` |
| GET | `/api/branches` | `admin::list_branches` |
| POST | `/api/branches` | `admin::create_branch` |
| PUT | `/api/branches/{id}` | `admin::update_branch` |
| GET | `/api/users` | `admin::list_users` |
| POST | `/api/users` | `admin::create_user` |
| PUT | `/api/users/{id}` | `admin::update_user` |
| POST | `/api/users/{id}/reset-pin` | `admin::reset_user_pin` |
| GET | `/api/roles` | `admin::list_roles` |
| POST | `/api/roles` | `admin::create_role` |
| PUT | `/api/roles/{id}` | `admin::update_role` |
| GET | `/api/permissions` | `admin::permission_catalogue` |

## Products, categories, suppliers, photos

| Method | Path | Handler |
|---|---|---|
| GET | `/api/products` | `catalog::list` |
| POST | `/api/products` | `catalog::create` |
| GET | `/api/products/lookup` | `catalog::lookup` |
| GET | `/api/products/{id}` | `catalog::detail` |
| PUT | `/api/products/{id}` | `catalog::update` |
| POST | `/api/products/{id}/status` | `catalog::set_status` |
| POST | `/api/products/{id}/photos` | `catalog::upload_photo` |
| DELETE | `/api/products/{id}/photos/{photo_id}` | `catalog::delete_photo` |
| POST | `/api/products/{id}/photos/{photo_id}/primary` | `catalog::set_primary` |
| GET | `/api/photos/{id}` | `catalog::photo` |
| GET | `/api/categories` | `catalog::list_categories` |
| POST | `/api/categories` | `catalog::create_category` |
| PUT | `/api/categories/{id}` | `catalog::update_category` |
| GET | `/api/suppliers` | `catalog::list_suppliers` |
| POST | `/api/suppliers` | `catalog::create_supplier` |
| PUT | `/api/suppliers/{id}` | `catalog::update_supplier` |

## Stock & inventory

| Method | Path | Handler |
|---|---|---|
| GET | `/api/stock` | `stock::levels` |
| GET | `/api/stock/availability/{product_id}` | `stock::availability` |
| POST | `/api/stock/receive` | `stock::receive` |
| GET | `/api/stock/movements` | `stock::movements` |
| GET | `/api/stock/items` | `stock::items` |
| GET | `/api/stock/barcode/{code}` | `stock::barcode_history` |
| GET | `/api/stock/adjustments` | `stock::list_adjustments` |
| POST | `/api/stock/adjustments` | `stock::create_adjustment` |
| POST | `/api/stock/count` | `stock::stock_count` |
| GET | `/api/stock/position` | `stock::position` |

## Transfers

| Method | Path | Handler |
|---|---|---|
| GET | `/api/transfers` | `transfers::list` |
| POST | `/api/transfers` | `transfers::create` |
| GET | `/api/transfers/{id}` | `transfers::detail` |
| POST | `/api/transfers/{id}/submit` | `transfers::submit` |
| POST | `/api/transfers/{id}/dispatch` | `transfers::dispatch` |
| POST | `/api/transfers/{id}/receive` | `transfers::receive` |
| POST | `/api/transfers/{id}/cancel` | `transfers::cancel` |

## Customers & custom fields

| Method | Path | Handler |
|---|---|---|
| GET | `/api/customers` | `customers::list` |
| POST | `/api/customers` | `customers::create` |
| GET | `/api/customers/lookup` | `customers::lookup` |
| GET | `/api/customers/{id}` | `customers::profile` |
| PUT | `/api/customers/{id}` | `customers::update` |
| GET | `/api/customer-fields` | `customers::list_fields` |
| POST | `/api/customer-fields` | `customers::create_field` |
| PUT | `/api/customer-fields/{id}` | `customers::update_field` |

## Loyalty, referrals & awards

| Method | Path | Handler |
|---|---|---|
| GET | `/api/loyalty/overview` | `loyalty::overview` |
| GET | `/api/customers/{id}/loyalty` | `loyalty::ledger` |
| POST | `/api/customers/{id}/redeem` | `loyalty::redeem` |
| POST | `/api/customers/{id}/points` | `loyalty::adjust` |
| GET | `/api/referrals` | `loyalty::list_referrals` |
| POST | `/api/referrals` | `loyalty::create_referral` |
| POST | `/api/referrals/{id}/deactivate` | `loyalty::deactivate_referral` |
| GET | `/api/awards` | `loyalty::list_awards` |
| POST | `/api/awards` | `loyalty::open_period` |
| POST | `/api/awards/{id}/close` | `loyalty::close_period` |
| POST | `/api/awards/message/{customer_id}` | `loyalty::message_customer` |

## Sales / POS

| Method | Path | Handler |
|---|---|---|
| GET | `/api/pos/products` | `sales::pos_products` |
| GET | `/api/sales` | `sales::list` |
| POST | `/api/sales` | `sales::create` |
| GET | `/api/sales/{id}` | `sales::detail` |
| POST | `/api/sales/{id}/return` | `sales::return_items` |
| POST | `/api/sales/{id}/cancel` | `sales::cancel` |
| POST | `/api/sales/{id}/share` | `sales::share` |

## M-Pesa STK

| Method | Path | Handler |
|---|---|---|
| POST | `/api/mpesa/stk` | `payments::push` |
| GET | `/api/mpesa/stk/{id}` | `payments::status` |

## Credit sales

| Method | Path | Handler |
|---|---|---|
| GET | `/api/credit` | `credit::list` |
| GET | `/api/credit/aging` | `credit::aging` |
| GET | `/api/credit/{id}` | `credit::detail` |
| POST | `/api/credit/{id}/payments` | `credit::repay` |
| POST | `/api/credit/{id}/write-off` | `credit::write_off` |
| POST | `/api/credit/{id}/remind` | `credit::remind` |

## Orders (staff)

| Method | Path | Handler |
|---|---|---|
| GET | `/api/orders` | `orders::list` |
| POST | `/api/orders` | `orders::create` |
| GET | `/api/orders/summary` | `orders::summary` |
| GET | `/api/orders/{id}` | `orders::detail` |
| POST | `/api/orders/{id}/status` | `orders::change_status` |

## Ordering portal (public)

| Method | Path | Handler |
|---|---|---|
| GET | `/api/portal/{slug}` | `portal::business` |
| POST | `/api/portal/{slug}/identify` | `portal::identify` |
| POST | `/api/portal/{slug}/session` | `portal::session` |
| GET | `/api/portal/{slug}/me` | `portal::me` |
| GET | `/api/portal/{slug}/catalogue` | `portal::catalogue` |
| GET | `/api/portal/{slug}/products/{id}` | `portal::product` |
| GET | `/api/portal/{slug}/orders` | `portal::my_orders` |
| POST | `/api/portal/{slug}/orders` | `portal::place_order` |
| GET | `/api/portal/track/{token}` | `portal::track` |

## Expenses

| Method | Path | Handler |
|---|---|---|
| GET | `/api/expenses` | `expenses::list` |
| POST | `/api/expenses` | `expenses::create` |
| GET | `/api/expenses/{id}/attachment` | `expenses::attachment` |
| POST | `/api/expenses/{id}/void` | `expenses::void` |
| GET | `/api/expense-categories` | `expenses::list_categories` |
| POST | `/api/expense-categories` | `expenses::create_category` |
| PUT | `/api/expense-categories/{id}` | `expenses::update_category` |

## Approvals

| Method | Path | Handler |
|---|---|---|
| GET | `/api/approvals` | `approvals::list` |
| POST | `/api/approvals/{id}/approve` | `approvals::approve` |
| POST | `/api/approvals/{id}/reject` | `approvals::reject` |
| POST | `/api/approvals/{id}/withdraw` | `approvals::withdraw` |

## Dashboard

| Method | Path | Handler |
|---|---|---|
| GET | `/api/dashboard` | `dashboard::dashboard` |

## Reports

| Method | Path | Handler |
|---|---|---|
| GET | `/api/reports` | `reports::catalogue` |
| GET | `/api/reports/{key}` | `reports::run` |

## Notifications & live events

| Method | Path | Handler |
|---|---|---|
| GET | `/api/notifications` | `notifications::list` |
| POST | `/api/notifications/read-all` | `notifications::read_all` |
| POST | `/api/notifications/{id}/read` | `notifications::read_one` |
| GET | `/api/events` | `notifications::stream` |

## Audit trail

| Method | Path | Handler |
|---|---|---|
| GET | `/api/audit` | `audit::list` |

## Search

| Method | Path | Handler |
|---|---|---|
| GET | `/api/search` | `search::search` |

## Webhooks (public)

| Method | Path | Handler |
|---|---|---|
| POST | `/api/webhooks/mpesa/{token}` | `webhooks::mpesa_callback` |
| GET | `/api/webhooks/whatsapp` | `webhooks::whatsapp_verify` |
| POST | `/api/webhooks/whatsapp` | `webhooks::whatsapp_event` |
