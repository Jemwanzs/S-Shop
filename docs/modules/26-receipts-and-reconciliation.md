# 26 — Digital receipts, sharing, return & exchange reconciliation (roadmap 65–67)

One compact receipt template covers every channel: on screen, PDF, print, image, email, WhatsApp, secure link and sales
history. Receipts are **issued once and stored**, so they never change afterwards. Returns, exchanges, cancellations and
recalls produce an **updated (adjustment) receipt** linked to the original. The original is always kept.

These are business sales receipts. They are **not KRA eTIMS tax invoices** and are never labelled as such.

| Part | Where |
|---|---|
| Issuing receipts (original, adjustment), snapshots | `server/src/receipts.rs` (`issue_original`, `issue_adjustment`, `of_sale`) |
| Receipt endpoints: list, secure link, email, public view, logo assets | `server/src/routes/receipts.rs` |
| Issued on: sale, order completion, exchange, return / cancellation / recall | `routes/sales.rs` (`create`, `execute_return`, `execute_exchange`), `routes/orders.rs` |
| Storage | `migrations/0020_receipts.sql` — `receipts`, `receipt_assets`, `sale_returns.points_unrecovered` |
| Receipt settings | `settings.rs` → `sales.receipt`; Settings → Sales & payments → *Receipt configuration* |
| Template (screen) and PDF / print / image | `web/src/components/Receipt.tsx`, `web/src/lib/receipt.ts` |
| Channels panel | `web/src/components/ReceiptPanel.tsx` (sale page, *Sale complete* dialog) |
| Public page | `web/src/pages/portal/ReceiptView.tsx` (`/r/{token}`) |

## 65 — Premium compact receipt

- **Size:** 50 mm wide at most. The height follows the content: the PDF is laid out twice so the page is exactly as tall
  as the receipt.
- **Order:** logo → business name → branch → phone → **SALES RECEIPT** → receipt number, date and time, customer →
  items (item, qty, price, total) → subtotal, discount, points redeemed, **Total** → payments (method, amount,
  reference), balance due on credit → points earned → *Served by* (the **Sale Owner**, module 25) → thank-you message
  → *Digitally signed by [Business]* → a small S' mark.
- **One template:** the screen, PDF, print, image and public page all draw from the same stored snapshot, so every
  channel shows the same receipt. The receipt document itself is not translated, so it reads the same on every channel.
  The buttons and labels around it follow the app's language.
- **Configuration** (Settings → Sales & payments → *Receipt configuration*, live preview): show logo, branch, contact,
  customer, salesperson, loyalty and payment references; thank-you message (the existing receipt footer); font *Clean
  sans* or *Thermal style*. Changes apply to receipts issued from then on; receipts already issued never change.

### Immutability

- The receipt stores a **snapshot** (`receipts.snapshot`) when it is issued. The snapshot holds names, prices, the owner,
  settings and the logo.
- The logo is stored content-addressed in `receipt_assets` (SHA-256), so a later logo change does not alter old receipts.
- Renaming a product, changing the Sale Owner, editing settings or changing the logo does not change an issued receipt.
- One original per sale and one adjustment per return are enforced by unique indexes. Re-issuing is a no-op, so retries
  and double submits never duplicate a receipt.
- Sales recorded before this release get their receipt issued the first time they are viewed or shared.

## 66 — Sharing

| Channel | How | What the app says |
|---|---|---|
| PDF | Generated from the snapshot (jsPDF, lazy-loaded) and downloaded | — |
| Print | The same layout in a hidden frame; the browser's print dialog | — |
| Image | PNG of the on-screen receipt (html2canvas) | — |
| Email | The app builds the PDF and the server attaches it, sent **from the business's name**; logged in the email log (`kind = receipt`) and audited | *Receipt emailed* only after Resend accepts it; otherwise the error |
| WhatsApp | 1) A configured WhatsApp Business number sends the link directly (confirmed by WhatsApp). 2) On phones, the native share sheet shares the **PDF file** itself (choose WhatsApp). 3) Otherwise WhatsApp opens with a short message and the secure link | It never claims the receipt was *sent* unless WhatsApp confirmed it |
| Secure link | `/r/{token}`: an unguessable random token per receipt; view, download PDF, print. No sign-in, no other data | *Secure receipt link copied* |

