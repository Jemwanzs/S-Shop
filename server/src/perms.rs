//! Permission catalogue and default role templates.
//!
//! Permissions are `module.action` strings stored on roles. The wildcard `*`
//! grants everything (Tenant Administrator).

use serde::Serialize;

#[derive(Serialize)]
pub struct PermGroup {
    pub module: &'static str,
    pub label: &'static str,
    pub permissions: &'static [(&'static str, &'static str)],
}

pub const CATALOGUE: &[PermGroup] = &[
    PermGroup { module: "dashboard", label: "Dashboard", permissions: &[
        ("dashboard.view", "View dashboard & analytics"),
    ]},
    PermGroup { module: "sales", label: "Sales", permissions: &[
        ("sales.view", "View sales"),
        ("sales.create", "Record sales"),
        ("sales.discount", "Sell below marked price"),
        ("sales.discount_override", "Approve discounts above the maximum"),
        ("sales.change_branch", "Sell from another branch"),
        ("sales.return", "Process returns & refunds"),
        ("sales.cancel", "Cancel sales"),
        ("sales.view_financials", "View cost & profit figures"),
    ]},
    PermGroup { module: "credit", label: "Credit Sales", permissions: &[
        ("credit.view", "View credit sales"),
        ("credit.collect", "Record repayments"),
        ("credit.write_off", "Write off credit"),
    ]},
    PermGroup { module: "orders", label: "Orders", permissions: &[
        ("orders.view", "View orders"),
        ("orders.manage", "Create & progress orders"),
    ]},
    PermGroup { module: "products", label: "Products", permissions: &[
        ("products.view", "View products"),
        ("products.create", "Create products"),
        ("products.edit", "Edit products"),
        ("products.deactivate", "Activate / deactivate products"),
    ]},
    PermGroup { module: "stock", label: "Stock", permissions: &[
        ("stock.view", "View stock"),
        ("stock.add", "Receive stock"),
        ("stock.adjust", "Adjust & count stock"),
        ("stock.write_off", "Write off stock"),
        ("stock.transfer", "Create & dispatch transfers"),
        ("stock.receive_transfer", "Receive transfers"),
    ]},
    PermGroup { module: "customers", label: "Customers", permissions: &[
        ("customers.view", "View customers"),
        ("customers.create", "Create customers"),
        ("customers.edit", "Edit customers"),
        ("customers.view_loyalty", "View loyalty points"),
        ("customers.redeem_points", "Redeem points"),
        ("customers.view_credit", "View credit balances"),
        ("loyalty.manage", "Manage referrals, award periods & points"),
    ]},
    PermGroup { module: "expenses", label: "Expenses", permissions: &[
        ("expenses.view", "View expenses"),
        ("expenses.create", "Record expenses"),
    ]},
    PermGroup { module: "reports", label: "Reports", permissions: &[
        ("reports.view", "View reports"),
        ("reports.export", "Export reports (PDF / Excel)"),
    ]},
    PermGroup { module: "admin", label: "Administration", permissions: &[
        ("approvals.approve", "Approve requests"),
        ("branches.manage", "Manage branches"),
        ("users.manage", "Manage users"),
        ("roles.manage", "Manage roles & permissions"),
        ("settings.manage", "Manage settings & workflows"),
        ("audit.view", "View audit trail"),
    ]},
];

pub fn is_known(p: &str) -> bool {
    p == "*" || CATALOGUE.iter().any(|g| g.permissions.iter().any(|(k, _)| *k == p))
}

pub struct RoleTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub permissions: &'static [&'static str],
}

pub const ADMIN_ROLE: &str = "Tenant Administrator";

pub const DEFAULT_ROLES: &[RoleTemplate] = &[
    RoleTemplate { name: ADMIN_ROLE, description: "Full access to everything", permissions: &["*"] },
    RoleTemplate { name: "Manager", description: "Runs the business day to day", permissions: &[
        "dashboard.view", "sales.view", "sales.create", "sales.discount", "sales.discount_override", "sales.change_branch",
        "sales.return", "sales.cancel", "sales.view_financials", "credit.view", "credit.collect", "credit.write_off",
        "orders.view", "orders.manage", "products.view", "products.create", "products.edit", "products.deactivate",
        "stock.view", "stock.add", "stock.adjust", "stock.write_off", "stock.transfer", "stock.receive_transfer",
        "customers.view", "customers.create", "customers.edit", "customers.view_loyalty", "customers.redeem_points",
        "customers.view_credit", "loyalty.manage", "expenses.view", "expenses.create", "reports.view", "reports.export",
        "approvals.approve", "audit.view",
    ]},
    RoleTemplate { name: "Branch Manager", description: "Manages one or more branches", permissions: &[
        "dashboard.view", "sales.view", "sales.create", "sales.discount", "sales.discount_override", "sales.return",
        "credit.view", "credit.collect", "orders.view", "orders.manage", "products.view", "stock.view", "stock.add",
        "stock.adjust", "stock.transfer", "stock.receive_transfer", "customers.view", "customers.create", "customers.edit",
        "customers.view_loyalty", "customers.redeem_points", "customers.view_credit", "expenses.view", "expenses.create",
        "reports.view", "reports.export", "approvals.approve",
    ]},
    RoleTemplate { name: "Salesperson", description: "Records sales at the counter", permissions: &[
        "sales.view", "sales.create", "sales.discount", "credit.view", "credit.collect", "products.view", "stock.view",
        "customers.view", "customers.create", "customers.view_loyalty", "customers.redeem_points", "orders.view",
    ]},
    RoleTemplate { name: "Storekeeper", description: "Receives, counts and moves stock", permissions: &[
        "products.view", "products.create", "products.edit", "stock.view", "stock.add", "stock.adjust",
        "stock.transfer", "stock.receive_transfer",
    ]},
    RoleTemplate { name: "Order Manager", description: "Processes customer orders", permissions: &[
        "orders.view", "orders.manage", "products.view", "stock.view", "customers.view", "customers.create",
    ]},
    RoleTemplate { name: "Finance", description: "Expenses, credit and financial reports", permissions: &[
        "dashboard.view", "sales.view", "sales.view_financials", "credit.view", "credit.collect", "credit.write_off",
        "expenses.view", "expenses.create", "reports.view", "reports.export", "customers.view", "customers.view_credit",
        "approvals.approve",
    ]},
    RoleTemplate { name: "Auditor", description: "Read-only access including the audit trail", permissions: &[
        "dashboard.view", "sales.view", "sales.view_financials", "credit.view", "orders.view", "products.view",
        "stock.view", "customers.view", "customers.view_loyalty", "customers.view_credit", "expenses.view",
        "reports.view", "reports.export", "audit.view",
    ]},
    RoleTemplate { name: "View Only", description: "Can look but not change anything", permissions: &[
        "dashboard.view", "sales.view", "orders.view", "products.view", "stock.view", "customers.view",
    ]},
];

/// Maps the legacy Pablo Loyalty fixed roles onto the new role templates.
pub fn legacy_role(old: &str) -> &'static str {
    match old {
        "admin" => ADMIN_ROLE,
        "manager" => "Manager",
        "assistant_manager" => "Branch Manager",
        "edit_update" => "Salesperson",
        "print_communicate" => "Auditor",
        _ => "View Only",
    }
}
