//! Reports: standard templates with shared filters and Excel export.
//! (PDF is rendered client-side from the same JSON.)
//!
//! Every report SQL starts with a `params` CTE that types all bind parameters:
//! $1 tenant, $2 branches, $3 start, $4 end, $5 product, $6 category, $7 user,
//! $8 from-date, $9 to-date, $10 time zone, $11 customer, $12 low-stock default.

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_xlsxwriter::{Format, Workbook};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::Period;
use crate::auth::Ctx;
use crate::error::{AppError, AppResult};
use crate::settings::{self, Valuation};
use crate::state::AppState;
use crate::util::local_range;

pub fn routes() -> Router<AppState> {
    Router::new().route("/reports", get(catalogue)).route("/reports/{key}", get(run))
}

#[derive(Serialize, Clone, Copy)]
pub struct Col {
    key: &'static str,
    label: &'static str,
    /// text | money | int | percent | date | datetime
    kind: &'static str,
    financial: bool,
}

const fn c(key: &'static str, label: &'static str, kind: &'static str) -> Col {
    Col { key, label, kind, financial: false }
}
const fn f(key: &'static str, label: &'static str, kind: &'static str) -> Col {
    Col { key, label, kind, financial: true }
}

#[derive(Serialize)]
pub struct Report {
    key: &'static str,
    title: &'static str,
    group: &'static str,
    description: &'static str,
    permission: &'static str,
    columns: &'static [Col],
    #[serde(skip)]
    sql: &'static str,
}

const PARAMS: &str = "WITH params AS (SELECT $1::uuid AS tid, $2::uuid[] AS br, $3::timestamptz AS ts, $4::timestamptz AS te,
    $5::uuid AS pid, $6::uuid AS cid, $7::uuid AS uid, $8::date AS fd, $9::date AS td, $10::text AS tz, $11::uuid AS cust,
    $12::int AS low)";

const LINES: &str = "lines AS (
    SELECT s.id AS sale_id, s.branch_id, s.user_id, s.customer_id, s.created_at, s.payment_method, si.product_id,
           si.quantity - si.returned_qty AS qty, (si.quantity - si.returned_qty) * si.unit_price AS revenue,
           (si.quantity - si.returned_qty) * (si.marked_price - si.unit_price) AS discount,
           CASE WHEN si.unit_cost IS NOT NULL THEN (si.quantity - si.returned_qty) * si.unit_cost END AS cost
    FROM sales s JOIN sale_items si ON si.sale_id = s.id JOIN products p ON p.id = si.product_id, params
    WHERE s.tenant_id = params.tid AND s.branch_id = ANY(params.br) AND s.created_at >= params.ts AND s.created_at < params.te
      AND s.status <> 'cancelled' AND (params.pid IS NULL OR si.product_id = params.pid)
      AND (params.cid IS NULL OR p.category_id = params.cid) AND (params.uid IS NULL OR s.user_id = params.uid)
      AND (params.cust IS NULL OR s.customer_id = params.cust))";

