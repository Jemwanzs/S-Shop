# 10 · Loyalty & rewards

**Scope:** §12, §21, §24, §39 · **Route:** `/loyalty` (Overview · Referrals · Awards) · **API:** `/api/loyalty…`,
`/api/referrals…`, `/api/awards…`, `/api/customers/{id}/loyalty|redeem|points`

The original Pablo Loyalty engine, now one module (Customers → Loyalty & Rewards) and fully ledger-based.

## Earning (preserved legacy rule)
For every sale line: `floor(line total ÷ threshold) × points-per`. Defaults: 1 point per KSh 500; a product can
override (e.g. *Classic Watch*: 2 points per KSh 1,000). Products can be excluded; a minimum sale amount applies if set.
Points are awarded only after the sale completes, and only to identified customers. When points are redeemed on a sale,
earning is scaled to the amount actually paid. The POS shows **🌼 +N Loyalty Points** before submitting.

## Referrals
A referrer earns **50 %** (configurable) of every point their referred customer earns. Each customer has at most one
active referrer; self- and circular referrals are refused. Points are tracked separately as **own** and **referral**
and displayed as `(own|referral)` as in the original app. Removing a referral stops future bonuses only.

## Redemption & expiry
1 point = KSh 1 by default; minimum balance to redeem 100 points. Redeem at the POS (reduces the amount payable) or
from the customer page (gifts). Optional expiry after N days — expired **first-in-first-out**, run every 15 minutes.

## Ledger
Every change is a `loyalty_ledger` row: earn · referral · redeem · expire · adjust (manual, reason required) ·
reversal (returns/cancellations remove the customer's points and the referrer's matching share). Customer totals:
own, referral, redeemed, expired, **available**.

## Tiers
Configurable names and minimum lifetime spend (defaults Bronze 20k · Silver 50k · Gold 100k). Shown as medals.

## Award periods (Gold / Silver / Bronze)
One open period at a time with **live standings** ranked by spend within the period. *Close & award* records the top N
(default 5) as Gold, Silver, Bronze, Bronze, Bronze and can WhatsApp the winners. **Opening a new period does not reset
balances or delete history** (the legacy app did). Past periods show their winners.

## Overview tab
Outstanding points and their value, own vs referral points, redeemed, expired, members with points, tier distribution,
top customers, and the current rules.

## Permissions & settings
`customers.view_loyalty`, `customers.redeem_points`, `loyalty.manage`. Settings → Loyalty & rewards: enable, threshold,
points per block, minimum sale, referral %, expiry, redemption on/off, point value, minimum redemption, tiers, winners per
period, show points / value on the ordering portal.
