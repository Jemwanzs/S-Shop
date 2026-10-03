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
Best sellers by revenue and by quantity (🥇🥈🥉 by rank) · Slow movers (in stock, least sold) · Low stock ·
Top customers (with own|referral points) · Staff performance (medals by rank) · Branch comparison bars.

## Permissions
`dashboard.view`; financial KPIs `sales.view_financials`; credit KPI `credit.view`.
