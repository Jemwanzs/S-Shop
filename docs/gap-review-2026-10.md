# Gap review — analytics, access control & operations (2026-10-05)

Reviewed against the code, database and roadmap before building. ✅ complete · 🟡 partial · 🔴 missing.

| # | Feature | Status | Existing implementation | Gap | Action |
|---|---|---|---|---|---|
| 1 | Product performance leaderboard | 🟡 | Dashboard: top 5 by revenue and by units (medals: rank or targets). Report *Sales by Product* (units, revenue, discount, avg price, profit, margin). | No ranked board with selectable metric (value, units, number of sales, orders, profit/margin) per branch. | **Leaderboards** screen + API on the same sales lines; metric switch; medals from Settings → Reports. |
| 2 | Employee performance leaderboard | 🟡 | Dashboard *Staff performance* top 5; report *User Performance* already computes value, units, transactions, avg ticket, discounts, credit, orders processed, customers served, new customers. | Not a ranked board; no metric choice. | Same Leaderboards screen (staff tab) reusing those definitions. |
| 3 | Recent activities | 🔴 | Data exists (sales, orders, receipts, transfers, expenses, customers, credit payments, returns, adjustments, approvals); audit trail needs `audit.view`. | No compact feed on the dashboard. | Permission-aware **Recent activity** feed (each item shown only with the matching permission and branch). |
| 4 | General dashboard | ✅ | KPIs, trend, payment mix, best/slow sellers, low stock, customers, staff, branches — all from transactions. | — | Keep; add the activity feed. |
| 5 | Branch & date filters | ✅ | Today, Yesterday, This week/month/year, specific date / range, branch (all accessible or one), product, category, staff. | — | Keep. |
| 6 | My Dashboard | 🔴 | Staff filter exists but any viewer can pick any employee. | No personal dashboard; no rule stopping a user from viewing colleagues. | **My Dashboard** (always the signed-in user) + server rule: other employees' figures need a new permission. |
| 7 | Roles & permissions | ✅ | Tenant roles (create/edit, 8 templates), 41 permissions, backend-enforced, user ↔ role ↔ branches. | Role cannot be removed. | Allow deactivating unused roles. |
| 8 | Granular action-level access | 🟡 | Actions per module (view/create/edit/deactivate/approve/export/transfer/cancel/refund/discount/change branch/financials). | Missing: *view other employees' sales & performance*, *print/share receipts*, per-area settings access; cost and profit share one permission (profit reveals cost, so they stay together). | Add `staff.view_others`, `sales.print`, settings split by area; enforce on the server. |
| 9 | Branch-level access | ✅ | All branches or assigned branches; Current Branch validated on every request. | — | Keep. |
| 10 | Working days | 🔴 | — | — | Settings → Workspace: Mon–Sun on/off, business-wide with branch overrides. |
| 11 | Operating hours | 🔴 | Periods use the calendar day (midnight) in the business time zone. | — | Opening/closing time, business-wide with branch overrides. |
| 12 | Cross-midnight business day | 🔴 | Only calendar dates. | — | Store the true timestamp **and** a `business_date` (snapshot at the time of the transaction); dashboards, reports and performance filter by business date. |
| 13 | Branch-specific hours | 🔴 | — | — | Per-branch override of days/hours; business date uses the transaction's branch. |
| 14 | Geofencing | 🔴 | Branches have a text location only. | — | Branch coordinates + radius + on/off; tenant policy *anywhere* (default) or *at the branch* for selected actions; server-side check with the device location; bypass permission; location in the audit trail. |
| 15 | Stock transfers | ✅ | Draft → Pending approval → Approved → Dispatched (in transit) → Received; dispatch deducts the source (available stock checked, rows locked), tracked units move as units (`in_transit`), receipt only from *dispatched* (row lock → no double receipt), destination stock appears only on receipt; optional receipt control; approval workflow. | Receipt is all-or-nothing. | Keep; add **receipt with discrepancies** (short / damaged recorded, not silently lost). |
| 16 | Transfer receipt / clearance | 🟡 | As above. | Shortage/damage on arrival cannot be recorded. | Discrepancy receipt with reason, audited, written to the ledger. |

**Progress:** step 1 done (2026-10-05) — items 3, 6, 7, 8 are now ✅: `staff.view_others`, `sales.print`, settings
areas, role retirement, My Dashboard, recent activity; plus a privilege-escalation fix (non-admins can no longer grant
or assign permissions they do not hold) found during this review.
Step 2 done (2026-10-05) — items 1 and 2 are now ✅: **Leaderboards** (`/leaderboards`).

**Order of work** (each end-to-end: database → backend → permissions → UI → audit → reports, with demo data):
1. Access: `staff.view_others` + data scoping, My Dashboard, recent activity, `sales.print`, settings areas, role deactivation.
2. Leaderboards (products, staff).
3. Workspace: working days, operating hours, cross-midnight business date, branch overrides.
4. Geofencing.
5. Transfer receipt with discrepancies.
Then the earlier roadmap: translations, exchange screen, offline POS.