pub const REPORTS: &[Report] = &[
    Report {
        key: "sales", title: "Sales Report", group: "Sales", permission: "sales.view",
        description: "Every sale in the period with totals, discounts and payment method",
        columns: &[c("created_at", "Date", "datetime"), c("receipt_no", "Receipt", "text"), c("branch", "Branch", "text"),
            c("customer", "Customer", "text"), c("salesperson", "Salesperson", "text"), c("items", "Items", "int"),
            c("gross", "Marked total", "money"), c("discount", "Discount", "money"), c("total", "Total", "money"),
            c("payment_method", "Payment", "text"), c("status", "Status", "text")],
        sql: "SELECT s.created_at, s.receipt_no, b.name AS branch, NULLIF(TRIM(c.first_name || ' ' || c.other_names),'') AS customer,
                     u.name AS salesperson, (SELECT SUM(quantity) FROM sale_items si WHERE si.sale_id = s.id) AS items,
                     s.gross_total AS gross, s.discount_total AS discount, s.total, s.payment_method, s.status
              FROM sales s JOIN branches b ON b.id = s.branch_id LEFT JOIN customers c ON c.id = s.customer_id
              LEFT JOIN users u ON u.id = s.user_id, params
              WHERE s.tenant_id = params.tid AND s.branch_id = ANY(params.br) AND s.created_at >= params.ts AND s.created_at < params.te
                AND (params.uid IS NULL OR s.user_id = params.uid) AND (params.cust IS NULL OR s.customer_id = params.cust)
                AND (params.pid IS NULL OR EXISTS (SELECT 1 FROM sale_items si WHERE si.sale_id = s.id AND si.product_id = params.pid))
              ORDER BY s.created_at DESC",
    },
    Report {
        key: "sales_by_product", title: "Sales by Product", group: "Sales", permission: "sales.view",
        description: "Units, revenue and discounts per product (net of returns)",
        columns: &[c("product", "Product", "text"), c("code", "Code", "text"), c("category", "Category", "text"), c("units", "Units", "int"),
            c("revenue", "Revenue", "money"), c("discount", "Discount", "money"), c("avg_price", "Avg price", "money"),
            f("profit", "Gross profit", "money"), f("margin", "Margin %", "percent")],
        sql: "SELECT p.name AS product, p.code, cat.name AS category, SUM(l.qty) AS units, SUM(l.revenue) AS revenue, SUM(l.discount) AS discount,
                     ROUND(SUM(l.revenue) / NULLIF(SUM(l.qty),0), 2) AS avg_price,
                     SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL) - SUM(l.cost) AS profit,
                     ROUND((SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL) - SUM(l.cost)) * 100 / NULLIF(SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL),0), 1) AS margin
              FROM lines l JOIN products p ON p.id = l.product_id LEFT JOIN categories cat ON cat.id = p.category_id
              GROUP BY p.id, p.name, p.code, cat.name ORDER BY revenue DESC",
    },
    Report {
        key: "sales_by_branch", title: "Sales by Branch", group: "Sales", permission: "sales.view",
        description: "Branch comparison: transactions, units, revenue and average ticket",
        columns: &[c("branch", "Branch", "text"), c("transactions", "Transactions", "int"), c("units", "Units", "int"),
            c("revenue", "Revenue", "money"), c("discount", "Discount", "money"), c("avg_ticket", "Avg ticket", "money"),
            f("profit", "Gross profit", "money")],
        sql: "SELECT b.name AS branch, COUNT(DISTINCT l.sale_id) AS transactions, SUM(l.qty) AS units, SUM(l.revenue) AS revenue,
                     SUM(l.discount) AS discount, ROUND(SUM(l.revenue) / NULLIF(COUNT(DISTINCT l.sale_id),0), 2) AS avg_ticket,
                     SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL) - SUM(l.cost) AS profit
              FROM lines l JOIN branches b ON b.id = l.branch_id GROUP BY b.id, b.name ORDER BY revenue DESC",
    },
    Report {
        key: "sales_by_user", title: "Sales by User", group: "Sales", permission: "sales.view",
        description: "Sales value, units and transactions per salesperson",
        columns: &[c("user", "Salesperson", "text"), c("transactions", "Transactions", "int"), c("units", "Units", "int"),
            c("revenue", "Revenue", "money"), c("discount", "Discounts given", "money"), c("avg_ticket", "Avg ticket", "money")],
        sql: "SELECT u.name AS user, COUNT(DISTINCT l.sale_id) AS transactions, SUM(l.qty) AS units, SUM(l.revenue) AS revenue,
                     SUM(l.discount) AS discount, ROUND(SUM(l.revenue) / NULLIF(COUNT(DISTINCT l.sale_id),0), 2) AS avg_ticket
              FROM lines l JOIN users u ON u.id = l.user_id GROUP BY u.id, u.name ORDER BY revenue DESC",
    },
    Report {
        key: "orders", title: "Order Report", group: "Orders", permission: "orders.view",
        description: "Orders placed in the period with status and value",
        columns: &[c("created_at", "Date", "datetime"), c("order_no", "Order", "text"), c("branch", "Branch", "text"),
            c("customer", "Customer", "text"), c("mobile", "Mobile", "text"), c("source", "Source", "text"), c("items", "Items", "int"),
            c("total", "Total", "money"), c("status", "Status", "text"), c("receipt_no", "Receipt", "text")],
        sql: "SELECT o.created_at, o.order_no, b.name AS branch, TRIM(c.first_name || ' ' || c.other_names) AS customer, c.mobile, o.source,
                     (SELECT SUM(quantity) FROM order_items oi WHERE oi.order_id = o.id) AS items, o.total, o.status, s.receipt_no
              FROM orders o JOIN branches b ON b.id = o.branch_id JOIN customers c ON c.id = o.customer_id
              LEFT JOIN sales s ON s.id = o.sale_id, params
              WHERE o.tenant_id = params.tid AND o.branch_id = ANY(params.br) AND o.created_at >= params.ts AND o.created_at < params.te
                AND (params.cust IS NULL OR o.customer_id = params.cust)
              ORDER BY o.created_at DESC",
    },
    Report {
        key: "stock_position", title: "Stock Position", group: "Stock", permission: "stock.view",
        description: "Opening, received, transfers, sold, returns, adjustments and closing stock",
        columns: &[c("name", "Product", "text"), c("code", "Code", "text"), c("opening", "Opening", "int"), c("added", "Added", "int"),
            c("transfers_in", "Transfers in", "int"), c("transfers_out", "Transfers out", "int"), c("sold", "Sold", "int"),
            c("returns", "Returns", "int"), c("adjustments", "Adjustments", "int"), c("damaged_written_off", "Damaged / written off", "int"),
            c("closing", "Closing", "int"), c("reserved", "Reserved", "int"), f("value", "Value", "money")],
        sql: "",
    },
    Report {
        key: "stock_movement", title: "Stock Movement", group: "Stock", permission: "stock.view",
        description: "Every ledger movement in the period",
        columns: &[c("created_at", "Date", "datetime"), c("branch", "Branch", "text"), c("product", "Product", "text"),
            c("kind", "Movement", "text"), c("quantity", "Qty", "int"), c("barcode", "Barcode", "text"), f("unit_cost", "Unit cost", "money"),
            c("unit_price", "Unit price", "money"), c("notes", "Notes", "text"), c("user", "User", "text")],
        sql: "SELECT m.created_at, b.name AS branch, p.name AS product, m.kind, m.quantity, si.barcode, m.unit_cost, m.unit_price, m.notes, u.name AS user
              FROM stock_movements m JOIN branches b ON b.id = m.branch_id JOIN products p ON p.id = m.product_id
              LEFT JOIN stock_items si ON si.id = m.stock_item_id LEFT JOIN users u ON u.id = m.user_id, params
              WHERE m.tenant_id = params.tid AND m.branch_id = ANY(params.br) AND m.created_at >= params.ts AND m.created_at < params.te
                AND (params.pid IS NULL OR m.product_id = params.pid) AND (params.cid IS NULL OR p.category_id = params.cid)
                AND (params.uid IS NULL OR m.user_id = params.uid)
              ORDER BY m.created_at DESC",
    },
    Report {
        key: "stock_valuation", title: "Stock Valuation", group: "Stock", permission: "stock.view",
        description: "Current stock at cost and at selling price per branch",
        columns: &[c("product", "Product", "text"), c("code", "Code", "text"), c("branch", "Branch", "text"), c("on_hand", "On hand", "int"),
            f("unit_cost", "Unit cost", "money"), c("unit_price", "Unit price", "money"), f("cost_value", "Cost value", "money"),
            c("retail_value", "Retail value", "money")],
        sql: "SELECT p.name AS product, p.code, b.name AS branch, sl.on_hand, p.cost_price AS unit_cost, p.marked_price AS unit_price,
                     sl.on_hand * p.cost_price AS cost_value, sl.on_hand * p.marked_price AS retail_value
              FROM stock_levels sl JOIN products p ON p.id = sl.product_id JOIN branches b ON b.id = sl.branch_id, params
              WHERE sl.tenant_id = params.tid AND sl.branch_id = ANY(params.br) AND sl.on_hand <> 0
                AND (params.pid IS NULL OR p.id = params.pid) AND (params.cid IS NULL OR p.category_id = params.cid)
              ORDER BY p.name, b.name",
    },
    Report {
        key: "stock_transfers", title: "Stock Transfer Report", group: "Stock", permission: "stock.view",
        description: "Transfers between branches and their status",
        columns: &[c("created_at", "Date", "datetime"), c("transfer_no", "Transfer", "text"), c("from_branch", "From", "text"),
            c("to_branch", "To", "text"), c("units", "Units", "int"), c("status", "Status", "text"), c("created_by", "Created by", "text"),
            c("received_at", "Received", "datetime")],
        sql: "SELECT t.created_at, t.transfer_no, fb.name AS from_branch, tb.name AS to_branch,
                     (SELECT SUM(quantity) FROM transfer_items ti WHERE ti.transfer_id = t.id) AS units, t.status, u.name AS created_by, t.received_at
              FROM transfers t JOIN branches fb ON fb.id = t.from_branch_id JOIN branches tb ON tb.id = t.to_branch_id
              LEFT JOIN users u ON u.id = t.created_by, params
              WHERE t.tenant_id = params.tid AND (t.from_branch_id = ANY(params.br) OR t.to_branch_id = ANY(params.br))
                AND t.created_at >= params.ts AND t.created_at < params.te
              ORDER BY t.created_at DESC",
    },
    Report {
        key: "stock_adjustments", title: "Stock Adjustment Report", group: "Stock", permission: "stock.view",
        description: "Counts, damages, losses, write-offs and manual adjustments",
        columns: &[c("created_at", "Date", "datetime"), c("branch", "Branch", "text"), c("product", "Product", "text"), c("kind", "Type", "text"),
            c("previous_qty", "Before", "int"), c("delta", "Change", "int"), c("new_qty", "After", "int"), c("reason", "Reason", "text"),
            c("status", "Status", "text"), c("created_by", "By", "text"), c("decided_by", "Approved by", "text")],
        sql: "SELECT a.created_at, b.name AS branch, p.name AS product, a.kind, a.previous_qty, a.delta, a.new_qty, a.reason, a.status,
                     cu.name AS created_by, du.name AS decided_by
              FROM stock_adjustments a JOIN branches b ON b.id = a.branch_id JOIN products p ON p.id = a.product_id
              LEFT JOIN users cu ON cu.id = a.created_by LEFT JOIN users du ON du.id = a.decided_by, params
              WHERE a.tenant_id = params.tid AND a.branch_id = ANY(params.br) AND a.created_at >= params.ts AND a.created_at < params.te
                AND (params.pid IS NULL OR a.product_id = params.pid)
              ORDER BY a.created_at DESC",
    },
    Report {
        key: "barcode_inventory", title: "Barcode Inventory Report", group: "Stock", permission: "stock.view",
        description: "Individually tracked items currently in stock, reserved or in transit",
        columns: &[c("barcode", "Barcode", "text"), c("product", "Product", "text"), c("branch", "Branch", "text"), c("status", "Status", "text"),
            c("received", "Received", "datetime"), f("cost_price", "Cost", "money")],
        sql: "SELECT si.barcode, p.name AS product, b.name AS branch, si.status, si.created_at AS received, si.cost_price
              FROM stock_items si JOIN products p ON p.id = si.product_id JOIN branches b ON b.id = si.branch_id, params
              WHERE si.tenant_id = params.tid AND si.branch_id = ANY(params.br) AND si.status IN ('in_stock','reserved','in_transit')
                AND (params.pid IS NULL OR si.product_id = params.pid) AND (params.cid IS NULL OR p.category_id = params.cid)
              ORDER BY p.name, si.barcode",
    },
    Report {
        key: "low_stock", title: "Low Stock Report", group: "Stock", permission: "stock.view",
        description: "Products at or below their low-stock threshold",
        columns: &[c("product", "Product", "text"), c("code", "Code", "text"), c("branch", "Branch", "text"), c("available", "Available", "int"),
            c("reserved", "Reserved", "int"), c("threshold", "Threshold", "int")],
        sql: "SELECT p.name AS product, p.code, b.name AS branch, sl.on_hand - sl.reserved AS available, sl.reserved,
                     COALESCE(p.low_stock_threshold, params.low) AS threshold
              FROM stock_levels sl JOIN products p ON p.id = sl.product_id JOIN branches b ON b.id = sl.branch_id, params
              WHERE sl.tenant_id = params.tid AND sl.branch_id = ANY(params.br) AND p.is_active
                AND sl.on_hand - sl.reserved > 0 AND sl.on_hand - sl.reserved <= COALESCE(p.low_stock_threshold, params.low)
                AND (params.cid IS NULL OR p.category_id = params.cid)
              ORDER BY available, p.name",
    },
    Report {
        key: "out_of_stock", title: "Out-of-Stock Report", group: "Stock", permission: "stock.view",
        description: "Active products with nothing available to sell",
        columns: &[c("product", "Product", "text"), c("code", "Code", "text"), c("branch", "Branch", "text"), c("on_hand", "On hand", "int"),
            c("reserved", "Reserved", "int"), c("last_sold", "Last sold", "datetime")],
        sql: "SELECT p.name AS product, p.code, b.name AS branch, COALESCE(sl.on_hand,0) AS on_hand, COALESCE(sl.reserved,0) AS reserved,
                     (SELECT MAX(m.created_at) FROM stock_movements m WHERE m.product_id = p.id AND m.branch_id = b.id AND m.kind IN ('sale','order_completion')) AS last_sold
              FROM products p CROSS JOIN branches b LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = b.id, params
              WHERE p.tenant_id = params.tid AND b.id = ANY(params.br) AND p.is_active
                AND (p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = b.id))
                AND COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) <= 0 AND (params.cid IS NULL OR p.category_id = params.cid)
              ORDER BY p.name, b.name",
    },
    Report {
        key: "customers", title: "Customer Report", group: "Customers", permission: "customers.view",
        description: "Customer book with spend, visits, points and tier",
        columns: &[c("name", "Customer", "text"), c("nickname", "Nickname", "text"), c("mobile", "Mobile", "text"),
            c("total_spend", "Total spend", "money"), c("purchases", "Purchases", "int"), c("last_purchase", "Last purchase", "datetime"),
            c("points", "Points", "int"), c("tier", "Tier", "text"), c("joined", "Joined", "datetime")],
        sql: "SELECT TRIM(c.first_name || ' ' || c.other_names) AS name, c.nickname, c.mobile, c.total_spend, c.purchase_count AS purchases,
                     c.last_purchase_at AS last_purchase, c.points_available AS points, c.tier, c.created_at AS joined
              FROM customers c, params WHERE c.tenant_id = params.tid ORDER BY c.total_spend DESC",
    },
    Report {
        key: "customer_purchases", title: "Customer Purchase History", group: "Customers", permission: "customers.view",
        description: "Every item bought per customer in the period (filter by customer)",
        columns: &[c("created_at", "Date", "datetime"), c("customer", "Customer", "text"), c("mobile", "Mobile", "text"),
            c("receipt_no", "Receipt", "text"), c("product", "Product", "text"), c("qty", "Qty", "int"), c("revenue", "Amount", "money")],
        sql: "SELECT l.created_at, TRIM(c.first_name || ' ' || c.other_names) AS customer, c.mobile, s.receipt_no, p.name AS product, l.qty, l.revenue
              FROM lines l JOIN customers c ON c.id = l.customer_id JOIN sales s ON s.id = l.sale_id JOIN products p ON p.id = l.product_id
              ORDER BY c.first_name, l.created_at DESC",
    },
    Report {
        key: "loyalty_points", title: "Loyalty Points Report", group: "Loyalty", permission: "customers.view_loyalty",
        description: "Points earned, referral bonuses, redeemed, expired and balances",
        columns: &[c("customer", "Customer", "text"), c("mobile", "Mobile", "text"), c("earned", "Earned", "int"), c("referral", "Referral", "int"),
            c("redeemed", "Redeemed", "int"), c("expired", "Expired", "int"), c("reversed", "Reversed", "int"), c("balance", "Balance now", "int")],
        sql: "SELECT TRIM(c.first_name || ' ' || c.other_names) AS customer, c.mobile,
                     COALESCE(SUM(l.points) FILTER (WHERE l.kind = 'earn'),0) AS earned,
                     COALESCE(SUM(l.points) FILTER (WHERE l.kind = 'referral'),0) AS referral,
                     COALESCE(-SUM(l.points) FILTER (WHERE l.kind = 'redeem'),0) AS redeemed,
                     COALESCE(-SUM(l.points) FILTER (WHERE l.kind = 'expire'),0) AS expired,
                     COALESCE(-SUM(l.points) FILTER (WHERE l.kind = 'reversal'),0) AS reversed,
                     c.points_available AS balance
              FROM loyalty_ledger l JOIN customers c ON c.id = l.customer_id, params
              WHERE l.tenant_id = params.tid AND l.created_at >= params.ts AND l.created_at < params.te
                AND (params.cust IS NULL OR l.customer_id = params.cust)
              GROUP BY c.id ORDER BY earned DESC",
    },
    Report {
        key: "loyalty_redemptions", title: "Loyalty Redemption Report", group: "Loyalty", permission: "customers.view_loyalty",
        description: "Every redemption with its value",
        columns: &[c("created_at", "Date", "datetime"), c("customer", "Customer", "text"), c("points", "Points", "int"),
            c("receipt_no", "Receipt", "text"), c("notes", "Notes", "text"), c("user", "By", "text")],
        sql: "SELECT l.created_at, TRIM(c.first_name || ' ' || c.other_names) AS customer, -l.points AS points, s.receipt_no, l.notes, u.name AS user
              FROM loyalty_ledger l JOIN customers c ON c.id = l.customer_id LEFT JOIN sales s ON s.id = l.sale_id
              LEFT JOIN users u ON u.id = l.user_id, params
              WHERE l.tenant_id = params.tid AND l.kind = 'redeem' AND l.created_at >= params.ts AND l.created_at < params.te
              ORDER BY l.created_at DESC",
    },
    Report {
        key: "referrals", title: "Referral Report", group: "Loyalty", permission: "customers.view",
        description: "Who referred whom and the bonus points earned",
        columns: &[c("created_at", "Date", "datetime"), c("referrer", "Referrer", "text"), c("referred", "Referred customer", "text"),
            c("bonus_points", "Bonus points", "int"), c("referred_spend", "Referred spend", "money")],
        sql: "SELECT r.created_at, TRIM(a.first_name || ' ' || a.other_names) AS referrer, TRIM(b.first_name || ' ' || b.other_names) AS referred,
                     r.bonus_points_earned AS bonus_points, b.total_spend AS referred_spend
              FROM referrals r JOIN customers a ON a.id = r.referrer_id JOIN customers b ON b.id = r.referred_id, params
              WHERE r.tenant_id = params.tid AND r.is_active ORDER BY r.created_at DESC",
    },
    Report {
        key: "credit_sales", title: "Credit Sales Report", group: "Credit", permission: "credit.view",
        description: "Credit issued in the period with payments and balances",
        columns: &[c("created_at", "Date", "datetime"), c("receipt_no", "Receipt", "text"), c("customer", "Customer", "text"),
            c("mobile", "Mobile", "text"), c("branch", "Branch", "text"), c("salesperson", "Salesperson", "text"),
            c("amount", "Amount", "money"), c("paid", "Paid", "money"), c("balance", "Balance", "money"), c("due_date", "Due", "date"),
            c("status", "Status", "text")],
        sql: "SELECT cs.created_at, s.receipt_no, TRIM(c.first_name || ' ' || c.other_names) AS customer, c.mobile, b.name AS branch,
                     u.name AS salesperson, cs.original_amount - cs.adjustments AS amount, cs.amount_paid AS paid,
                     cs.original_amount - cs.amount_paid - cs.adjustments AS balance, cs.due_date,
                     CASE WHEN cs.status IN ('outstanding','partially_paid') AND cs.due_date < params.td THEN 'overdue' ELSE cs.status END AS status
              FROM credit_sales cs JOIN sales s ON s.id = cs.sale_id JOIN customers c ON c.id = cs.customer_id
              JOIN branches b ON b.id = cs.branch_id LEFT JOIN users u ON u.id = cs.user_id, params
              WHERE cs.tenant_id = params.tid AND cs.branch_id = ANY(params.br) AND cs.created_at >= params.ts AND cs.created_at < params.te
                AND (params.cust IS NULL OR cs.customer_id = params.cust) AND (params.uid IS NULL OR cs.user_id = params.uid)
              ORDER BY cs.created_at DESC",
    },
    Report {
        key: "credit_aging", title: "Credit Aging Report", group: "Credit", permission: "credit.view",
        description: "Open balances per customer by days past due (as at the end date)",
        columns: &[c("customer", "Customer", "text"), c("mobile", "Mobile", "text"), c("current", "Not yet due", "money"),
            c("d1_30", "1–30 days", "money"), c("d31_60", "31–60 days", "money"), c("d61_90", "61–90 days", "money"),
            c("d90", "90+ days", "money"), c("total", "Total", "money")],
        sql: "SELECT TRIM(c.first_name || ' ' || c.other_names) AS customer, c.mobile,
                     SUM(x.bal) FILTER (WHERE x.late <= 0) AS current, SUM(x.bal) FILTER (WHERE x.late BETWEEN 1 AND 30) AS d1_30,
                     SUM(x.bal) FILTER (WHERE x.late BETWEEN 31 AND 60) AS d31_60, SUM(x.bal) FILTER (WHERE x.late BETWEEN 61 AND 90) AS d61_90,
                     SUM(x.bal) FILTER (WHERE x.late > 90) AS d90, SUM(x.bal) AS total
              FROM (SELECT cs.customer_id, cs.original_amount - cs.amount_paid - cs.adjustments AS bal, params.td - cs.due_date AS late
                    FROM credit_sales cs, params WHERE cs.tenant_id = params.tid AND cs.branch_id = ANY(params.br)
                      AND cs.status IN ('outstanding','partially_paid')) x
              JOIN customers c ON c.id = x.customer_id GROUP BY c.id ORDER BY total DESC",
    },
    Report {
        key: "expenses", title: "Expenses Report", group: "Finance", permission: "expenses.view",
        description: "Approved and pending expenses by category and branch",
        columns: &[c("expense_date", "Date", "date"), c("branch", "Branch", "text"), c("category", "Category", "text"),
            c("description", "Description", "text"), c("payee", "Payee", "text"), c("payment_method", "Paid by", "text"),
            c("amount", "Amount", "money"), c("status", "Status", "text"), c("user", "Recorded by", "text")],
        sql: "SELECT e.expense_date, b.name AS branch, ec.name AS category, e.description, e.payee, e.payment_method, e.amount, e.status, u.name AS user
              FROM expenses e JOIN branches b ON b.id = e.branch_id JOIN expense_categories ec ON ec.id = e.category_id
              LEFT JOIN users u ON u.id = e.user_id, params
              WHERE e.tenant_id = params.tid AND e.branch_id = ANY(params.br) AND e.expense_date BETWEEN params.fd AND params.td
                AND e.status <> 'void' AND (params.uid IS NULL OR e.user_id = params.uid)
              ORDER BY e.expense_date DESC",
    },
    Report {
        key: "profitability", title: "Profitability Report", group: "Finance", permission: "sales.view_financials",
        description: "Revenue, cost and margin per product where cost data exists",
        columns: &[c("product", "Product", "text"), c("units", "Units", "int"), c("revenue", "Revenue", "money"), f("cost", "Cost", "money"),
            f("profit", "Gross profit", "money"), f("margin", "Margin %", "percent"), c("cost_coverage", "Cost data %", "percent")],
        sql: "SELECT p.name AS product, SUM(l.qty) AS units, SUM(l.revenue) AS revenue, SUM(l.cost) AS cost,
                     SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL) - SUM(l.cost) AS profit,
                     ROUND((SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL) - SUM(l.cost)) * 100 / NULLIF(SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL),0), 1) AS margin,
                     ROUND(COALESCE(SUM(l.revenue) FILTER (WHERE l.cost IS NOT NULL),0) * 100 / NULLIF(SUM(l.revenue),0), 0) AS cost_coverage
              FROM lines l JOIN products p ON p.id = l.product_id GROUP BY p.id, p.name ORDER BY profit DESC NULLS LAST",
    },
    Report {
        key: "user_performance", title: "User Performance Report", group: "Staff", permission: "reports.view",
        description: "Sales, orders processed, discounts, credit and customers per user",
        columns: &[c("user", "User", "text"), c("sales_value", "Sales value", "money"), c("units", "Units", "int"),
            c("transactions", "Transactions", "int"), c("avg_ticket", "Avg ticket", "money"), c("discounts", "Discounts given", "money"),
            c("credit_sales", "Credit sales", "money"), c("orders_processed", "Orders processed", "int"),
            c("customers_served", "Customers served", "int"), c("new_customers", "New customers", "int")],
        sql: "SELECT u.name AS user, COALESCE(SUM(l.revenue),0) AS sales_value, COALESCE(SUM(l.qty),0) AS units,
                     COUNT(DISTINCT l.sale_id) AS transactions, ROUND(COALESCE(SUM(l.revenue),0) / NULLIF(COUNT(DISTINCT l.sale_id),0), 2) AS avg_ticket,
                     COALESCE(SUM(l.discount),0) AS discounts,
                     COALESCE(SUM(l.revenue) FILTER (WHERE l.payment_method = 'credit'),0) AS credit_sales,
                     (SELECT COUNT(DISTINCT e.order_id) FROM order_events e WHERE e.user_id = u.id AND e.created_at >= params.ts AND e.created_at < params.te) AS orders_processed,
                     COUNT(DISTINCT l.customer_id) AS customers_served,
                     (SELECT COUNT(*) FROM customers c WHERE c.created_by = u.id AND c.created_at >= params.ts AND c.created_at < params.te) AS new_customers
              FROM users u CROSS JOIN params LEFT JOIN lines l ON l.user_id = u.id
              WHERE u.tenant_id = params.tid AND (params.uid IS NULL OR u.id = params.uid)
              GROUP BY u.id, u.name, params.ts, params.te HAVING COUNT(l.sale_id) > 0 OR u.is_active
              ORDER BY sales_value DESC",
    },
];

