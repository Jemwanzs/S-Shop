# 33 — Business onboarding & data migration (roadmap 99–101)

**Settings → Business → Onboarding** brings a business's records into S'Shop from Excel:
- **Phase 1 (live):** Products, Customers, Opening stock balances, Expenses.
- **Phase 2 (live):** Sales history, Credit sales and opening balances, Loyalty opening points.
- **Phase 3 (next):** orders, transfers and the reconciliation report.

| Part | Where |
|---|---|
| Field definitions, templates, reading workbooks | `server/src/onboarding.rs` (`columns`, `template`, `read`) |
| Upload, validation, approval, posting, history, error report | `server/src/routes/onboarding.rs` |
| Schema | `migrations/0029_onboarding.sql` — `import_batches`, `import_rows`, `import_batch_id` on products / customers / expenses / stock movements, `tenants.migration_cutover` |
| Screen | `web/src/pages/settings/Onboarding.tsx` |
| Permission | `settings.onboarding`; posting needs a full sign-in; the **Data import (onboarding)** workflow adds approval |

## Flow

**Download template → fill in → Upload → map columns → preview & validate → Import (or send for approval) → history.**

- **Templates** come from the same definitions that read uploads:
  - only real S'Shop fields;
  - required columns marked `*`;
  - a grey sample row (skipped if left in);
  - code, barcode and phone columns formatted as text (leading zeros kept);
  - dropdowns for controlled values (YES/NO, statuses, your branch codes, your payment methods);
  - an **Instructions** sheet explaining every field and how templates link: products by **Product Code**, branches
    by **Branch Code**, customers by **Mobile Number**.
- **Column mapping**: headers are matched by name and common alternatives (*SKU*, *Price*, *Phone Number*,
  *Voucher No*, *Qty*…). The batch view lets each column be re-mapped or ignored, and the stored workbook is re-read.
  No database IDs are ever asked for.
- **Preview**: total, valid, warnings and invalid rows, colour-coded, with the reason beside each row and the failing
  cell highlighted. Filters by status.
  - **Error report** (`.xlsx`): only the rows to fix, with what to fix. Correct them and upload that file again.
  - **Check again**: re-validates, e.g. after importing the products an opening-stock file refers to.
- **Never silently skipped**:
  - Invalid rows block the import.
  - Products and customers may be imported *valid rows only* when the person explicitly ticks it.
  - Opening stock and expenses import complete or not at all.
- **Posting**:
  - The batch is re-validated inside the database transaction and posts everything or nothing.
  - With the *Data import* workflow on, it waits for an approver (never the uploader) and posts on approval.
  - The uploader, approver, times and file name are kept, and every posted record points to its batch.
  - Nothing sends notifications or payment requests.
- **Once only**:
  - The same workbook cannot post twice per dataset.
  - Row-level checks catch re-imported codes, barcodes, mobiles and expense references even in a re-saved file.

## Datasets (phase 1)

| Dataset | Rules |
|---|---|
| **Products** | *Add new*: S'Shop numbering when no code is given; existing codes refused (use *Update existing products*, `products.edit`); barcodes validated (shape, unique against products and live item labels, the owner named), required when *Barcode Required During Sale* = YES (default when barcodes are on); *Stock Tracking* Product barcode / Individual items; new categories and suppliers created (warned); *Branch Applicability* ALL or branch codes — catalogue only, **never stock**. |
| **Customers** | Mobile normalised (07 / 01 / 254) and unique; an existing mobile is **matched, not duplicated**. |
| **Opening stock** | Product (code or barcode) and branch (code) checked; quantity ≥ 0; tracked products need one item barcode per unit (unique); posted through the stock ledger's **opening** receipt with the as-of date (warned when it differs from the migration cutover); **refused where stock is already recorded** for that product and branch — corrections go through a stock take. |
| **Expenses** | Original dates (day-first dates understood), branch codes, categories (new ones created), payment methods from Settings; an expense reference imports once; posted as approved expenses. |

**Migration cutover**: the day S'Shop takes over. Records before it are history, opening balances are as of it, and
S'Shop records everything after it.

## Phase 2 (roadmap 100)

| Dataset | Rules |
|---|---|
| **Sales history** | Rows with the same *Receipt Reference* are one sale. Its lines must agree on date, branch, payment, customer and salesperson. Recorded as a historical sale (`is_legacy`) with the original receipt number and date (midday local time, so the business day never shifts), lines with their discounts, the payment (method from Settings, reference kept — nothing is requested) and the salesperson as sale owner. **No stock movement and no loyalty points.** The customer's spend, visits and tier update, and the last purchase moves forward only to the original date. Refused: on or after the migration cutover (record those in S'Shop), future dates, receipt numbers already in S'Shop, unknown products, branches, customers or salespeople, and *Credit* as the payment method (credit sales use their own template, so receivables are never counted twice). |
| **Credit sales & balances** | One row per credit: a historical sale plus its credit record. The original sale date, the **original due date** (aging), the amount paid before S'Shop (recorded once as *Paid before S'Shop*), and the status (outstanding / partially paid / paid). Without a product it is an **Opening Credit Balance** — no invented item history. Refused: due before the sale date, paid above the original, dates on or after the cutover, references already used. **Debts already overdue when imported never trigger automatic overdue alerts or customer WhatsApp reminders**; later ones remind as usual. |
| **Loyalty opening points** | Recorded as opening entries in the loyalty ledger (earned and redeemed before S'Shop when given — they must equal the balance), with the customer's totals updated together. Once per customer, never awarded again by imported sales. Optional expiry date. |

All three import complete or not at all (money and points are never partially imported).

## Phase 3 (roadmap 101, next)

- Orders and transfers: closed ones as history, outstanding ones into the live modules.
- The migration reconciliation report.

## Tests

- **Smoke suite**, section *Roadmap 99*, with workbooks built by the suite:
  - templates, header aliases, per-row errors, invalid rows blocking, explicit skip, the error report;
  - automatic numbering, refusing the same workbook twice, update mode;
  - customer matching;
  - opening stock completeness and never adding on top of recorded stock;
  - day-first dates, approval by a second administrator, expense references imported once;
  - totals and permissions.
- **Unit**: templates read back, header aliases, value conversion.
