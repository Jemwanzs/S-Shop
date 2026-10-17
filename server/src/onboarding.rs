//! Business onboarding: Excel templates and reading uploaded workbooks (roadmap 99).
//!
//! One definition per dataset ([`columns`]) drives both the downloadable template (headers, required / optional,
//! help text, a sample row, dropdowns, codes kept as text) and reading an upload (headers matched to fields — also
//! under common alternative names — and values converted to text, numbers, dates or yes/no). Validation against the
//! business's data and posting live in `routes::onboarding`.

use std::collections::HashMap;
use std::io::Cursor;

use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};
use chrono::NaiveDate;
use rust_xlsxwriter::{DataValidation, Format, FormatAlign, Workbook};
use serde_json::{json, Map, Value};

use crate::error::{bad, AppResult};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Need {
    Required,
    Optional,
    /// Required in some cases (explained in the help text).
    Conditional,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Text,
    /// Kept exactly as typed (codes, barcodes, phone numbers): formatted as text in the template.
    Code,
    Money,
    Int,
    Date,
    YesNo,
}

pub struct Col {
    pub key: &'static str,
    pub header: &'static str,
    pub need: Need,
    pub kind: Kind,
    pub help: &'static str,
    pub example: &'static str,
    /// Dropdown values (also accepted when reading, case-insensitively).
    pub choices: &'static [&'static str],
    /// Other headers read as this field (column mapping without asking).
    pub aliases: &'static [&'static str],
}

const fn col(key: &'static str, header: &'static str, need: Need, kind: Kind, help: &'static str, example: &'static str) -> Col {
    Col { key, header, need, kind, help, example, choices: &[], aliases: &[] }
}

pub const DATASETS: &[(&str, &str)] = &[
    ("products", "Products"),
    ("customers", "Customers"),
    ("opening_stock", "Opening stock balances"),
    ("expenses", "Expenses"),
    ("sales", "Sales history"),
    ("credit", "Credit sales & balances"),
    ("loyalty", "Loyalty opening points"),
    ("orders", "Orders"),
    ("transfers", "Stock transfers"),
];

pub fn label(dataset: &str) -> Option<&'static str> {
    DATASETS.iter().find(|(k, _)| *k == dataset).map(|(_, l)| *l)
}

/// The fields of a dataset, matching the S'Shop records they create.
pub fn columns(dataset: &str) -> &'static [Col] {
    match dataset {
        "products" => &PRODUCTS,
        "customers" => &CUSTOMERS,
        "opening_stock" => &OPENING_STOCK,
        "expenses" => &EXPENSES,
        "sales" => &SALES,
        "credit" => &CREDIT,
        "loyalty" => &LOYALTY,
        "orders" => &ORDERS,
        "transfers" => &TRANSFERS,
        _ => &[],
    }
}

