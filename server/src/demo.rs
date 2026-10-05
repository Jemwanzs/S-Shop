//! Pablo Niche demo business: a premium watches / necklaces / perfumes retailer with ~5 months of trading.
//!
//! Everything is created through S'Shop's own HTTP API, called in-process, so the inventory ledger, loyalty,
//! credit, orders, approvals and audit trail behave exactly as they do for real users. After each call the rows
//! that call created are moved to the event's date ("stamping"), which is how history is spread over the past
//! months. Safety:
//! - only ever touches a business flagged `is_demo`; reruns are no-ops unless `reset` is asked for, and reset
//!   deletes only that demo business;
//! - demo staff get random PINs (nobody can sign in as them; platform admins open the business instead);
//! - WhatsApp is never sent for demo businesses, M-Pesa references are marked `DEMO…` (no real payments);
//! - product barcodes use the GS1 restricted range (prefix 2), item labels `PN-…`: never real product codes.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Context as _};
use axum::body::Body;
use axum::http::{Method, Request};
use axum::Router;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::auth::issue_token;
use crate::state::AppState;

pub const DEMO_SLUG: &str = "pablo-niche-demo";
pub const DEMO_NAME: &str = "Pablo Niche (Demo)";
const TZ: chrono_tz::Tz = chrono_tz::Africa::Nairobi;
const HISTORY_DAYS: i64 = 150;

#[derive(Debug, Default, Clone, Serialize)]
pub struct DemoReport {
    pub tenant_id: Option<Uuid>,
    pub created: bool,
    pub branches: usize,
    pub products: usize,
    pub customers: usize,
    pub sales: usize,
    pub orders: usize,
    pub transfers: usize,
    pub expenses: usize,
    pub returns: usize,
    pub photos: usize,
    pub photos_note: String,
    pub notes: Vec<String>,
}

/// Seeds the demo business. Without `reset` an existing demo business is left untouched.
pub async fn seed(state: &AppState, reset: bool, progress: impl Fn(&str)) -> anyhow::Result<DemoReport> {
    let existing: Option<(Uuid, bool)> = sqlx::query_as("SELECT id, is_demo FROM tenants WHERE slug = $1")
        .bind(DEMO_SLUG)
        .fetch_optional(&state.db)
        .await?;
    if let Some((id, is_demo)) = existing {
        if !is_demo {
            bail!("A business with slug {DEMO_SLUG} exists and is not a demo business — refusing to touch it");
        }
        if !reset {
            return Ok(DemoReport { tenant_id: Some(id), notes: vec!["The demo business already exists — use reset to rebuild it".into()], ..Default::default() });
        }
        progress("Removing the previous demo business");
        purge(&state.db, id).await.context("removing the previous demo business")?;
    }
    let mut s = Seeder::new(state).await?;
    s.run(&progress).await?;
    Ok(s.report)
}

/// Deletes a demo business and everything in it, in one transaction. A single `DELETE FROM tenants` cannot do
/// it: cascades run in an order where e.g. products go before the transfer lines that reference them. So every
/// table holding the business's rows is emptied, retrying tables still referenced until their dependents are gone.
async fn purge(db: &PgPool, tenant: Uuid) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    let demo: bool = sqlx::query_scalar("SELECT is_demo FROM tenants WHERE id = $1 FOR UPDATE").bind(tenant).fetch_one(&mut *tx).await?;
    if !demo {
        bail!("refusing to delete a business that is not a demo");
    }
    let mut tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.table_name::text FROM information_schema.columns c JOIN information_schema.tables t
           ON t.table_name = c.table_name AND t.table_schema = c.table_schema AND t.table_type = 'BASE TABLE'
         WHERE c.table_schema = 'public' AND c.column_name = 'tenant_id' AND c.table_name <> 'access_requests'",
    )
    .fetch_all(&mut *tx)
    .await?;
    // Break reference loops (an order points to its sale and the sale back to its order) by clearing every
    // optional foreign key inside the business first.
    let nullable_fks: Vec<(String, String)> = sqlx::query_as(
        "SELECT kcu.table_name::text, kcu.column_name::text
         FROM information_schema.table_constraints tc
         JOIN information_schema.key_column_usage kcu ON kcu.constraint_name = tc.constraint_name AND kcu.table_schema = tc.table_schema
         JOIN information_schema.columns col ON col.table_name = kcu.table_name AND col.column_name = kcu.column_name AND col.table_schema = kcu.table_schema
         WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = 'public' AND col.is_nullable = 'YES'
           AND kcu.column_name <> 'tenant_id' AND kcu.table_name = ANY($1)",
    )
    .bind(&tables)
    .fetch_all(&mut *tx)
    .await?;
    for (table, column) in nullable_fks {
        sqlx::query("SAVEPOINT purge_null").execute(&mut *tx).await?;
        let ok = sqlx::query(&format!("UPDATE {table} SET {column} = NULL WHERE tenant_id = $1 AND {column} IS NOT NULL"))
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .is_ok();
        sqlx::query(if ok { "RELEASE SAVEPOINT purge_null" } else { "ROLLBACK TO SAVEPOINT purge_null" }).execute(&mut *tx).await?;
    }
    for _pass in 0..12 {
        let mut blocked = vec![];
        for table in &tables {
            sqlx::query("SAVEPOINT purge_step").execute(&mut *tx).await?;
            match sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1")).bind(tenant).execute(&mut *tx).await {
                Ok(_) => {
                    sqlx::query("RELEASE SAVEPOINT purge_step").execute(&mut *tx).await?;
                }
                Err(_) => {
                    sqlx::query("ROLLBACK TO SAVEPOINT purge_step").execute(&mut *tx).await?;
                    blocked.push(table.clone());
                }
            }
        }
        if blocked.is_empty() {
            break;
        }
        tables = blocked;
    }
    sqlx::query("DELETE FROM tenants WHERE id = $1 AND is_demo").bind(tenant).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

// ───────────────────────────── In-process API client ─────────────────────────────

struct Api {
    app: Router,
    db: PgPool,
    tenant: Uuid,
    secret: String,
}

impl Api {
    fn token(&self, user: Uuid) -> String {
        issue_token(&self.secret, user, self.tenant, "staff", Duration::hours(2)).expect("token")
    }

