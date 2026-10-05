# 01 · Dashboard & analytics

**Scope:** §23 Dashboard, §24 Gold/Silver/Bronze, §25 user performance · **Route:** `/` · **API:** `GET /api/dashboard`

## Purpose
A visual, uncluttered view of how the business is doing for any period, branch, category, product or staff member.

## Screens
- **Phone:** period chips scroll sideways; KPI cards in 2 columns; charts and lists stack.
- **Desktop:** KPI grid 4 columns (6 on large screens); sales trend beside payment mix; four product panels in a row;
  customers, staff and branch comparison side by side.
- Users **without** `dashboard.view` (e.g. salespeople) see a quick-action home (New sale, Orders, Stock, Customers…).

## Filters
Period (default **This week**): Today · Yesterday · This week · This month · This year · specific date · date range.
Branch (all the user's branches or one) · Category · Staff member · Product (open *Performance* on a product).

## KPIs
Sales (with % change vs the previous equal period) · Transactions (+% change) · Average transaction · Units sold ·
Orders (+ open orders) · Gross profit* (+ % of sales that have cost data) · Expenses · Net performance* (gross profit −
expenses) · Stock value* · Customers served (+ new customers) · Credit outstanding (needs `credit.view`) ·
Loyalty points issued / redeemed. *Financial figures need `sales.view_financials`.

Sales figures are **net of returns** and exclude cancelled sales. Points redemptions reduce the amount paid but not
product revenue.

## Panels
Sales trend (daily ≤ 2 months, weekly ≤ 1 year, else monthly; profit line when permitted) · Payment mix ·
Best sellers by revenue and by quantity · Slow movers (in stock, least sold) · Low stock ·
Top customers (with own|referral points, 🥇🥈🥉 by rank) · Staff performance · Branch comparison bars.

### Medals (best sellers & staff)
Set in Settings → Reports. **Rank** (default): 1st 🥇, 2nd 🥈, 3rd 🥉. **Targets**: anyone reaching a target earns the
medal regardless of position. Targets are entered **per day**, on sales value or units, and multiplied by the number of
days in the period viewed. So a Gold of 10,000/day means 10,000 for *Today* and 70,000 for a 7-day week. A target of 0
switches that medal off. Entries without a medal show their position number. The server computes the medal and returns
it as `medal` on `top_products_*` and `by_user`.

## Permissions
`dashboard.view`; financial KPIs `sales.view_financials`; credit KPI `credit.view`.

## My Dashboard (`/my`)
Every user who sells or serves has **My Dashboard**: the same panels computed only from their own work — sales,
transactions, average sale, units, orders they created or progressed, customers they added, credit they gave, points
issued on their sales, their best sellers and customers, their recent activity — plus their **position among sellers**
for the period (position only, never colleagues' figures). The server always scopes it to the signed-in user.

On the business dashboard, choosing a staff member needs *View other employees*, and every figure then follows that
person (not only sales). The staff leaderboard is shown only with that permission.

## Leaderboards (`/leaderboards`)
Ranked boards over a period and branch, from the same net sale lines as the dashboard and reports (sales after
returns), so the figures agree everywhere. Shown to users with `dashboard.view` or `reports.view`.

- **Products** — rank by sales value, units, number of sales, orders, profit or margin (category filter). Profit and
  margin need `sales.view_financials`; without it they are neither offered nor returned.
- **Staff** — rank by sales value, units, transactions, average sale, orders processed, customers served, new
  customers, discounts given or credit sales. Needs *View other employees* (`staff.view_others`); without it the tab
  is hidden and the server refuses.
- **Medals** follow Settings → Reports: by rank (top three), or by targets for value/units scaled to the period;
  other metrics always medal the top three.

## Recent activity
A compact feed of the latest sales, returns, orders, stock receipts, transfers, adjustments, expenses, new customers,
credit payments and approvals — each kind only with the permission that guards it, only for the user's branches, and
only the user's own actions without *View other employees* (always on My Dashboard).