static PRODUCTS: [Col; 16] = [
    Col { aliases: &["name", "product", "item", "item name", "description of item"], ..col("name", "Product Name", Need::Required, Kind::Text, "The name shown at the till, on receipts and online.", "Black Perfume 100ml") },
    Col { aliases: &["code", "sku", "product code / sku", "item code"], ..col("code", "Product Code", Need::Optional, Kind::Code, "Your product code (SKU). Leave blank and S'Shop numbers it automatically (e.g. PRD0001). Must be unique. Links stock and sales rows to the product.", "BP100") },
    Col { aliases: &["category name", "group"], ..col("category", "Category", Need::Required, Kind::Text, "An existing category, or a new one (created on import).", "Fragrance") },
    Col { aliases: &["details"], ..col("description", "Description", Need::Optional, Kind::Text, "Shown on the ordering link and website.", "Long-lasting eau de parfum") },
    Col { aliases: &["short name", "alias"], ..col("nickname", "Nickname", Need::Optional, Kind::Text, "A short name the team uses.", "Black 100") },
    Col { aliases: &["price", "selling price (ksh)", "retail price", "marked price"], ..col("selling_price", "Selling Price", Need::Optional, Kind::Money, "The marked selling price. Leave blank when it is not set yet (Price on Request) — never 0 unless the product really is free.", "3500") },
    Col { aliases: &["cost", "buying price", "unit cost"], ..col("cost_price", "Cost Price", Need::Optional, Kind::Money, "What one unit costs you (for profit reports).", "2100") },
    Col { aliases: &["max discount"], ..col("max_discount", "Maximum Discount", Need::Optional, Kind::Money, "The largest discount a cashier may give without approval (an amount).", "200") },
    Col { aliases: &["barcode", "barcode no", "ean", "upc"], ..col("barcode", "Barcode Number", Need::Conditional, Kind::Code, "The manufacturer's barcode shared by all units (not the product code). Required when 'Barcode Required During Sale' is YES. Keep leading zeros.", "6291234567890") },
    Col { choices: &["YES", "NO"], ..col("barcode_required", "Barcode Required During Sale", Need::Optional, Kind::YesNo, "YES (default when barcodes are used): the product must have a barcode now. Whether sales must scan it follows Settings → Stock → Barcode settings.", "YES") },
    Col { choices: &["Product barcode", "Individual items"], aliases: &["tracking", "barcode tracking type"], ..col("tracking", "Stock Tracking", Need::Optional, Kind::Text, "Product barcode (default): one barcode for all units. Individual items: every unit gets its own barcode when stock is added.", "Product barcode") },
    Col { aliases: &["vendor"], ..col("supplier", "Supplier", Need::Optional, Kind::Text, "An existing supplier, or a new one (created on import).", "Scents Ltd") },
    Col { aliases: &["reorder level", "low stock alert", "reorder point"], ..col("low_stock", "Low-Stock Alert", Need::Optional, Kind::Int, "Alert when stock at a branch falls to this quantity.", "3") },
    Col { choices: &["Active", "Inactive"], ..col("status", "Status", Need::Optional, Kind::Text, "Active (default) or Inactive.", "Active") },
    Col { choices: &["YES", "NO"], aliases: &["available online", "online"], ..col("publish_online", "Publish Online", Need::Optional, Kind::YesNo, "YES (default): available on the ordering link and website.", "YES") },
    Col { aliases: &["branches", "branch"], ..col("branches", "Branch Applicability", Need::Optional, Kind::Text, "ALL (default) or branch codes separated by ; (e.g. MAIN;WL). Puts the product in those branches' catalogue — it never adds stock.", "ALL") },
];

static CUSTOMERS: [Col; 5] = [
    Col { aliases: &["mobile", "phone", "phone number", "mobile number", "customer reference", "telephone"], ..col("mobile", "Mobile Number", Need::Required, Kind::Code, "07…, 01… or 254…. Identifies the customer everywhere in S'Shop (sales, credit, orders, points).", "0712345678") },
    Col { aliases: &["first name", "name", "customer name"], ..col("first_name", "First Name", Need::Required, Kind::Text, "", "Amina") },
    Col { aliases: &["surname", "last name", "other name"], ..col("other_names", "Other Names", Need::Optional, Kind::Text, "", "Wanjiru") },
    Col { aliases: &["alias"], ..col("nickname", "Nickname", Need::Optional, Kind::Text, "", "Mama Ken") },
    Col { aliases: &["email address", "e-mail"], ..col("email", "Email", Need::Optional, Kind::Text, "", "amina@example.com") },
];

static OPENING_STOCK: [Col; 7] = [
    Col { aliases: &["product code", "code", "sku", "barcode", "item code"], ..col("product", "Product Code", Need::Required, Kind::Code, "The product's code (or its product barcode).", "BP100") },
    Col { aliases: &["branch", "branch code", "store", "location"], ..col("branch", "Branch Code", Need::Required, Kind::Code, "The branch's code (Settings → Branches).", "MAIN") },
    Col { aliases: &["quantity", "qty", "opening qty", "stock", "on hand"], ..col("quantity", "Opening Quantity", Need::Required, Kind::Int, "Units physically at that branch on the as-of date (0 or more).", "20") },
    Col { aliases: &["cost", "unit cost", "cost price"], ..col("unit_cost", "Unit Cost", Need::Optional, Kind::Money, "Cost of one unit (values the opening stock).", "2100") },
    Col { aliases: &["date", "as of", "as-of date", "stock date"], ..col("as_of", "As-of Date", Need::Required, Kind::Date, "The date the quantity was counted — normally the migration cutover date.", "2026-10-01") },
    Col { aliases: &["item barcodes", "serials", "serial numbers"], ..col("item_barcodes", "Item Barcodes", Need::Conditional, Kind::Text, "Individually tracked products only: each unit's barcode, separated by ; — as many as the quantity.", "") },
    Col { aliases: &["remarks", "comment"], ..col("notes", "Notes", Need::Optional, Kind::Text, "", "Counted by Peter") },
];