    async fn call(&self, user: Uuid, branch: Option<Uuid>, method: Method, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api{path}"))
            .header("authorization", format!("Bearer {}", self.token(user)))
            .header("content-type", "application/json")
            .header("x-forwarded-for", "127.0.0.1")
            .header("user-agent", "sshop-demo-seed");
        if let Some(b) = branch {
            req = req.header("x-branch-id", b.to_string());
        }
        let req = req.body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))?;
        let res = self.app.clone().oneshot(req).await.map_err(|e| anyhow!("{e}"))?;
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 20 * 1024 * 1024).await?;
        let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("{method} {path} → {status}: {}", v["error"]["message"].as_str().unwrap_or(&String::from_utf8_lossy(&bytes)));
        }
        Ok(v)
    }

    async fn post(&self, user: Uuid, branch: Option<Uuid>, path: &str, body: Value) -> anyhow::Result<Value> {
        self.call(user, branch, Method::POST, path, Some(body)).await
    }

    /// Database clock just before a call: rows created by the call have created_at >= this.
    async fn mark(&self) -> anyhow::Result<DateTime<Utc>> {
        Ok(sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&self.db).await?)
    }

    /// Moves everything created since `since` to `at` (keeping due dates and expiries relative).
    async fn stamp(&self, since: DateTime<Utc>, at: DateTime<Utc>) -> anyhow::Result<()> {
        let t = self.tenant;
        let mut tx = self.db.begin().await?;
        for table in [
            "sales", "payments", "credit_sales", "sale_returns", "orders", "transfers", "expenses", "stock_adjustments", "approvals",
            "audit_log", "referrals", "notifications", "customers", "mpesa_requests",
        ] {
            sqlx::query(&format!("UPDATE {table} SET created_at = $3 WHERE tenant_id = $1 AND created_at >= $2"))
                .bind(t)
                .bind(since)
                .bind(at)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "UPDATE stock_movements SET created_at = $3, occurred_on = ($3 AT TIME ZONE 'Africa/Nairobi')::date
             WHERE tenant_id = $1 AND created_at >= $2",
        )
        .bind(t)
        .bind(since)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE loyalty_ledger SET expires_at = expires_at - (created_at - $3), created_at = $3 WHERE tenant_id = $1 AND created_at >= $2")
            .bind(t)
            .bind(since)
            .bind(at)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE stock_items SET created_at = $3, updated_at = $3 WHERE tenant_id = $1 AND created_at >= $2")
            .bind(t)
            .bind(since)
            .bind(at)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE stock_items SET updated_at = $3 WHERE tenant_id = $1 AND updated_at >= $2")
            .bind(t)
            .bind(since)
            .bind(at)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE order_events e SET created_at = $3 FROM orders o WHERE o.id = e.order_id AND o.tenant_id = $1 AND e.created_at >= $2")
            .bind(t)
            .bind(since)
            .bind(at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

// ───────────────────────────── Catalogue definition ─────────────────────────────

struct ProductDef {
    name: &'static str,
    category: usize, // 0 watches, 1 necklaces, 2 perfumes
    price: i64,
    tracked: bool,
    /// Relative popularity (higher sells more often).
    weight: u32,
    /// Opening stock per branch [main, westlands, village, two rivers]; tracked items are individual units.
    stock: [i32; 4],
}

const PRODUCTS: &[ProductDef] = &[
    ProductDef { name: "Imperial Chronograph", category: 0, price: 285_000, tracked: true, weight: 3, stock: [4, 3, 2, 0] },
    ProductDef { name: "Heritage Automatic", category: 0, price: 198_000, tracked: true, weight: 4, stock: [5, 2, 3, 2] },
    ProductDef { name: "Royal Classic Watch", category: 0, price: 125_000, tracked: true, weight: 6, stock: [6, 4, 3, 3] },
    ProductDef { name: "Executive Steel Watch", category: 0, price: 89_500, tracked: true, weight: 7, stock: [8, 5, 4, 4] },
    ProductDef { name: "Midnight Chronograph", category: 0, price: 164_000, tracked: true, weight: 4, stock: [0, 4, 0, 2] },
    ProductDef { name: "Signature Gold Watch", category: 0, price: 420_000, tracked: true, weight: 1, stock: [2, 1, 1, 0] },
    ProductDef { name: "Royal Gold Necklace", category: 1, price: 245_000, tracked: true, weight: 3, stock: [3, 2, 2, 1] },
    ProductDef { name: "Signature Silver Necklace", category: 1, price: 48_500, tracked: false, weight: 8, stock: [14, 9, 6, 6] },
    ProductDef { name: "Imperial Pendant Necklace", category: 1, price: 78_000, tracked: false, weight: 6, stock: [10, 6, 5, 3] },
    ProductDef { name: "Classic Pearl Necklace", category: 1, price: 112_000, tracked: false, weight: 4, stock: [6, 4, 2, 2] },
    ProductDef { name: "Luxury Chain Necklace", category: 1, price: 64_000, tracked: false, weight: 6, stock: [9, 7, 5, 4] },
    ProductDef { name: "Diamond-Style Pendant", category: 1, price: 156_000, tracked: true, weight: 2, stock: [2, 2, 1, 1] },
    ProductDef { name: "Pablo Noir", category: 2, price: 18_500, tracked: false, weight: 14, stock: [40, 24, 18, 16] },
    ProductDef { name: "Imperial Oud", category: 2, price: 32_000, tracked: false, weight: 10, stock: [26, 16, 12, 10] },
    ProductDef { name: "Royal Essence", category: 2, price: 14_500, tracked: false, weight: 12, stock: [34, 20, 16, 14] },
    ProductDef { name: "Midnight Reserve", category: 2, price: 24_000, tracked: false, weight: 9, stock: [22, 14, 10, 8] },
    ProductDef { name: "Signature Intense", category: 2, price: 21_500, tracked: false, weight: 9, stock: [3, 18, 12, 10] },
    ProductDef { name: "Amber Prestige", category: 2, price: 27_500, tracked: false, weight: 7, stock: [18, 0, 8, 6] },
    // Inactive: kept in the catalogue but no longer sold.
    ProductDef { name: "Vintage Leather Watch", category: 0, price: 54_000, tracked: false, weight: 0, stock: [0, 0, 0, 0] },
];

const CATEGORIES: [&str; 3] = ["Watches", "Necklaces", "Perfumes"];
const SUPPLIERS: [(&str, &str); 3] = [
    ("Swiss Time Distributors (demo)", "0700 000 901"),
    ("Gold Coast Jewellers (demo)", "0700 000 902"),
    ("Essence Fragrance Imports (demo)", "0700 000 903"),
];
const BRANCHES: [(&str, &str, &str); 3] = [("Westlands", "WLD", "Westlands, Nairobi"), ("Village Market", "VMK", "Gigiri, Nairobi"), ("Two Rivers", "TRV", "Ruaka, Nairobi")];

const FIRST: &[&str] = &[
    "Amani", "Wanjiru", "Brian", "Achieng", "Kevin", "Njeri", "Dennis", "Atieno", "Collins", "Wairimu", "Faith", "Otieno", "Mercy", "Kiprono",
    "Grace", "Mwangi", "Linda", "Omondi", "Joy", "Kamau", "Sharon", "Mutua", "Esther", "Kibet", "Diana", "Odhiambo", "Ivy", "Njoroge", "Ruth",
    "Cheruiyot", "Naomi", "Wekesa", "Lucy", "Barasa", "Tabitha", "Gitau", "Stella", "Onyango", "Beatrice", "Rotich", "Caroline", "Maina",
];
const LAST: &[&str] = &[
    "Kariuki", "Ochieng", "Wambui", "Kiptoo", "Muthoni", "Owino", "Chebet", "Gathoni", "Ndungu", "Akinyi", "Langat", "Wafula", "Nyambura", "Kimani",
];

/// EAN-13 in the GS1 restricted-circulation range (prefix 2): valid for scanners, never a real product code.
fn demo_ean(n: usize) -> String {
    let body = format!("20{:010}", 4_200_000_000u64 + n as u64);
    let digits: Vec<u32> = body.chars().map(|c| c.to_digit(10).unwrap()).collect();
    let sum: u32 = digits.iter().enumerate().map(|(i, d)| if i % 2 == 0 { *d } else { d * 3 }).sum();
    format!("{body}{}", (10 - sum % 10) % 10)
}

// ───────────────────────────── Seeder ─────────────────────────────

struct Product {
    id: Uuid,
    def: &'static ProductDef,
    barcode: Option<String>,
}

struct Seeder<'a> {
    state: &'a AppState,
    api: Api,
    rng: StdRng,
    report: DemoReport,
    admin: Uuid,
    branches: Vec<Uuid>,
    /// Staff per branch (index aligned with `branches`).
    staff: Vec<Vec<Uuid>>,
    products: Vec<Product>,
    suppliers: Vec<Uuid>,
    customers: Vec<Uuid>,
    expense_categories: HashMap<String, Uuid>,
    item_seq: usize,
}

fn at(days_ago: i64, hour: u32, minute: u32) -> DateTime<Utc> {
    let day = Utc::now().with_timezone(&TZ).date_naive() - Duration::days(days_ago);
    TZ.from_local_datetime(&day.and_hms_opt(hour, minute, 0).unwrap()).single().unwrap().with_timezone(&Utc).min(Utc::now() - Duration::minutes(5))
}

fn date(days_ago: i64) -> NaiveDate {
    Utc::now().with_timezone(&TZ).date_naive() - Duration::days(days_ago)
}

impl<'a> Seeder<'a> {
    async fn new(state: &'a AppState) -> anyhow::Result<Seeder<'a>> {
        // The business, its defaults and an administrator, exactly like a first start.
        let mut tx = state.db.begin().await?;
        let tenant = crate::bootstrap::seed_tenant(&mut tx, DEMO_NAME, DEMO_SLUG).await?;
        sqlx::query(
            "UPDATE tenants SET is_demo = true, tagline = 'Fine watches, jewellery & fragrance — demo business', phone = '0700 000 900',
                 email = 'hello@pablo-niche.demo.invalid', address = 'Westlands, Nairobi', created_at = now() - interval '170 days'
             WHERE id = $1",
        )
        .bind(tenant)
        .execute(&mut *tx)
        .await?;
        let pin: String = (0..16).map(|_| rand::thread_rng().sample(rand::distributions::Alphanumeric) as char).collect();
        let admin = crate::bootstrap::create_admin(&mut tx, tenant, "Demo Administrator", "admin@pablo-niche.demo.invalid", &pin).await?;
        tx.commit().await?;

        let app = Router::new().nest("/api", crate::routes::api()).with_state(state.clone());
        let api = Api { app, db: state.db.clone(), tenant, secret: state.cfg.jwt_secret.clone() };
        Ok(Seeder {
            state,
            api,
            rng: StdRng::seed_from_u64(20261005),
            report: DemoReport { tenant_id: Some(tenant), created: true, ..Default::default() },
            admin,
            branches: vec![],
            staff: vec![],
            products: vec![],
            suppliers: vec![],
            customers: vec![],
            expense_categories: HashMap::new(),
            item_seq: 0,
        })
    }

    async fn run(&mut self, progress: &impl Fn(&str)) -> anyhow::Result<()> {
        progress("Business, branches and staff");
        self.setup().await.context("setting up the business")?;
        progress("Catalogue");
        self.catalogue().await.context("creating products")?;
        progress("Product photos");
        self.photos().await;
        progress("Customers");
        self.customers().await.context("creating customers")?;
        progress("Trading history");
        self.history(progress).await.context("replaying trading history")?;
        progress("Orders, approvals and stock take");
        self.orders().await.context("creating orders")?;
        self.approvals_and_count().await.context("creating approvals")?;
        progress("Finishing");
        self.finish().await?;
        Ok(())
    }

    async fn setup(&mut self) -> anyhow::Result<()> {
        let a = self.admin;
        let main: Uuid = sqlx::query_scalar("SELECT id FROM branches WHERE tenant_id = $1").bind(self.api.tenant).fetch_one(&self.api.db).await?;
        sqlx::query("UPDATE branches SET location = 'Kimathi Street, Nairobi CBD', created_at = now() - interval '170 days' WHERE id = $1")
            .bind(main)
            .execute(&self.api.db)
            .await?;
        self.branches.push(main);
        for (name, code, location) in BRANCHES {
            let v = self.api.post(a, None, "/branches", json!({ "name": name, "code": code, "location": location })).await?;
            self.branches.push(uuid(&v["id"])?);
        }
        self.report.branches = self.branches.len();

        // Loyalty tuned for premium tickets; WhatsApp stays off for a demo business.
        let mut settings = self.api.call(a, None, Method::GET, "/settings", None).await?["settings"].clone();
        settings["loyalty"]["threshold"] = json!(1000);
        settings["loyalty"]["points_per"] = json!(1);
        settings["loyalty"]["min_spend"] = json!(5000);
        settings["loyalty"]["point_value"] = json!(1);
        settings["loyalty"]["min_redemption_points"] = json!(200);
        settings["loyalty"]["tiers"] = json!([
            { "name": "Bronze", "min_spend": 100000 }, { "name": "Silver", "min_spend": 350000 }, { "name": "Gold", "min_spend": 800000 }
        ]);
        settings["loyalty"]["award_winners"] = json!(3);
        settings["notifications"] = json!({ "whatsapp_receipts": false, "whatsapp_credit_reminders": false, "whatsapp_loyalty": false });
        settings["orders"]["portal_enabled"] = json!(true);
        settings["product"]["auto_code_prefix"] = json!("PN");
        self.api.call(a, None, Method::PUT, "/settings", Some(settings)).await?;

        // Staff: a manager and salespeople per branch (random PINs: platform admins open the business instead).
        let roles = self.api.call(a, None, Method::GET, "/roles", None).await?;
        let role = |name: &str| roles.as_array().and_then(|r| r.iter().find(|x| x["name"] == name)).and_then(|x| x["id"].as_str().map(String::from));
        let (mgr, sales) = (role("Branch Manager").ok_or(anyhow!("role"))?, role("Salesperson").ok_or(anyhow!("role"))?);
        let names = [
            ["Faith Wanjiku", "Brian Otieno", "Mercy Achieng"],
            ["Kevin Mwangi", "Linda Chebet", "Dennis Kiprop"],
            ["Grace Njeri", "Collins Wafula", ""],
            ["Joy Atieno", "Samuel Kiptoo", ""],
        ];
        for (i, branch) in self.branches.clone().into_iter().enumerate() {
            let mut team = vec![];
            for (j, name) in names[i].iter().filter(|n| !n.is_empty()).enumerate() {
                let pin: String = (0..12).map(|_| self.rng.sample(rand::distributions::Alphanumeric) as char).collect();
                let email = format!("{}.{}@pablo-niche.demo.invalid", name.split(' ').next().unwrap().to_lowercase(), i + 1);
                let v = self
                    .api
                    .post(a, None, "/users", json!({
                        "name": name, "email": email, "pin": pin, "role_id": if j == 0 { &mgr } else { &sales },
                        "all_branches": false, "branch_ids": [branch],
                    }))
                    .await?;
                team.push(uuid(&v["id"])?);
            }
            self.staff.push(team);
        }

        for (name, phone) in SUPPLIERS {
            let v = self.api.post(a, None, "/suppliers", json!({ "name": name, "phone": phone })).await?;
            self.suppliers.push(uuid(&v["id"])?);
        }
        let existing = self.api.call(a, None, Method::GET, "/expense-categories", None).await?;
        for c in existing.as_array().into_iter().flatten() {
            self.expense_categories.insert(c["name"].as_str().unwrap_or_default().to_string(), uuid(&c["id"])?);
        }
        for name in ["Delivery", "Packaging", "Shop supplies"] {
            let v = self.api.post(a, None, "/expense-categories", json!({ "name": name })).await?;
            self.expense_categories.insert(name.into(), uuid(&v["id"])?);
        }
        Ok(())
    }

    async fn catalogue(&mut self) -> anyhow::Result<()> {
        let a = self.admin;
        let mut cats = vec![];
        for c in CATEGORIES {
            cats.push(uuid(&self.api.post(a, None, "/categories", json!({ "name": c })).await?["id"])?);
        }
        for (n, def) in PRODUCTS.iter().enumerate() {
            let barcode = (!def.tracked).then(|| demo_ean(n + 1));
            let max_discount = (def.price as f64 * 0.05 / 500.0).round() * 500.0;
            let v = self
                .api
                .post(a, None, "/products", json!({
                    "name": def.name,
                    "nickname": def.name.split(' ').next(),
                    "description": format!("{} — demo catalogue item.", CATEGORIES[def.category].trim_end_matches('s')),
                    "category_id": cats[def.category],
                    "supplier_id": self.suppliers[def.category],
                    "marked_price": def.price,
                    "max_discount": max_discount,
                    "cost_price": (def.price as f64 * if def.category == 2 { 0.48 } else { 0.62 }).round(),
                    "barcode": barcode,
                    "track_items": def.tracked,
                    "loyalty_eligible": true,
                    "low_stock_threshold": if def.category == 2 { 6 } else { 2 },
                }))
                .await?;
            let id = uuid(&v["result"]["id"])?;
            self.products.push(Product { id, def, barcode });
        }
        // Retired line: stays visible as inactive.
        let retired = self.products.iter().find(|p| p.def.weight == 0).map(|p| p.id);
        if let Some(id) = retired {
            self.api.post(a, None, &format!("/products/{id}/status"), json!({ "is_active": false, "reason": "Discontinued line" })).await?;
        }
        sqlx::query("UPDATE products SET created_at = now() - interval '165 days' WHERE tenant_id = $1").bind(self.api.tenant).execute(&self.api.db).await?;
        self.report.products = self.products.len();
        Ok(())
    }

    /// Product photos from Pexels (free licence) when PEXELS_API_KEY is set; stored in our own photo storage with
    /// the photographer credit and source page. Without a key, products are left without photos and reported.
    async fn photos(&mut self) {
        let Ok(key) = std::env::var("PEXELS_API_KEY") else {
            self.report.photos_note = "Set PEXELS_API_KEY and reset the demo to add product photos (all products are waiting for photos).".into();
            return;
        };
        let queries = ["luxury wrist watch", "gold necklace jewelry", "perfume bottle box"];
        let mut pools: Vec<Vec<(String, String, String)>> = vec![];
        for q in queries {
            let res = self
                .state
                .http
                .get("https://api.pexels.com/v1/search")
                .query(&[("query", q), ("per_page", "30"), ("orientation", "square")])
                .header("Authorization", key.trim())
                .send()
                .await;
            let mut pool = vec![];
            if let Ok(r) = res {
                if let Ok(v) = r.json::<Value>().await {
                    for p in v["photos"].as_array().into_iter().flatten() {
                        if let (Some(src), Some(page)) = (p["src"]["large"].as_str(), p["url"].as_str()) {
                            pool.push((src.to_string(), page.to_string(), format!("Photo by {} on Pexels", p["photographer"].as_str().unwrap_or("unknown"))));
                        }
                    }
                }
            }
            pools.push(pool);
        }
        let mut used = [0usize; 3];
        let mut missing = vec![];
        for p in &self.products {
            let pool = &pools[p.def.category];
            let mut stored = 0;
            for k in 0..3 {
                let Some((src, page, credit)) = pool.get(used[p.def.category]).cloned() else { break };
                used[p.def.category] += 1;
                let Ok(res) = self.state.http.get(&src).send().await else { continue };
                let mime = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("image/jpeg").to_string();
                let Ok(bytes) = res.bytes().await else { continue };
                if bytes.len() > 4 * 1024 * 1024 || !mime.starts_with("image/") {
                    continue;
                }
                let ok = sqlx::query(
                    "INSERT INTO product_photos (tenant_id, product_id, data, mime, is_primary, sort_order, source, attribution)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(self.api.tenant)
                .bind(p.id)
                .bind(bytes.as_ref())
                .bind(&mime)
                .bind(k == 0)
                .bind(k as i32)
                .bind(&page)
                .bind(&credit)
                .execute(&self.api.db)
                .await
                .is_ok();
                if ok {
                    stored += 1;
                    self.report.photos += 1;
                }
            }
            if stored == 0 {
                missing.push(p.def.name);
            }
        }
        self.report.photos_note = if missing.is_empty() {
            "Photos from Pexels (credits stored with each photo).".into()
        } else {
            format!("No photo found for: {}", missing.join(", "))
        };
    }

    async fn customers(&mut self) -> anyhow::Result<()> {
        let a = self.admin;
        for i in 0..48 {
            let first = FIRST[i % FIRST.len()];
            let last = LAST[(i * 7) % LAST.len()];
            let v = self
                .api
                .post(a, None, "/customers", json!({
                    "mobile": format!("0700{:06}", 100 + i),
                    "first_name": first,
                    "other_names": last,
                    "nickname": if i % 5 == 0 { first.chars().take(3).collect::<String>() } else { String::new() },
                }))
                .await?;
            self.customers.push(uuid(&v["id"])?);
        }
        sqlx::query("UPDATE customers SET created_at = now() - interval '160 days' WHERE tenant_id = $1").bind(self.api.tenant).execute(&self.api.db).await?;
        // Referral chains: the referrer earns a share of the referred customer's points from now on.
        for (referrer, referred) in [(0, 5), (0, 11), (1, 7), (2, 9), (3, 14), (4, 18), (1, 21), (6, 27)] {
            let since = self.api.mark().await?;
            self.api.post(a, None, "/referrals", json!({ "referrer_id": self.customers[referrer], "referred_id": self.customers[referred] })).await?;
            self.api.stamp(since, at(HISTORY_DAYS + 5, 10, 0)).await?;
        }
        self.report.customers = self.customers.len();
        Ok(())
    }

    /// Next item label for a tracked product (e.g. PN-W03-0007): demo-only labels.
    fn item_codes(&mut self, product: usize, n: i32) -> Vec<String> {
        (0..n)
            .map(|_| {
                self.item_seq += 1;
                format!("PN-{}{:02}-{:04}", ["W", "N", "P"][self.products[product].def.category], product + 1, self.item_seq)
            })
            .collect()
    }

    async fn receive(&mut self, product: usize, branch: usize, qty: i32, when: DateTime<Utc>, opening: bool) -> anyhow::Result<()> {
        if qty <= 0 {
            return Ok(());
        }
        let p = &self.products[product];
        let (id, tracked, cat) = (p.id, p.def.tracked, p.def.category);
        let codes = if tracked { self.item_codes(product, qty) } else { vec![] };
        let since = self.api.mark().await?;
        self.api
            .post(self.admin, Some(self.branches[branch]), "/stock/receive", json!({
                "product_id": id, "branch_id": self.branches[branch], "quantity": qty, "barcodes": codes,
                "supplier_id": self.suppliers[cat], "reference": format!("DEMO-GRN-{}", when.format("%y%m%d")),
                "date_received": when.with_timezone(&TZ).date_naive(), "kind": if opening { "opening" } else { "received" },
            }))
            .await?;
        self.api.stamp(since, when).await
    }

    async fn stock_at(&self, product: usize, branch: usize) -> anyhow::Result<i32> {
        Ok(sqlx::query_scalar("SELECT COALESCE((SELECT on_hand - reserved FROM stock_levels WHERE product_id = $1 AND branch_id = $2), 0)")
            .bind(self.products[product].id)
            .bind(self.branches[branch])
            .fetch_one(&self.api.db)
            .await?)
    }

    async fn free_item(&self, product: usize, branch: usize, taken: &[String]) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT barcode FROM stock_items WHERE product_id = $1 AND branch_id = $2 AND status = 'in_stock' AND NOT (barcode = ANY($3))
             ORDER BY created_at, barcode LIMIT 1",
        )
        .bind(self.products[product].id)
        .bind(self.branches[branch])
        .bind(taken)
        .fetch_optional(&self.api.db)
        .await?)
    }

    fn weighted_product(&mut self) -> usize {
        let total: u32 = self.products.iter().map(|p| p.def.weight).sum();
        let mut x = self.rng.gen_range(0..total);
        for (i, p) in self.products.iter().enumerate() {
            if x < p.def.weight {
                return i;
            }
            x -= p.def.weight;
        }
        0
    }

    /// ~5 months replayed in date order: stock arrivals, sales, credit collections, returns, transfers, expenses.
    async fn history(&mut self, progress: &impl Fn(&str)) -> anyhow::Result<()> {
        // Opening stock 160 days ago (deliberately uneven: some lines only at some branches).
        for i in 0..self.products.len() {
            for b in 0..self.branches.len() {
                let q = self.products[i].def.stock[b];
                self.receive(i, b, q, at(HISTORY_DAYS + 10, 9, 0), true).await?;
            }
        }
        // The award round that runs until mid-history.
        sqlx::query("UPDATE award_periods SET name = 'Launch Season Awards', start_date = $2 WHERE tenant_id = $1")
            .bind(self.api.tenant)
            .bind(date(HISTORY_DAYS))
            .execute(&self.api.db)
            .await?;

        let mut credits: Vec<(Uuid, i64)> = vec![]; // credit id, sale day
        let mut sales_for_returns: Vec<(Uuid, i64)> = vec![];
        for day in (0..=HISTORY_DAYS).rev() {
            if day % 30 == 0 {
                progress(&format!("Trading history — {} days to go", day));
            }
            // Restocking runs (the last ones show up as "recently received").
            if [110, 75, 40, 12, 2].contains(&day) {
                for i in 0..self.products.len() {
                    if self.products[i].def.weight == 0 {
                        continue;
                    }
                    for b in 0..self.branches.len() {
                        let base = self.products[i].def.stock[b];
                        if base == 0 || self.rng.gen_bool(0.35) {
                            continue;
                        }
                        let qty = (base / 2).max(1) + self.rng.gen_range(0..=2);
                        self.receive(i, b, qty, at(day, 8, 30), false).await?;
                    }
                }
            }
            // Busier recently and on weekends.
            let weekday = date(day).format("%u").to_string().parse::<u32>().unwrap_or(1);
            let base = if day < 30 { 3.2 } else if day < 90 { 2.2 } else { 1.4 };
            let n = (base * if weekday >= 6 { 1.5 } else { 1.0 } + self.rng.gen_range(0.0..1.6)) as usize;
            for k in 0..n {
                let when = at(day, 9 + (k as u32 * 3 + self.rng.gen_range(0..3)) % 11, self.rng.gen_range(0..60));
                if let Some((sale, credit)) = self.sale(when).await? {
                    self.report.sales += 1;
                    if let Some(c) = credit {
                        credits.push((c, day));
                    }
                    if self.rng.gen_bool(0.03) && day > 3 {
                        sales_for_returns.push((sale, day));
                    }
                }
            }
            // Credit collections: some paid in full, some partly, some left to go overdue.
            for (credit, sold_day) in credits.clone() {
                let age = sold_day - day;
                if age == 7 || age == 21 || age == 40 {
                    let r = self.rng.gen_range(0..10);
                    if r < 4 {
                        self.repay(credit, when_collect(day), age == 40).await?;
                    }
                }
            }
            // Occasional returns a few days after the sale.
            for (sale, sold_day) in sales_for_returns.clone() {
                if sold_day - day == 3 {
                    if self.return_part(sale, at(day, 15, 10)).await.is_ok() {
                        self.report.returns += 1;
                    }
                }
            }
            // Inter-branch transfers.
            if [100, 60, 25, 6].contains(&day) {
                self.transfer(day, true).await?;
            }
            // Expenses: weekly delivery/packaging, monthly rent/utilities/salaries, marketing pushes.
            self.expenses(day).await?;
            // Close the launch award round half way and open the current one.
            if day == HISTORY_DAYS / 2 {
                self.awards(day).await?;
            }
        }
        // A transfer still on the road.
        self.transfer(1, false).await?;
        Ok(())
    }

    async fn sale(&mut self, when: DateTime<Utc>) -> anyhow::Result<Option<(Uuid, Option<Uuid>)>> {
        let b = *[0usize, 0, 0, 1, 1, 2, 3].get(self.rng.gen_range(0..7)).unwrap();
        let team = self.staff[b].clone();
        let seller = team[self.rng.gen_range(0..team.len())];
        let mut items = vec![];
        let mut taken: Vec<String> = vec![];
        let lines = if self.rng.gen_bool(0.7) { 1 } else if self.rng.gen_bool(0.75) { 2 } else { 3 };
        for _ in 0..lines * 3 {
            if items.len() >= lines {
                break;
            }
            let i = self.weighted_product();
            if items.iter().any(|x: &Value| x["product_id"] == json!(self.products[i].id)) {
                continue;
            }
            let p = &self.products[i];
            let price = p.def.price;
            // The product's own maximum discount (same rounding as when it was created).
            let (id, tracked, max_disc) = (p.id, p.def.tracked, ((price as f64 * 0.05 / 500.0).round() * 500.0) as i64);
            // Usually at the marked price; sometimes a small discount (within the maximum) or a small premium.
            let r: f64 = self.rng.gen();
            let unit = if r < 0.68 { price } else if r < 0.95 { price - (self.rng.gen_range(1..=max_disc.max(200)) / 100 * 100).clamp(100, max_disc.max(100)) } else { price + 500 };
            if tracked {
                let Some(code) = self.free_item(i, b, &taken).await? else { continue };
                taken.push(code.clone());
                items.push(json!({ "product_id": id, "quantity": 1, "unit_price": unit, "barcode": code }));
            } else {
                let avail = self.stock_at(i, b).await?;
                if avail <= 0 {
                    continue;
                }
                let qty = if self.products[i].def.category == 2 && self.rng.gen_bool(0.25) { 2.min(avail) } else { 1 };
                items.push(json!({ "product_id": id, "quantity": qty, "unit_price": unit, "barcode": self.products[i].barcode }));
            }
        }
        if items.is_empty() {
            return Ok(None);
        }
        // Repeat customers dominate; a quarter are walk-ins.
        let customer = if self.rng.gen_bool(0.78) {
            let n = self.customers.len() as f64;
            let idx = ((self.rng.gen::<f64>().powf(2.2)) * n) as usize;
            Some(self.customers[idx.min(self.customers.len() - 1)])
        } else {
            None
        };
        let roll: f64 = self.rng.gen();
        let method = if customer.is_some() && roll < 0.12 { "credit" } else if roll < 0.62 { "mpesa" } else { "cash" };
        let mut body = json!({
            "branch_id": self.branches[b],
            "customer_id": customer,
            "items": items,
            "payment": { "method": method },
            "client_ref": Uuid::new_v4(),
        });
        if method == "mpesa" {
            let code: String = (0..6).map(|_| self.rng.sample(rand::distributions::Alphanumeric) as char).collect::<String>().to_uppercase();
            body["payment"]["reference"] = json!(format!("DEMO{code}"));
        }
        if method == "credit" && self.rng.gen_bool(0.4) {
            body["deposit"] = json!({ "amount": 5000 * self.rng.gen_range(1..=4), "method": "cash" });
        }
        // Some loyal customers redeem points (handled by an administrator, who holds the permission).
        let mut actor = seller;
        if let Some(c) = customer {
            let pts: i64 = sqlx::query_scalar("SELECT own_points + referral_points FROM customers WHERE id = $1").bind(c).fetch_one(&self.api.db).await?;
            if pts >= 400 && self.rng.gen_bool(0.18) && method != "credit" {
                body["redeem_points"] = json!((pts / 2).min(1500));
                actor = self.admin;
            }
        }
        let since = self.api.mark().await?;
        let res = self.api.post(actor, Some(self.branches[b]), "/sales", body.clone()).await;
        let v = match res {
            Ok(v) => v,
            // Deposits larger than a small total, or stock races: retry once without the extras.
            Err(_) => {
                body.as_object_mut().map(|o| {
                    o.remove("deposit");
                    o.remove("redeem_points");
                });
                body["client_ref"] = json!(Uuid::new_v4());
                match self.api.post(seller, Some(self.branches[b]), "/sales", body).await {
                    Ok(v) => v,
                    Err(e) => {
                        self.report.notes.push(format!("skipped a sale: {e}"));
                        return Ok(None);
                    }
                }
            }
        };
        self.api.stamp(since, when).await?;
        let sale = uuid(&v["sale"]["id"])?;
        let credit = v["credit"]["id"].as_str().and_then(|s| s.parse().ok());
        if credit.is_some() {
            // Due dates follow the (backdated) sale date.
            sqlx::query("UPDATE credit_sales SET due_date = ($2 AT TIME ZONE 'Africa/Nairobi')::date + 30 WHERE sale_id = $1")
                .bind(sale)
                .bind(when)
                .execute(&self.api.db)
                .await?;
        }
        Ok(Some((sale, credit)))
    }

    async fn repay(&mut self, credit: Uuid, when: DateTime<Utc>, full: bool) -> anyhow::Result<()> {
        let (balance, branch): (rust_decimal::Decimal, Uuid) =
            sqlx::query_as("SELECT original_amount - amount_paid - adjustments, branch_id FROM credit_sales WHERE id = $1 AND status IN ('outstanding','partially_paid')")
                .bind(credit)
                .fetch_optional(&self.api.db)
                .await?
                .unwrap_or((rust_decimal::Decimal::ZERO, Uuid::nil()));
        if balance <= rust_decimal::Decimal::ZERO {
            return Ok(());
        }
        use rust_decimal::prelude::ToPrimitive;
        let bal = balance.to_f64().unwrap_or(0.0);
        let amount = if full { bal } else { (bal * self.rng.gen_range(0.3..0.6) / 100.0).round() * 100.0 };
        let since = self.api.mark().await?;
        let code: String = (0..6).map(|_| self.rng.sample(rand::distributions::Alphanumeric) as char).collect::<String>().to_uppercase();
        self.api
            .post(self.admin, Some(branch), &format!("/credit/{credit}/payments"), json!({ "amount": amount.max(100.0).min(bal), "method": "mpesa", "reference": format!("DEMO{code}") }))
            .await?;
        self.api.stamp(since, when).await
    }

    async fn return_part(&mut self, sale: Uuid, when: DateTime<Utc>) -> anyhow::Result<()> {
        let d = self.api.call(self.admin, None, Method::GET, &format!("/sales/{sale}"), None).await?;
        let item = d["items"].as_array().and_then(|i| i.first()).ok_or(anyhow!("no items"))?;
        let branch = uuid(&d["sale"]["branch_id"])?;
        let since = self.api.mark().await?;
        self.api
            .post(self.admin, Some(branch), &format!("/sales/{sale}/return"), json!({
                "items": [{ "sale_item_id": item["id"], "quantity": 1 }],
                "reason": "Customer changed mind (demo)", "restock": true, "refund_method": "cash",
            }))
            .await?;
        self.api.stamp(since, when).await
    }

    async fn transfer(&mut self, day: i64, complete: bool) -> anyhow::Result<()> {
        // From the main branch to the branch that needs it most.
        let to = self.rng.gen_range(1..self.branches.len());
        let mut items = vec![];
        for i in 0..self.products.len() {
            if self.products[i].def.weight == 0 || items.len() >= 3 || !self.rng.gen_bool(0.3) {
                continue;
            }
            if self.products[i].def.tracked {
                let mut codes = vec![];
                if let Some(c) = self.free_item(i, 0, &[]).await? {
                    codes.push(c);
                }
                if !codes.is_empty() {
                    items.push(json!({ "product_id": self.products[i].id, "quantity": 1, "barcodes": codes }));
                }
            } else if self.stock_at(i, 0).await? > 4 {
                items.push(json!({ "product_id": self.products[i].id, "quantity": 2, "barcodes": [] }));
            }
        }
        if items.is_empty() {
            return Ok(());
        }
        let (from, dest) = (self.branches[0], self.branches[to]);
        let since = self.api.mark().await?;
        let t = self
            .api
            .post(self.admin, Some(from), "/transfers", json!({ "to_branch_id": dest, "transfer_date": date(day), "items": items, "submit": true, "notes": "Demo replenishment" }))
            .await?;
        let id = uuid(&t["id"])?;
        self.api.post(self.admin, Some(from), &format!("/transfers/{id}/dispatch"), json!({})).await?;
        if complete {
            self.api.post(self.admin, Some(dest), &format!("/transfers/{id}/receive"), json!({})).await?;
        }
        let when = at(day, 11, 0);
        self.api.stamp(since, when).await?;
        sqlx::query(
            "UPDATE transfers SET approved_at = CASE WHEN approved_at IS NULL THEN NULL ELSE $2 END,
                 dispatched_at = CASE WHEN dispatched_at IS NULL THEN NULL ELSE $2 + interval '1 hour' END,
                 received_at = CASE WHEN received_at IS NULL THEN NULL ELSE $2 + interval '1 day' END
             WHERE id = $1",
        )
        .bind(id)
        .bind(when)
        .execute(&self.api.db)
        .await?;
        self.report.transfers += 1;
        Ok(())
    }

    async fn expenses(&mut self, day: i64) -> anyhow::Result<()> {
        let d = date(day);
        let dom: u32 = d.format("%d").to_string().parse().unwrap_or(1);
        let mut list: Vec<(&str, usize, i64, &str)> = vec![];
        if dom == 1 {
            list.extend([("Rent", 0, 185_000, "Shop rent — CBD"), ("Rent", 1, 240_000, "Shop rent — Westlands"), ("Rent", 2, 210_000, "Shop rent — Village Market"), ("Rent", 3, 160_000, "Shop rent — Two Rivers")]);
            list.push(("Salaries & Wages", 0, 640_000, "Monthly payroll"));
        }
        if dom == 5 {
            list.push(("Utilities", self.rng.gen_range(0..4), 18_000 + self.rng.gen_range(0..9_000), "Electricity & internet"));
        }
        if day % 7 == 3 {
            list.push(("Delivery", self.rng.gen_range(0..4), 2_500 + self.rng.gen_range(0..4_000), "Rider deliveries"));
            list.push(("Packaging", 0, 6_000 + self.rng.gen_range(0..6_000), "Gift boxes and bags"));
        }
        if day % 14 == 9 {
            list.push(("Shop supplies", self.rng.gen_range(0..4), 3_500 + self.rng.gen_range(0..3_000), "Cleaning and display supplies"));
            list.push(("Transport", self.rng.gen_range(0..4), 1_500 + self.rng.gen_range(0..2_500), "Stock run"));
        }
        if day % 30 == 18 {
            list.push(("Marketing", 0, 45_000 + self.rng.gen_range(0..40_000), "Social media campaign"));
        }
        for (cat, branch, amount, desc) in list {
            let Some(category) = self.expense_categories.get(cat).copied() else { continue };
            let since = self.api.mark().await?;
            self.api
                .post(self.admin, Some(self.branches[branch]), "/expenses", json!({
                    "branch_id": self.branches[branch], "category_id": category, "amount": amount, "expense_date": d,
                    "description": desc, "payee": "Demo payee", "payment_method": if amount > 50_000 { "bank" } else { "mpesa" },
                }))
                .await?;
            self.api.stamp(since, at(day, 16, 0)).await?;
            self.report.expenses += 1;
        }
        Ok(())
    }

    async fn awards(&mut self, day: i64) -> anyhow::Result<()> {
        let list = self.api.call(self.admin, None, Method::GET, "/awards", None).await?;
        let open = list["periods"].as_array().or(list.as_array()).and_then(|p| p.iter().find(|x| x["status"] == "open")).and_then(|x| x["id"].as_str()).map(String::from);
        if let Some(id) = open {
            self.api.post(self.admin, None, &format!("/awards/{id}/close"), json!({})).await?;
            sqlx::query("UPDATE award_periods SET end_date = $2 WHERE id = $1::uuid").bind(&id).bind(date(day)).execute(&self.api.db).await?;
        }
        self.api.post(self.admin, None, "/awards", json!({ "name": "Festive Season Awards", "start_date": date(day - 1) })).await?;
        Ok(())
    }

    /// Customer orders at every stage (recent days), through the staff order flow.
    async fn orders(&mut self) -> anyhow::Result<()> {
        let stages: [(&str, i64); 10] = [
            ("new", 0), ("new", 0), ("confirmed", 1), ("preparing", 1), ("dispatched", 2), ("on_delivery", 2),
            ("delivered", 4), ("completed", 6), ("completed", 9), ("cancelled", 3),
        ];
        let flow = ["confirmed", "preparing", "dispatched", "on_delivery", "delivered", "completed"];
        for (n, (target, day)) in stages.iter().enumerate() {
            let b = n % 2; // main and Westlands
            let mut items = vec![];
            for i in [12usize, 13, 14, 15, 7, 8] {
                if items.len() < 1 + n % 3 && self.stock_at(i, b).await? > 2 {
                    items.push(json!({ "product_id": self.products[i].id, "quantity": 1 }));
                }
            }
            if items.is_empty() {
                continue;
            }
            let customer = self.customers[(n * 5 + 3) % self.customers.len()];
            let since = self.api.mark().await?;
            let o = self
                .api
                .post(self.admin, Some(self.branches[b]), "/orders", json!({
                    "branch_id": self.branches[b], "customer_id": customer, "items": items,
                    "delivery_location": (["Kilimani", "Lavington", "Runda", "Karen", "Kileleshwa"][n % 5]), "notes": "Demo order",
                }))
                .await?;
            let id = uuid(&o["id"])?;
            if *target == "cancelled" {
                self.api.post(self.admin, Some(self.branches[b]), &format!("/orders/{id}/status"), json!({ "status": "cancelled", "notes": "Customer cancelled (demo)" })).await?;
            } else if *target != "new" {
                for step in flow.iter().take(flow.iter().position(|s| s == target).unwrap() + 1) {
                    let mut body = json!({ "status": step });
                    if *step == "delivered" {
                        body["payment"] = json!({ "method": "cash" });
                    }
                    // Optional steps may be switched off in settings: skip any the API refuses.
                    let _ = self.api.post(self.admin, Some(self.branches[b]), &format!("/orders/{id}/status"), body).await;
                }
            }
            self.api.stamp(since, at(*day, 10 + n as u32 % 8, 15)).await?;
            self.report.orders += 1;
        }
        Ok(())
    }

    /// Pending approvals (one transfer, one large expense) and a recent stock take with variances.
    async fn approvals_and_count(&mut self) -> anyhow::Result<()> {
        let a = self.admin;
        let admin_level = json!([{ "approver_type": "admin" }]);
        self.api.call(a, None, Method::PUT, "/settings/workflows/stock.transfer", Some(json!({ "enabled": true, "levels": admin_level, "min_amount": null }))).await?;
        let since = self.api.mark().await?;
        let main_mgr = self.staff[0][0];
        if let Some(i) = (12..17).next() {
            let _ = self
                .api
                .post(main_mgr, Some(self.branches[0]), "/transfers", json!({
                    "to_branch_id": self.branches[3], "transfer_date": date(0), "submit": true,
                    "items": [{ "product_id": self.products[i].id, "quantity": 3, "barcodes": [] }], "notes": "Weekend top-up (demo)",
                }))
                .await;
        }
        self.api.call(a, None, Method::PUT, "/settings/workflows/stock.transfer", Some(json!({ "enabled": false, "levels": admin_level, "min_amount": null }))).await?;
        self.api.call(a, None, Method::PUT, "/settings/workflows/expense", Some(json!({ "enabled": true, "levels": admin_level, "min_amount": 100000 }))).await?;
        if let Some(cat) = self.expense_categories.get("Marketing").copied() {
            let _ = self
                .api
                .post(main_mgr, Some(self.branches[0]), "/expenses", json!({
                    "branch_id": self.branches[0], "category_id": cat, "amount": 150000, "expense_date": date(0),
                    "description": "Billboard — festive campaign (demo)", "payee": "Demo Media Ltd",
                }))
                .await;
        }
        self.api.call(a, None, Method::PUT, "/settings/workflows/expense", Some(json!({ "enabled": false, "levels": admin_level, "min_amount": null }))).await?;
        self.api.stamp(since, at(0, 9, 30)).await?;

        // Stock take at Village Market: most counts match, two differ.
        let mut lines = vec![];
        for (k, i) in [12usize, 13, 14, 15, 16, 17, 8, 10].into_iter().enumerate() {
            let on_hand: i32 = sqlx::query_scalar("SELECT COALESCE((SELECT on_hand FROM stock_levels WHERE product_id = $1 AND branch_id = $2), 0)")
                .bind(self.products[i].id)
                .bind(self.branches[2])
                .fetch_one(&self.api.db)
                .await?;
            let counted = match k {
                2 => (on_hand - 1).max(0),
                5 => on_hand + 1,
                _ => on_hand,
            };
            lines.push(json!({ "product_id": self.products[i].id, "counted": counted }));
        }
        let since = self.api.mark().await?;
        self.api.post(a, Some(self.branches[2]), "/stock/count", json!({ "branch_id": self.branches[2], "lines": lines, "reason": "Monthly stock take (demo)" })).await?;
        self.api.stamp(since, at(1, 18, 30)).await?;
        Ok(())
    }

    async fn finish(&mut self) -> anyhow::Result<()> {
        // Last purchase follows the backdated sales (the API sets it at the time of the call).
        sqlx::query(
            "UPDATE customers c SET last_purchase_at = s.last FROM
               (SELECT customer_id, max(created_at) AS last FROM sales WHERE tenant_id = $1 AND status <> 'cancelled' GROUP BY customer_id) s
             WHERE c.id = s.customer_id AND c.tenant_id = $1",
        )
        .bind(self.api.tenant)
        .execute(&self.api.db)
        .await?;
        sqlx::query("UPDATE notifications SET read_at = now() WHERE tenant_id = $1 AND created_at < now() - interval '2 days'")
            .bind(self.api.tenant)
            .execute(&self.api.db)
            .await?;
        Ok(())
    }
}

fn when_collect(day: i64) -> DateTime<Utc> {
    at(day, 12, 20)
}

fn uuid(v: &Value) -> anyhow::Result<Uuid> {
    v.as_str().and_then(|s| s.parse().ok()).ok_or_else(|| anyhow!("expected an id, got {v}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_barcodes_are_valid_restricted_ean13() {
        for n in 1..30 {
            let c = demo_ean(n);
            assert_eq!(c.len(), 13);
            assert!(c.starts_with('2'), "GS1 restricted range");
            let d: Vec<u32> = c.chars().map(|x| x.to_digit(10).unwrap()).collect();
            let sum: u32 = d.iter().take(12).enumerate().map(|(i, x)| if i % 2 == 0 { *x } else { x * 3 }).sum();
            assert_eq!((10 - sum % 10) % 10, d[12]);
        }
    }
}