- The WhatsApp / share message is short: *Hello Ruth! Thank you for shopping with [Business]. Here is your receipt
  RCP-…: https://s-shop.store/r/… We appreciate your business!* The long text receipt was retired.
- Permissions: viewing and sharing need `sales.print` and the sale's data-visibility scope (module 25). The public link
  only reveals that one receipt. It is rate-limited (120 per 10 minutes per client), and unknown tokens return 404.
- Email: at most 20 per user every 10 minutes. The attachment must be a PDF of up to 2 MB, and the address is
  validated.

## 67 — Reconciliation after returns, exchanges, cancellations and recalls

**Nothing changes until final approval.** When `sale.return` needs approval, returns, cancellations and **exchanges** are
submitted to the workflow engine. Stock, payments, loyalty points and receipts stay untouched until the last level
approves. The approval then executes the stored request exactly once, at the sale's branch. Previously, exchanges that
needed approval were refused.

On execution:

- **Returns (partial or full), cancellations and recalls:** stock and refund as before (module 02). The returned lines'
  loyalty points are reversed. If the customer has already spent the points, the shortfall is recorded as
  `points_unrecovered` — an outstanding liability, never silently dropped — and shown on the receipt and in the audit
  entry.
- **Exchanges:** the return plus the replacement sale, paid first by the returned value. The replacement is credited to
  the **original Sale Owner** and earns points under the usual rules, so loyalty nets out. Any value left over is
  refunded.
- **Adjustment receipt** (`kind = adjustment`, number = the return reference `RTN-…`): titled *ADJUSTMENT RECEIPT* or
  *EXCHANGE RECEIPT*, with status *Partially Returned*, *Fully Returned*, *Exchanged*, *Cancelled* or *Recalled*.
  It contains:
  - the original receipt number and date;
  - the items returned and the items remaining;
  - original sale total, amount refunded (this return and all returns so far), customer credit and **net sale value**;
  - for exchanges, the replacement receipt and the difference;
  - who approved it;
  - loyalty: original points, points reversed, points not recoverable and net points.
  The original receipt stays unchanged and one tap away. The panel shows *Original* and *Updated* side by side, and
  the newest is shown first.
- **Reconciliation summary** on the sale page (*Returns & reversals*): original sale total, amount refunded, exchange
  difference, net sale value and the points lines. Amount paid stays as originally paid; refunds are listed separately.
- **Sync:** sales history status, the sale page, customer loyalty history, credit balance, dashboards, reports and
  leaderboards all read the same records, so they agree immediately.

Refunds are recorded as completed by the chosen method (cash, M-Pesa, …). There is no outbound refund integration.

## Audit

| Event | Entry |
|---|---|
| Receipt emailed | `sales.receipt_email` — receipt number, recipient, delivery status (also in the email log) |
| Return / cancellation / recall | `sales.return` / `sales.cancellation` / `sales.recall` — reference, refund, restock, points reversed, **points unrecovered**, status, approval |
| Exchange | `sales.exchange` — returned and replacement receipts, difference, approval |
| Exchange or return awaiting approval | Approval request (`sale.return`) with the full stored request; decisions in the approval history |

## Tests

The smoke suite section *Roadmap 65–67* covers:
- receipt issued with the sale;
- immutability after an owner change;
- the secure link and an unknown link;
- the short WhatsApp message;
- email with the PDF attached from the business name, and a non-PDF refused;
- settings that affect new receipts only;
- the adjustment receipt after a partial return;
- loyalty reconciliation;
- an approval-gated exchange: nothing changes while pending, it executes once on approval, and the replacement is
  credited to the original owner.