static EXPENSES: [Col; 8] = [
    Col { aliases: &["reference", "voucher", "voucher no", "ref"], ..col("reference", "Expense Reference", Need::Optional, Kind::Code, "Your reference (voucher number). Stops the same expense being imported twice.", "EXP-0091") },
    Col { aliases: &["date", "expense date", "paid on"], ..col("date", "Expense Date", Need::Required, Kind::Date, "The original date — kept as the expense date.", "2026-09-14") },
    Col { aliases: &["branch", "branch code", "store"], ..col("branch", "Branch Code", Need::Required, Kind::Code, "The branch's code (Settings → Branches).", "MAIN") },
    Col { aliases: &["category", "expense category", "type"], ..col("category", "Category", Need::Required, Kind::Text, "An existing expense category, or a new one (created on import).", "Rent") },
    Col { aliases: &["details", "narration", "particulars"], ..col("description", "Description", Need::Optional, Kind::Text, "", "September rent") },
    Col { aliases: &["amount (ksh)", "total", "value"], ..col("amount", "Amount", Need::Required, Kind::Money, "More than 0.", "45000") },
    Col { aliases: &["payment", "paid by", "mode of payment", "payment mode"], ..col("payment_method", "Payment Method", Need::Optional, Kind::Text, "One of your payment methods (Settings → Sales & payments); default Cash.", "Cash") },
    Col { aliases: &["supplier", "paid to", "vendor"], ..col("payee", "Supplier / Payee", Need::Optional, Kind::Text, "", "Landlord") },
];

static SALES: [Col; 12] = [
    Col { aliases: &["receipt", "receipt no", "receipt number", "invoice", "invoice no", "reference", "sale reference"], ..col("receipt", "Receipt Reference", Need::Required, Kind::Code, "Your original receipt number. Lines with the same reference are one sale (same date, branch, payment and customer). Kept as the sale's receipt number.", "INV-2025-0412") },
    Col { aliases: &["date", "sale date", "transaction date"], ..col("date", "Sale Date", Need::Required, Kind::Date, "The original date (before the migration cutover) — reports show the sale on this date.", "2025-11-14") },
    Col { aliases: &["branch", "branch code", "store"], ..col("branch", "Branch Code", Need::Required, Kind::Code, "The branch's code (Settings → Branches).", "MAIN") },
    Col { aliases: &["product code", "code", "sku", "item code", "barcode"], ..col("product", "Product Code", Need::Required, Kind::Code, "The product's code (or product barcode) — import products first.", "BP100") },
    Col { aliases: &["qty", "quantity sold"], ..col("quantity", "Quantity", Need::Required, Kind::Int, "Units sold on this line (more than 0).", "2") },
    Col { aliases: &["unit price", "price", "selling price"], ..col("unit_price", "Unit Selling Price", Need::Required, Kind::Money, "The price actually charged for one unit (after discount).", "3300") },
    Col { aliases: &["discount amount", "line discount"], ..col("discount", "Discount", Need::Optional, Kind::Money, "Total discount on this line (an amount), if any.", "400") },
    Col { aliases: &["payment", "paid by", "mode of payment", "payment mode"], ..col("payment_method", "Payment Method", Need::Required, Kind::Text, "One of your payment methods (Settings → Sales & payments). Credit sales go in the Credit sales template instead.", "Cash") },
    Col { aliases: &["mpesa code", "transaction code", "payment ref"], ..col("payment_reference", "Payment Reference", Need::Optional, Kind::Code, "e.g. the M-Pesa code. Recorded only — no payment is requested.", "QFT1ABC2DE") },
    Col { aliases: &["customer", "customer mobile", "mobile", "phone"], ..col("customer", "Customer Mobile", Need::Optional, Kind::Code, "An existing customer's mobile (import customers first).", "0712345678") },
    Col { aliases: &["salesperson", "sold by", "cashier", "sale owner"], ..col("salesperson", "Salesperson Email", Need::Optional, Kind::Text, "Sign-in email of the team member credited with the sale.", "peter@example.com") },
    Col { aliases: &["remarks", "comment"], ..col("notes", "Notes", Need::Optional, Kind::Text, "", "") },
];

