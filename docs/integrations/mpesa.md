# M-Pesa (Safaricom Daraja — Lipa na M-Pesa Online / STK Push)

M-Pesa works in two ways; the sale never depends on the integration alone.

1. **STK Push** (when configured): the cashier taps *Push STK*, the customer enters their M-Pesa PIN on their phone,
   Daraja calls S'Shop back and the POS turns green with the M-Pesa receipt number.
2. **Manual confirmation** (Settings → Sales → *Allow manual M-Pesa confirmation*): the cashier types the code from the
   customer's SMS (e.g. `QFT1ABC2DE`).

Either way an M-Pesa code can settle only **one** payment (database unique index), and an STK confirmation must cover
the sale total and can be used once.

## Configuration (environment variables)

| Variable | Notes |
|---|---|
| `MPESA_ENV` | `sandbox` (default) or `production` |
| `MPESA_CONSUMER_KEY`, `MPESA_CONSUMER_SECRET` | From the Daraja app |
| `MPESA_SHORTCODE` | Paybill / Head-office shortcode (sandbox: `174379`) |
| `MPESA_PASSKEY` | Lipa na M-Pesa Online passkey |
| `MPESA_TRANSACTION_TYPE` | `CustomerPayBillOnline` (Paybill) or `CustomerBuyGoodsOnline` (Till) |
| `MPESA_PARTY_B` | Till number for Buy Goods (defaults to the shortcode) |
| `MPESA_CALLBACK_TOKEN` | Long random string; forms the secret callback URL |

Callback URL registered automatically with each request:
`https://<your-domain>/api/webhooks/mpesa/<MPESA_CALLBACK_TOKEN>`
(Daraja does not sign callbacks, so the secret path authenticates them; it must be HTTPS and public.)

## Flow & states

`POST /api/mpesa/stk` → `pending` → callback → `success` (receipt stored) / `cancelled` (1032) / `failed`.
If no callback arrives, the POS polls `GET /api/mpesa/stk/{id}`; after 20 s the server queries Daraja, after 3 min
the request is marked `timeout`. Successful requests are consumed by the sale or credit repayment that uses them.
Amounts are rounded **up** to whole shillings (Daraja accepts integers only).

## Going live

Test in sandbox with the Daraja test phone numbers, then switch `MPESA_ENV=production` with production credentials
(Safaricom “Go Live” approval). No code changes are needed.
