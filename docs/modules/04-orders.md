# 04 · Orders (staff)

**Scope:** §19, §34 · **Routes:** `/orders`, `/orders/:id` · **API:** `/api/orders…`

## Order list
Status tabs with counts: **Active** (default) · New · Confirmed · Preparing · Dispatched · On delivery · Delivered ·
Completed · Cancelled · All; search by order number, customer or mobile. New orders are flagged and arrive live
(notification + list refresh). The page header shows the business ordering link.

## New-order alerts & the Orders badge (roadmap 69)
- **Who is notified:** active users with `orders.manage` at the order's branch whose *orders* data scope reaches it
  (an *own records* scope never sees customers' orders, so it is not alerted either), unless they switched off
  *New order notifications* (User preferences → Notifications). One notification per order and person (de-duplicated),
  e.g. *New customer order ORD-2026-000011 — Customer: James · Main Branch · 4 items · KSh 697,500*, opening the order.
- **Orders badge:** an orange counter beside *Orders* (sidebar and phone bar) = orders still **New** in the user's
  orders scope (same filter as the list), `99+` above 99, hidden at zero. It is operational, not a read counter:
  reading or dismissing notifications never changes it; confirming (or cancelling) the order does.
- **Live:** order events refresh the list, the bell and the badge for every signed-in user (SSE), with the regular
  2-minute refresh as a fallback.
- **Preferences:** new-order notifications (on), in-app pop-up alerts (on), sound — a short chime with new-order
  alerts (off). Preferences never hide the badge.

**Phone order** button: staff create an order for a customer (mobile + name, products, delivery location, notes).

## Statuses
`New → Confirmed → Preparing → Ready/Dispatched → On delivery → Delivered → Completed`, plus `Cancelled`,
`Rejected` (from New) and `Returned` (set when the resulting sale is cancelled). Steps may be skipped forward.

**Configurable statuses** (Settings → Orders → Order statuses): every status can be renamed — staff screens, the
customer tracker and WhatsApp messages use your names. The optional steps *Preparing*, *Dispatched*, *On delivery* and
*Completed* can be switched off (e.g. a shop without delivery); disabled steps disappear from the next-step buttons and
the customer tracker. *New*, *Confirmed*, *Delivered*, the terminal statuses and whichever stage turns the order into a
sale are always on.

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