static CREDIT: [Col; 11] = [
    Col { aliases: &["reference", "credit ref", "invoice", "invoice no", "receipt", "receipt reference"], ..col("reference", "Credit Reference", Need::Required, Kind::Code, "Your original invoice / receipt number for the credit sale (unique). Kept as its receipt number.", "CR-0091") },
    Col { aliases: &["customer", "mobile", "phone", "customer reference"], ..col("customer", "Customer Mobile", Need::Required, Kind::Code, "An existing customer's mobile (import customers first).", "0712345678") },
    Col { aliases: &["branch", "branch code", "store"], ..col("branch", "Branch Code", Need::Required, Kind::Code, "The branch's code.", "MAIN") },
    Col { aliases: &["sale date", "date", "original date"], ..col("sale_date", "Original Sale Date", Need::Required, Kind::Date, "When the credit sale was made (before the migration cutover).", "2026-08-02") },
    Col { aliases: &["due", "due on"], ..col("due_date", "Due Date", Need::Required, Kind::Date, "The original due date (aging is counted from it).", "2026-09-01") },
    Col { aliases: &["amount", "original amount", "credit amount", "total"], ..col("original_amount", "Original Amount", Need::Required, Kind::Money, "The full credit sale amount.", "12000") },
    Col { aliases: &["paid", "amount paid so far", "payments"], ..col("amount_paid", "Amount Paid", Need::Optional, Kind::Money, "Paid before S'Shop (0 if nothing). The outstanding balance is Original − Paid.", "5000") },
    Col { aliases: &["product code", "code", "sku"], ..col("product", "Product Code", Need::Optional, Kind::Code, "The product sold, if known. Leave blank to import an Opening Credit Balance (no item history).", "") },
    Col { aliases: &["qty"], ..col("quantity", "Quantity", Need::Optional, Kind::Int, "Units of that product (default 1).", "") },
    Col { aliases: &["salesperson", "sold by", "sale owner"], ..col("salesperson", "Salesperson Email", Need::Optional, Kind::Text, "Sign-in email of the team member credited with the sale.", "") },
    Col { aliases: &["remarks", "comment"], ..col("notes", "Notes", Need::Optional, Kind::Text, "", "") },
];

static LOYALTY: [Col; 7] = [
    Col { aliases: &["customer", "mobile", "phone", "customer reference", "customer mobile"], ..col("customer", "Customer Mobile", Need::Required, Kind::Code, "An existing customer's mobile (import customers first).", "0712345678") },
    Col { aliases: &["balance", "points", "current points", "current points balance", "points available"], ..col("balance", "Points Balance", Need::Required, Kind::Int, "Points the customer can use on the effective date (0 or more).", "350") },
    Col { aliases: &["earned", "points earned", "total earned"], ..col("earned", "Points Earned", Need::Optional, Kind::Int, "Lifetime points earned before S'Shop (give with Points Redeemed; Earned − Redeemed must equal the balance).", "500") },
    Col { aliases: &["redeemed", "points redeemed", "total redeemed"], ..col("redeemed", "Points Redeemed", Need::Optional, Kind::Int, "Lifetime points redeemed before S'Shop.", "150") },
    Col { aliases: &["date", "as of", "effective"], ..col("effective", "Effective Date", Need::Required, Kind::Date, "The date of the balance — normally the migration cutover.", "2026-10-01") },
    Col { aliases: &["expiry", "expires", "expires on"], ..col("expiry", "Expiry Date", Need::Optional, Kind::Date, "When these points expire, if they do.", "") },
    Col { aliases: &["remarks", "comment"], ..col("notes", "Notes", Need::Optional, Kind::Text, "", "") },
];

