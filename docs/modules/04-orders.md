# 04 · Orders (staff)

**Scope:** §19, §34 · **Routes:** `/orders`, `/orders/:id` · **API:** `/api/orders…`

## Order list
Status tabs with counts: **Active** (default) · New · Confirmed · Preparing · Dispatched · On delivery · Delivered ·
Completed · Cancelled · All; search by order number, customer or mobile. New orders are flagged and arrive live
(notification + list refresh). The page header shows the business ordering link.

**Phone order** button: staff create an order for a customer (mobile + name, products, delivery location, notes).

## Statuses
`New → Confirmed → Preparing → Ready/Dispatched → On delivery → Delivered → Completed`, plus `Cancelled`,
`Rejected` (from New) and `Returned` (set when the resulting sale is cancelled). Steps may be skipped forward.
Every change records user, time and an optional note (timeline on the order page).

## Stock: reserved vs sold
- **Confirming** (or jumping past *New*) **reserves** the items at the order's branch when *Reserve stock* is on:
  available-to-sell drops, physical stock does not.
- **Cancel / reject** releases the reservation.
- At the configured stage (**Delivered** or **Completed**, Settings → Orders) the order **becomes a sale**: the
  reservation is released and a normal sale is recorded (stock cleared, customer totals, loyalty points). Staff choose
  how the customer paid (cash, M-Pesa with code, credit …).

## Order page
Progress tracker, next-step buttons, stock warning if items are short, items with photos and current availability,
customer (call, WhatsApp with tracking link, copy link), delivery location and notes, fulfilment info and receipt link.
With WhatsApp configured the customer is messaged on every status change.

## Permissions
`orders.view`, `orders.manage`.
