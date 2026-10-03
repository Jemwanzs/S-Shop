# 09 · Customers

**Scope:** §11 · **Routes:** `/customers`, `/customers/:id` · **API:** `/api/customers…`, `/api/customer-fields`

## Customer book
Search by name, nickname or mobile (any part of the number). Sort by top spend, points, most recent, A–Z.
Columns: name (+ tier medal), masked mobile, nickname, total spend, purchases, last purchase, points (own|referral),
tier, amount owed. Loyalty and credit figures are shown only with `customers.view_loyalty` / `customers.view_credit`.

## Fields
Default: **Mobile** (required, unique per business, normalised to 2547…/2541…), **First name** (required), other names,
**Nickname** (optional), email. **Custom fields** (Settings → Customers): name, type (text, number, date, dropdown,
yes/no, email), required/optional, active/inactive, display order. Validated on save.

Customers are created from this page, automatically at checkout (by mobile + first name), from phone orders and from
the ordering portal.

## Customer profile
Stats (total spend, purchases, points available + value, own | referral, redeemed · expired, amount owed) and tabs:
Purchases · Orders · Credit · Points history (every ledger entry) · Referrals (referred by, people referred, bonus
earned, *Record a referral*). Actions: Call, WhatsApp loyalty message, Edit, **Redeem points**, **Adjust points**.

## Rules
Customers are deactivated, not deleted. Tier is recalculated from total spend after every sale or return.

## Permissions
`customers.view`, `customers.create`, `customers.edit`, `customers.view_loyalty`, `customers.redeem_points`,
`customers.view_credit`, `loyalty.manage`.