static ORDERS: [Col; 10] = [
    Col { aliases: &["order", "order no", "order number", "reference"], ..col("order", "Order Reference", Need::Required, Kind::Code, "Your original order number. Lines with the same reference are one order. Kept as the order number.", "ORD-0311") },
    Col { aliases: &["date", "order date", "ordered on"], ..col("date", "Order Date", Need::Required, Kind::Date, "The original order date.", "2026-09-20") },
    Col { aliases: &["customer", "mobile", "phone", "customer reference", "customer mobile"], ..col("customer", "Customer Mobile", Need::Required, Kind::Code, "An existing customer's mobile (import customers first).", "0712345678") },
    Col { aliases: &["branch", "branch code", "store"], ..col("branch", "Branch Code", Need::Required, Kind::Code, "The branch that fulfils the order.", "MAIN") },
    Col { aliases: &["product code", "code", "sku", "barcode"], ..col("product", "Product Code", Need::Required, Kind::Code, "The product's code (or product barcode).", "BP100") },
    Col { aliases: &["qty"], ..col("quantity", "Quantity", Need::Required, Kind::Int, "Units ordered (more than 0).", "1") },
    Col { aliases: &["price", "unit price"], ..col("unit_price", "Unit Price", Need::Required, Kind::Money, "Price of one unit on the order.", "3500") },
    Col { choices: &["New", "Confirmed", "Preparing", "Dispatched", "On delivery", "Delivered", "Completed", "Cancelled", "Rejected", "Returned"], aliases: &["status"], ..col("status", "Order Status", Need::Required, Kind::Text, "The status today. Orders still in progress go into Orders (confirmed ones hold stock); delivered / completed / cancelled ones are history and create no sale — import their sales in Sales history.", "Confirmed") },
    Col { aliases: &["delivery", "delivery details", "address", "location"], ..col("delivery", "Delivery Location", Need::Optional, Kind::Text, "", "Kilimani") },
    Col { aliases: &["remarks", "comment"], ..col("notes", "Notes", Need::Optional, Kind::Text, "", "") },
];

static TRANSFERS: [Col; 10] = [
    Col { aliases: &["transfer", "transfer no", "transfer number", "reference"], ..col("transfer", "Transfer Reference", Need::Required, Kind::Code, "Your original transfer number. Lines with the same reference are one transfer.", "TRF-0042") },
    Col { aliases: &["from", "from branch", "origin", "origin branch"], ..col("from_branch", "Origin Branch Code", Need::Required, Kind::Code, "The branch the stock left.", "MAIN") },
    Col { aliases: &["to", "to branch", "destination", "destination branch"], ..col("to_branch", "Destination Branch Code", Need::Required, Kind::Code, "The branch receiving it.", "WL") },
    Col { aliases: &["product code", "code", "sku", "barcode"], ..col("product", "Product Code", Need::Required, Kind::Code, "The product's code (or product barcode).", "BP100") },
    Col { aliases: &["qty", "quantity", "quantity sent", "dispatched"], ..col("dispatched", "Quantity Dispatched", Need::Required, Kind::Int, "Units sent (more than 0).", "5") },
    Col { aliases: &["received", "quantity received"], ..col("received", "Quantity Received", Need::Conditional, Kind::Int, "Received transfers: units that arrived (the rest is recorded as short). Leave blank for the full quantity.", "5") },
    Col { aliases: &["date", "transfer date", "sent on"], ..col("date", "Transfer Date", Need::Required, Kind::Date, "When it was sent.", "2026-09-10") },
    Col { aliases: &["receipt date", "received on", "date received"], ..col("received_date", "Receipt Date", Need::Conditional, Kind::Date, "Received transfers: when it arrived.", "2026-09-11") },
    Col { choices: &["Received", "In transit", "Pending", "Cancelled"], aliases: &["status"], ..col("status", "Transfer Status", Need::Required, Kind::Text, "Received / Cancelled = history (no stock moves: opening stock already reflects it). In transit = sent before the cutover, not yet arrived: receive it in S'Shop (it is in neither branch's opening stock). Pending = not sent: dispatch it in S'Shop.", "Received") },
    Col { aliases: &["remarks", "comment"], ..col("notes", "Notes", Need::Optional, Kind::Text, "", "") },
];

