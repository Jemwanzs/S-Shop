# 32 — M-Pesa per business & STK Push sales (roadmap 91)

Each business takes M-Pesa into its own Till or Paybill. A cashier enters the customer's phone and sends an STK Push;
S'Shop completes the sale only after Safaricom confirms the payment.

| Part | Where |
|---|---|
| Daraja client, per-account credentials, which account serves a branch | `server/src/integrations/mpesa.rs` (`Creds`, `for_branch`, `for_request`, `stk_branches`) |
| Encryption at rest | `server/src/secrets.rs` (AES-256-GCM, `DATA_ENCRYPTION_KEY`) |
| Accounts, test connection / prompt, STK switch, activity, results | `server/src/routes/payments.rs` (`apply_result`, `reconcile`, `CONSUMABLE`) |
| Sales waiting for their STK Push | `server/src/routes/stk_sales.rs` (`start`, `settle`, `complete`, `sweep`) |
| One sale path for the counter and STK sales | `server/src/routes/sales.rs` (`create_in_tx`; `PaymentInput.dry_run`) |
| The cashier's context for completion | `server/src/auth.rs` (`CtxSnapshot`) |
| Callbacks | `server/src/routes/webhooks.rs` (`/webhooks/mpesa/a/{account}/{token}`) |
| Schema | `migrations/0026_mpesa_accounts.sql` — `mpesa_accounts`, `pending_sales`, `mpesa_requests.account_id / purpose / paid_amount / confirmed_by` |
| Screens | Settings → M-Pesa & WhatsApp (`web/src/pages/settings/Mpesa.tsx`); till (`web/src/pages/sales/pos/Checkout.tsx`, `Pos.tsx`) |

Setup, callbacks and environment variables: [integrations/mpesa.md](../integrations/mpesa.md).

## At the till

- **Payment choice**: *M-Pesa* offers **Manual M-PESA | STK Push** when the business has a verified account with STK
  on for the branch. If manual confirmation is off, only STK Push is offered. If STK isn't available, only manual.
- **STK Push**:
  - The cashier enters the customer's phone (`07…`, `01…` or `254…`, normalised) and sees the amount due.
  - **Send STK Push** shows the states *Pending → Successful* (the receipt opens by itself), or *Failed*,
    *Cancelled* or *Timed out* (with **Retry**).
  - **Stop waiting** releases the sale. While a sale waits, the cart is locked.
- **No duplicates**: a second tap returns the waiting sale, with no second prompt. The cart reference allows only one
  waiting sale, and a completed cart returns its sale.

## On the server

1. **Validate**: the sale is recorded inside a transaction exactly as if completing it (customer, trading hours,
   approvals, prices and discounts, stock, points), then **rolled back**. The amount due and the stock to hold come
   from that dry run. A supervisor's approval is verified here, and only the approver's id is kept, never their PIN.
2. **Hold**: the stock is reserved with the stock engine (`inventory::reserve`) and the waiting sale is stored with the
   sale body and the cashier's context. No locks are held during the call to Safaricom.
3. **Prompt**: sent through the account serving the branch. If Safaricom refuses it, the stock is released at once and
   the error is kept on the account.
4. **Confirm**: only Safaricom marks a payment paid, through the callback or a status query. Accepting the prompt is
   never payment.
5. **Complete, exactly once**: the waiting sale row is locked, the payment is checked (confirmed, unused, enough), the
   hold is released and the sale is recorded through the normal counter path (`create_in_tx`) with the receipt. Stock,
   payments, reports, loyalty and audit (`sales.stk_sale_completed`) are updated as for any sale.
6. **Not paid**: failed, cancelled or timed out releases the stock. The attempt is kept for the record.
7. **Needs attention**: a payment that can't become a sale (paid after the cashier stopped waiting, less than due,
   prices changed meanwhile) is never lost.
   - It is flagged, audited (`sales.stk_sale_attention`) and notified to M-Pesa managers and the cashier.
   - An authorised person completes it from **M-Pesa activity**, or refunds it from the M-Pesa portal.

## Security

- **Credentials**: encrypted at rest and never returned. The consumer key shows only its last 4 characters. Audit
  entries record what changed, never secrets. Debug output of the server's configuration redacts every secret.
- **Callbacks**: each account has its own secret callback URL, matched to the request and account that sent it.
  Repeats are idempotent.
- **Isolation**: every query is scoped to the business, and branches only use their assigned account.
- **Verification**: STK Push stays off until verified (database check: `stk_enabled` needs `verified_at`).
- **Sandbox first**: each account has its own environment.

## Tests

- **Smoke suite**, section *Roadmap 91*, with a Daraja stand-in:
  - validation, masking, a wrong passkey refused, verification, STK off until verified;
  - Buy Goods fields and the callback URL, forged callbacks;
  - stock held and released, no second prompt, completion with the receipt, repeated callbacks;
  - cancellation, a lost callback recovered by status query with a late receipt, less paid, paid after cancelling;
  - test prompts unusable, permissions, another business, deactivation.
- **Unit**: `secrets.rs` (round trip, tamper detection, key formats).
