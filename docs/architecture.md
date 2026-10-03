# Architecture

## Overview

```
 Phones / tablets / desktops ──HTTPS──▶  S'Shop service (one Rust binary on Railway)
   • staff web app  /                      ├─ /api/*            JSON API (Axum)
   • ordering link  /order/<slug>          ├─ /api/events       live events (SSE)
   • tracking       /track/<token>         ├─ /api/webhooks/*   M-Pesa & WhatsApp callbacks
                                           ├─ /assets, /*       built React app (SPA fallback)
                                           └─ background jobs   every 15 min
                                                    │
                                     PostgreSQL 16 (Railway) ◀── migrations run at start-up
 Safaricom Daraja ◀──── STK push / query ───────────┤
 Meta WhatsApp Cloud API ◀── messages / templates ──┘
```

One deployable keeps hosting simple and same-origin (no CORS, one domain, cookies-free bearer tokens).

## Server (`server/`)

| Path | Responsibility |
|---|---|
| `main.rs` | Start-up, migrations, router, static files, CLI (`serve`, `reset-pin`, `import-legacy`) |
| `config.rs` | Environment configuration (M-Pesa / WhatsApp enabled only when fully configured) |
| `auth.rs` | Argon2 PIN hashing, JWT issue/verify, `Ctx` extractor (user, permissions, Current Branch), `PortalCustomer` extractor |
| `perms.rs` | Permission catalogue + default roles + legacy role mapping |
| `settings.rs` | Typed tenant settings with defaults (stored as JSON) |
| `inventory.rs` | **Inventory ledger** — `lock`, `apply` movement, `reserve`, `release` |
| `loyalty.rs` | Points rules, award, referral bonus, reversal, redemption, FIFO expiry |
| `workflow.rs` | Approval engine (`needs_approval`, `submit`, `can_decide`, `approvers`) |
| `audit.rs` | Audit-trail writer (same transaction as the change) |
| `notify.rs` | In-app notifications (+ live push) and WhatsApp fan-out |
| `jobs.rs` | Points expiry, overdue-credit alerts, M-Pesa timeouts, OTP clean-up |
| `legacy.rs` | Pablo Loyalty importer |
| `integrations/` | `mpesa.rs` (Daraja), `whatsapp.rs` (Cloud API) |
| `routes/` | One module per functional area; each exposes `routes()` and, where approvals apply, `on_approved()` |

### Request context

Every staff request carries `Authorization: Bearer <jwt>` and optionally `X-Branch-Id`. The `Ctx` extractor loads the
user's role permissions and branch list **on every request** (deactivation and permission changes apply immediately)
and rejects branches the user is not assigned to. Handlers call `ctx.require("module.action")`.

### Transactions & integrity

All multi-step operations (sale completion, returns, transfers, order completion, approvals) run in **one PostgreSQL
transaction**. Stock rows are locked with `SELECT … FOR UPDATE` before checking availability, so two cashiers cannot
sell the last unit twice. The database enforces:

- unique customer mobile per business, unique product code, unique **active** barcode (partial index),
- non-negative reservations, positive quantities, `returned_qty ≤ quantity`,
- one open award period per business, one active referrer per customer,
- one payment per M-Pesa confirmation code, idempotent POS submissions (`client_ref`).

Financial and operational records are never deleted: sales are cancelled/returned, expenses voided, products and
customers deactivated, referrals deactivated.

## Inventory ledger

```
stock_movements (append-only)            stock_levels (projection, locked per update)
 branch · product · item · kind · qty     branch · product · on_hand · reserved
```

`inventory::apply` locks the level row, checks the rule (`Available` = on hand − reserved, `OnHand`, or `None` for
recounts), inserts the movement and updates `on_hand` — all in the caller's transaction. Movement kinds:
`opening, received, sale, order_completion, transfer_out, transfer_in, customer_return, sale_reversal,
supplier_return, damage, loss, write_off, adjustment, count_variance`.
Individually tracked units live in `stock_items` (`in_stock → reserved/in_transit → sold/written_off/…`).

## Data model (main tables)

| Area | Tables |
|---|---|
| Access | `tenants`, `branches`, `roles`, `users`, `user_branches` |
| Catalogue | `products`, `product_branches`, `product_photos`, `categories`, `suppliers` |
| Inventory | `stock_levels`, `stock_movements`, `stock_items`, `stock_adjustments`, `transfers`, `transfer_items` |
| Sales | `sales`, `sale_items`, `payments`, `mpesa_requests`, `credit_sales`, `sale_returns`, `sale_return_items` |
| Orders | `orders`, `order_items`, `order_events`, `portal_otps` |
| Customers | `customers`, `customer_fields`, `referrals`, `loyalty_ledger`, `award_periods`, `award_winners` |
| Finance | `expenses`, `expense_categories` |
| Control | `workflows`, `approvals`, `audit_log`, `notifications`, `whatsapp_messages`, `doc_counters` |

Document numbers (`RCP-2026-000001`, `ORD-…`, `TRF-…`, `RTN-…`) come from `doc_counters` per business and year.

## Security

- PINs hashed with Argon2id; 5 failed attempts lock an account for 15 minutes; login timing does not reveal emails.
- JWT (HS256) staff sessions last 12 h; portal sessions 30 days and are tenant-bound.
- Permissions and branch access checked server-side on every request; the UI only hides what is not allowed.
- WhatsApp webhooks verified with the Meta app-secret HMAC; M-Pesa callbacks authenticated by a secret URL token.
- Maker-checker: requesters can never approve their own requests.
- Secrets only in environment variables; settings visible in the app never contain credentials.

## Real-time

`GET /api/events?access_token=…` streams `notification`, `order`, `stock`, `sale`, `approval` and `mpesa` events
(ids only). The web app invalidates the matching queries, so screens refresh themselves — e.g. the POS sees an STK
payment confirmation the instant Safaricom calls back. Events are delivered in-process; when scaling beyond one
instance, replace the broadcast channel with PostgreSQL `LISTEN/NOTIFY`.

## Web app (`web/`)

React 18 + TypeScript, routing with React Router, server state with TanStack Query (cached, refetched on focus and on
live events), Tailwind design tokens, Radix primitives (shadcn/ui). Every page is code-split. See [ui.md](ui.md).