fn norm(s: &str) -> String {
    s.trim().to_lowercase().replace(['_', '*'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Which field a header means (its own name, the template header, or an alternative name).
pub fn field_for(dataset: &str, header: &str) -> Option<&'static str> {
    let h = norm(header.trim_end_matches(" (required)").trim_end_matches(" (optional)"));
    columns(dataset)
        .iter()
        .find(|c| norm(c.header) == h || norm(c.key) == h || c.aliases.iter().any(|a| norm(a) == h))
        .map(|c| c.key)
}

// ── Templates ───────────────────────────────────────────────────────────────────────────────────────────────────────

/// An .xlsx template: the data sheet (headers marked required / optional, a sample row, codes as text, dropdowns) and an
/// Instructions sheet explaining every field and how the templates link to each other.
pub fn template(dataset: &str, choices_extra: &HashMap<&'static str, Vec<String>>) -> AppResult<Vec<u8>> {
    let cols = columns(dataset);
    let title = label(dataset).ok_or_else(|| bad("Unknown dataset"))?;
    let x = |e: rust_xlsxwriter::XlsxError| crate::error::AppError::Other(anyhow::anyhow!(e));
    let mut wb = Workbook::new();
    let head_req = Format::new().set_bold().set_background_color("F9E3CF").set_border(rust_xlsxwriter::FormatBorder::Thin);
    let head_opt = Format::new().set_bold().set_background_color("F3EEE7").set_border(rust_xlsxwriter::FormatBorder::Thin);
    let text = Format::new().set_num_format("@");
    let money = Format::new().set_num_format("#,##0.00");
    let date = Format::new().set_num_format("yyyy-mm-dd");
    let sample = Format::new().set_italic().set_font_color("8A7A6C");
    {
        let ws = wb.add_worksheet().set_name(title.chars().take(31).collect::<String>()).map_err(x)?;
        for (i, c) in cols.iter().enumerate() {
            let i = i as u16;
            let req = c.need == Need::Required;
            ws.write_string_with_format(0, i, format!("{}{}", c.header, if req { " *" } else { "" }), if req { &head_req } else { &head_opt }).map_err(x)?;
            ws.set_column_width(i, (c.header.len() as f64 + 6.0).clamp(12.0, 34.0)).map_err(x)?;
            match c.kind {
                Kind::Code => {
                    ws.set_column_format(i, &text).map_err(x)?;
                }
                Kind::Money => {
                    ws.set_column_format(i, &money).map_err(x)?;
                }
                Kind::Date => {
                    ws.set_column_format(i, &date).map_err(x)?;
                }
                _ => {}
            }
            // Row 2: the sample (to be replaced or deleted).
            ws.write_string_with_format(1, i, c.example, &sample).map_err(x)?;
            let list: Vec<String> = match choices_extra.get(c.key) {
                Some(v) if !v.is_empty() => v.clone(),
                _ => c.choices.iter().map(|s| s.to_string()).collect(),
            };
            if !list.is_empty() && list.iter().map(|s| s.len() + 1).sum::<usize>() < 250 {
                let dv = DataValidation::new().allow_list_strings(&list).map_err(x)?.ignore_blank(true);
                ws.add_data_validation(1, i, 5000, i, &dv).map_err(x)?;
            }
        }
        ws.set_freeze_panes(1, 0).map_err(x)?;
    }
    {
        let ws = wb.add_worksheet().set_name("Instructions").map_err(x)?;
        let bold = Format::new().set_bold();
        let wrap = Format::new().set_text_wrap().set_align(FormatAlign::Top);
        ws.set_column_width(0, 30).map_err(x)?;
        ws.set_column_width(1, 14).map_err(x)?;
        ws.set_column_width(2, 90).map_err(x)?;
        ws.write_string_with_format(0, 0, format!("S'Shop — {title} template"), &bold).map_err(x)?;
        let intro = [
            "Fill one row per record on the first sheet, from row 2 (replace the grey sample row). Keep the header row.",
            "Columns marked * are required. Codes, barcodes and phone numbers are text columns: leading zeros are kept.",
            "Linking templates: products are referred to by their Product Code, branches by their Branch Code, customers by their Mobile Number.",
            "Upload in Settings → Onboarding. Nothing is saved until the preview is validated and the batch is posted (approval may apply).",
        ];
        for (r, s) in intro.iter().enumerate() {
            ws.write_string_with_format(r as u32 + 1, 0, *s, &wrap).map_err(x)?;
        }
        let top = intro.len() as u32 + 2;
        ws.write_string_with_format(top, 0, "Field", &bold).map_err(x)?;
        ws.write_string_with_format(top, 1, "Requirement", &bold).map_err(x)?;
        ws.write_string_with_format(top, 2, "What to enter", &bold).map_err(x)?;
        for (r, c) in cols.iter().enumerate() {
            let r = top + 1 + r as u32;
            ws.write_string(r, 0, c.header).map_err(x)?;
            ws.write_string(r, 1, match c.need {
                Need::Required => "Required",
                Need::Optional => "Optional",
                Need::Conditional => "Conditional",
            })
            .map_err(x)?;
            let fmt = match c.kind {
                Kind::Money => " Amount (numbers only).",
                Kind::Int => " Whole number.",
                Kind::Date => " Date (e.g. 2026-10-01).",
                Kind::YesNo => " YES or NO.",
                _ => "",
            };
            let choices = if c.choices.is_empty() { String::new() } else { format!(" Choices: {}.", c.choices.join(", ")) };
            ws.write_string_with_format(r, 2, format!("{}{fmt}{choices}", c.help), &wrap).map_err(x)?;
        }
    }
    wb.save_to_buffer().map_err(x)
}

// ── Reading an upload ───────────────────────────────────────────────────────────────────────────────────────────────

pub struct Sheet {
    pub headers: Vec<String>,
    /// header → field (detected or given)
    pub mapping: HashMap<String, String>,
    /// (Excel row number, values by field — normalised, and as read)
    pub rows: Vec<(u32, Map<String, Value>, Map<String, Value>)>,
}

pub const MAX_ROWS: usize = 5000;

/// Reads the first data sheet (not "Instructions"): headers on row 1; blank rows and the template's sample row are
/// skipped. `given` overrides the detected column mapping (source header → field key, "" = ignore the column).
pub fn read(dataset: &str, bytes: &[u8], given: &HashMap<String, String>) -> AppResult<Sheet> {
    let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes)).map_err(|_| bad("This is not an Excel (.xlsx) workbook"))?;
    let name = wb
        .sheet_names()
        .into_iter()
        .find(|n| !n.eq_ignore_ascii_case("instructions"))
        .ok_or_else(|| bad("The workbook has no data sheet"))?;
    let range = wb.worksheet_range(&name).map_err(|_| bad("The data sheet cannot be read"))?;
    let mut rows = range.rows();
    let head: Vec<String> = rows.next().ok_or_else(|| bad("The sheet is empty"))?.iter().map(|c| cell_text(c, false).trim().trim_end_matches('*').trim().to_string()).collect();
    let cols = columns(dataset);
    let mut mapping = HashMap::new();
    for h in head.iter().filter(|h| !h.is_empty()) {
        let m = match given.get(h) {
            Some(k) if k.is_empty() => continue,
            Some(k) if cols.iter().any(|c| c.key == k) => k.clone(),
            Some(_) => continue,
            None => match field_for(dataset, h) {
                Some(k) => k.to_string(),
                None => continue,
            },
        };
        mapping.insert(h.clone(), m);
    }
    let missing: Vec<&str> = cols.iter().filter(|c| c.need == Need::Required && !mapping.values().any(|v| v == c.key)).map(|c| c.header).collect();
    if !missing.is_empty() {
        return Err(bad(format!("Required column(s) not found: {} — map them to the workbook's columns", missing.join(", "))));
    }
    let sample: Vec<&str> = cols.iter().map(|c| c.example).collect();
    let mut out = Vec::new();
    for (i, r) in rows.enumerate() {
        let excel_row = i as u32 + 2;
        let mut data = Map::new();
        let mut source = Map::new();
        let mut typed: HashMap<&str, String> = HashMap::new();
        let mut blank = true;
        for (j, cell) in r.iter().enumerate() {
            let Some(h) = head.get(j) else { continue };
            let Some(key) = mapping.get(h) else { continue };
            let c = cols.iter().find(|c| c.key == key).expect("mapped");
            let raw = cell_text(cell, c.kind == Kind::Code);
            if !raw.trim().is_empty() {
                blank = false;
            }
            source.insert(h.clone(), Value::String(raw.clone()));
            typed.insert(c.key, raw.trim().to_string());
            data.insert(key.clone(), convert(c, cell, &raw));
        }
        if blank {
            continue;
        }
        // The template's grey sample row, left in place.
        if excel_row == 2 && cols.iter().zip(&sample).all(|(c, s)| typed.get(c.key).map_or(true, |v| v.is_empty() || v == s)) {
            continue;
        }
        out.push((excel_row, data, source));
        if out.len() > MAX_ROWS {
            return Err(bad(format!("Up to {MAX_ROWS} rows per upload — split the workbook")));
        }
    }
    if out.is_empty() {
        return Err(bad("No rows to import (row 1 holds the headers)"));
    }
    Ok(Sheet { headers: head.into_iter().filter(|h| !h.is_empty()).collect(), mapping, rows: out })
}