async fn catalogue(ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("reports.view")?;
    let reports: Vec<&Report> = REPORTS
        .iter()
        .filter(|r| ctx.can(r.permission) && (ctx.sees_others() || !matches!(r.key, "sales_by_user" | "user_performance")))
        .collect();
    Ok(Json(json!({ "reports": reports, "can_export": ctx.can("reports.export") })))
}

#[derive(Deserialize)]
struct RunQuery {
    #[serde(flatten)]
    period: Period,
    branch_id: Option<Uuid>,
    product_id: Option<Uuid>,
    category_id: Option<Uuid>,
    user_id: Option<Uuid>,
    customer_id: Option<Uuid>,
    /// json (default) | xlsx
    format: Option<String>,
}

async fn run(State(state): State<AppState>, ctx: Ctx, Path(key): Path<String>, Query(q): Query<RunQuery>) -> AppResult<Response> {
    ctx.require("reports.view")?;
    // Per-employee reports need "view other employees"; otherwise every report is limited to the user's own records.
    if matches!(key.as_str(), "sales_by_user" | "user_performance") && !ctx.sees_others() {
        return Err(AppError::Forbidden("Viewing other employees' performance needs permission".into()));
    }
    let user_filter = if ctx.sees_others() { q.user_id } else { Some(ctx.user_id) };
    let report = REPORTS.iter().find(|r| r.key == key).ok_or(AppError::NotFound("Report"))?;
    ctx.require(report.permission)?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let (from, to) = q.period.resolve(ctx.tz, "month");
    let (start, end) = local_range(from, to, ctx.tz);
    let mut conn = state.db.acquire().await?;
    let s = settings::load(&mut conn, ctx.tenant_id).await?;

    let rows: Vec<Value> = if report.key == "stock_position" {
        let pq = super::stock::PositionQuery {
            branch_id: q.branch_id,
            category_id: q.category_id,
            product_id: q.product_id,
            status: None,
            period: q.period.clone(),
        };
        let (_, _, rows) = super::stock::position_rows(&mut conn, &ctx, &pq).await?;
        rows.into_iter().filter_map(|r| serde_json::to_value(r).ok()).collect()
    } else {
        let uses_lines = report.sql.contains("FROM lines") || report.sql.contains("JOIN lines");
        let sql = if uses_lines {
            format!("{PARAMS}, {LINES} SELECT to_jsonb(r) FROM ({}) r", report.sql)
        } else {
            format!("{PARAMS} SELECT to_jsonb(r) FROM ({}) r", report.sql)
        };
        sqlx::query_scalar(&sql)
            .bind(ctx.tenant_id)
            .bind(&branches)
            .bind(start)
            .bind(end)
            .bind(q.product_id)
            .bind(q.category_id)
            .bind(user_filter)
            .bind(from)
            .bind(to)
            .bind(ctx.tz.name())
            .bind(q.customer_id)
            .bind(s.stock.low_stock_threshold)
            .fetch_all(&mut *conn)
            .await?
    };

    let hide_fin = s.reports.hide_financials_without_permission && !ctx.can("sales.view_financials");
    let columns: Vec<Col> = report.columns.iter().filter(|c| !(hide_fin && c.financial)).copied().collect();
    let rows: Vec<Value> = if hide_fin {
        rows.into_iter()
            .map(|mut r| {
                if let Some(o) = r.as_object_mut() {
                    for c in report.columns.iter().filter(|c| c.financial) {
                        o.remove(c.key);
                    }
                }
                r
            })
            .collect()
    } else {
        rows
    };

    // Totals for money / int columns.
    let mut totals = serde_json::Map::new();
    for col in columns.iter().filter(|c| matches!(c.kind, "money" | "int")) {
        let sum: Decimal = rows.iter().filter_map(|r| r.get(col.key)).filter_map(to_decimal).sum();
        totals.insert(col.key.to_string(), json!(sum));
    }

    let business: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&mut *conn).await?;
    let valuation = match s.stock.valuation {
        Valuation::Cost => "cost",
        Valuation::Selling => "selling price",
    };

    if q.format.as_deref() == Some("xlsx") {
        ctx.require("reports.export")?;
        let bytes = to_xlsx(report.title, &business, from, to, &columns, &rows, &totals).map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
        let filename = format!("{}-{}-{}.xlsx", report.key, from, to);
        return Ok((
            [
                (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
                (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
            ],
            bytes,
        )
            .into_response());
    }

    Ok(Json(json!({
        "report": { "key": report.key, "title": report.title, "description": report.description, "group": report.group },
        "business": business,
        "from": from, "to": to,
        "columns": columns,
        "rows": rows,
        "totals": totals,
        "valuation": valuation,
        "can_export": ctx.can("reports.export"),
    }))
    .into_response())
}

fn to_decimal(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => n.to_string().parse().ok(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn to_xlsx(
    title: &str,
    business: &str,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    columns: &[Col],
    rows: &[Value],
    totals: &serde_json::Map<String, Value>,
) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(&title.chars().filter(|c| c.is_alphanumeric() || *c == ' ').take(31).collect::<String>())?;
    let bold = Format::new().set_bold();
    let head = Format::new().set_bold().set_background_color("#F4E7DA").set_border_bottom(rust_xlsxwriter::FormatBorder::Thin);
    let money = Format::new().set_num_format("#,##0.00");
    let money_bold = Format::new().set_num_format("#,##0.00").set_bold();
    let int = Format::new().set_num_format("#,##0");

    ws.write_string_with_format(0, 0, format!("{business} — {title}"), &bold)?;
    ws.write_string(1, 0, format!("{} to {}", from.format("%d/%m/%Y"), to.format("%d/%m/%Y")))?;
    for (i, col) in columns.iter().enumerate() {
        ws.write_string_with_format(3, i as u16, col.label, &head)?;
        ws.set_column_width(i as u16, if col.kind == "text" { 22 } else { 14 })?;
    }
    for (r, row) in rows.iter().enumerate() {
        let r = (r + 4) as u32;
        for (i, col) in columns.iter().enumerate() {
            let v = row.get(col.key).unwrap_or(&Value::Null);
            let i = i as u16;
            match (col.kind, v) {
                (_, Value::Null) => {}
                ("money" | "percent", v) => {
                    if let Some(d) = to_decimal(v).and_then(|d| d.to_f64()) {
                        ws.write_number_with_format(r, i, d, &money)?;
                    }
                }
                ("int", v) => {
                    if let Some(d) = to_decimal(v).and_then(|d| d.to_f64()) {
                        ws.write_number_with_format(r, i, d, &int)?;
                    }
                }
                ("datetime" | "date", Value::String(s)) => {
                    ws.write_string(r, i, s.replace('T', " ").chars().take(16).collect::<String>())?;
                }
                (_, Value::String(s)) => {
                    ws.write_string(r, i, s)?;
                }
                (_, other) => {
                    ws.write_string(r, i, other.to_string())?;
                }
            }
        }
    }
    let total_row = (rows.len() + 4) as u32;
    ws.write_string_with_format(total_row, 0, "Total", &bold)?;
    for (i, col) in columns.iter().enumerate().skip(1) {
        if let Some(v) = totals.get(col.key).and_then(to_decimal).and_then(|d| d.to_f64()) {
            ws.write_number_with_format(total_row, i as u16, v, &money_bold)?;
        }
    }
    ws.set_freeze_panes(4, 0)?;
    wb.save_to_buffer()
}