/// A cell as text. Numbers typed into a code column come back without ".0" (leading zeros are lost only if the cell
/// was not formatted as text in the first place — the template's code columns are).
fn cell_text(c: &Data, code: bool) -> String {
    match c {
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) if code => format!("{f}"),
        Data::Float(f) => format!("{f}"),
        Data::Bool(b) => if *b { "YES" } else { "NO" }.into(),
        Data::DateTime(d) => d.as_datetime().map(|t| t.date().to_string()).unwrap_or_default(),
        Data::Error(_) | Data::Empty => String::new(),
    }
}

/// Converts to the field's type; anything unreadable stays as text for validation to explain.
fn convert(c: &Col, cell: &Data, raw: &str) -> Value {
    let t = raw.trim();
    if t.is_empty() {
        return Value::Null;
    }
    match c.kind {
        Kind::Money => t.replace([',', ' '], "").trim_start_matches("KSh").trim_start_matches("Ksh").trim().parse::<f64>().map(|v| json!(v)).unwrap_or_else(|_| json!({ "invalid": t })),
        Kind::Int => t.replace(',', "").parse::<f64>().ok().filter(|v| v.fract() == 0.0).map(|v| json!(v as i64)).unwrap_or_else(|| json!({ "invalid": t })),
        Kind::Date => match cell {
            Data::DateTime(d) => d.as_datetime().map(|x| json!(x.date().to_string())).unwrap_or_else(|| json!({ "invalid": t })),
            _ => parse_date(t).map(|d| json!(d.to_string())).unwrap_or_else(|| json!({ "invalid": t })),
        },
        Kind::YesNo => match t.to_lowercase().as_str() {
            "yes" | "y" | "true" | "1" => json!(true),
            "no" | "n" | "false" | "0" => json!(false),
            _ => json!({ "invalid": t }),
        },
        Kind::Text | Kind::Code => json!(t),
    }
}

/// 2026-10-01, 01/10/2026 (day first, as in Kenya), 1-Oct-2026, 1 Oct 2026.
pub fn parse_date(t: &str) -> Option<NaiveDate> {
    ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y", "%d-%b-%Y", "%d %b %Y", "%d %B %Y", "%Y/%m/%d"]
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(t.trim(), f).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_open_and_read_back() {
        for (d, _) in DATASETS {
            let bytes = template(d, &HashMap::new()).unwrap();
            // The untouched template has only the sample row: nothing to import.
            let err = read(d, &bytes, &HashMap::new()).err().expect("sample row is not data");
            assert!(err.to_string().contains("No rows"), "{d}: {err}");
        }
    }

    #[test]
    fn headers_map_by_alternative_names() {
        assert_eq!(field_for("products", "SKU"), Some("code"));
        assert_eq!(field_for("products", "Selling Price *"), Some("selling_price"), "the required marker is ignored");
        assert_eq!(field_for("products", "selling price"), Some("selling_price"));
        assert_eq!(field_for("customers", "Phone Number"), Some("mobile"));
        assert_eq!(field_for("expenses", "Voucher No"), Some("reference"));
        assert_eq!(field_for("opening_stock", "Qty"), Some("quantity"));
        assert_eq!(field_for("products", "Colour"), None);
    }

    #[test]
    fn values_converted() {
        let money = &PRODUCTS[5];
        assert_eq!(convert(money, &Data::Empty, "KSh 3,500"), json!(3500.0));
        assert_eq!(convert(money, &Data::Empty, "abc"), json!({ "invalid": "abc" }));
        let d = &EXPENSES[1];
        assert_eq!(convert(d, &Data::Empty, "14/09/2026"), json!("2026-09-14"));
        assert_eq!(cell_text(&Data::Float(6291234567890.0), true), "6291234567890");
        assert_eq!(cell_text(&Data::String(" 0712345678 ".into()), true), "0712345678");
    }
}
