# M-Pesa (Safaricom Daraja — Lipa na M-Pesa Online / STK Push)

Each business connects **its own** Buy Goods Till or Paybill (roadmap 91, [module 32](../modules/32-mpesa-stk.md)).
The sale never depends on the integration alone: manual M-Pesa codes keep working.

1. **STK Push sale**: the cashier chooses *M-Pesa → STK Push*, enters the customer's phone and taps *Send STK Push*.
   The server validates the whole sale, holds its stock and sends the prompt. When Safaricom confirms the payment, the
   sale completes by itself, once, with the M-Pesa receipt number.
2. **Pay first**: deposits on credit sales, credit repayments and orders push a prompt for an amount, then use the
   confirmed payment.
3. **Manual confirmation** (Settings → Sales → *Allow manual M-Pesa confirmation*): the cashier types the code from the
   customer's SMS (e.g. `QFT1ABC2DE`).

An M-Pesa code can settle only **one** payment (database unique index). A confirmed prompt can be used once, only for
the amount Safaricom reported, never if it was a test prompt, and a waiting sale's prompt only by that sale.

## Setting up a business (Settings → M-Pesa & WhatsApp → M-Pesa)

| Field | Notes |
|---|---|
| Type | *Buy Goods Till* (`CustomerBuyGoodsOnline`) or *Paybill* (`CustomerPayBillOnline`) |
| Store / Head Office number, or Paybill number | The Lipa na M-Pesa Online shortcode (`BusinessShortCode`) |
| Till number | Buy Goods only: the number customers pay (`PartyB`) |
| Account reference | Paybill only (optional, up to 12 characters) |
| Environment | Sandbox (testing) or Production |
| Consumer key, consumer secret, passkey | From the Daraja app. Encrypted when saved (AES-256-GCM) and never shown again; empty on edit keeps them |
| Branches | All, or chosen branches (a branch-specific account wins over an all-branches one) |

1. **Test connection** checks the consumer key and secret: Safaricom issues an access token.
2. **Send test prompt** sends a KSh 1 prompt to a phone. Safaricom accepting it proves the shortcode, passkey and
   till together, and the account becomes **verified**. Nobody has to pay; cancel it on the phone.
3. **STK Push** can then be switched on. It switches off by itself (verification cleared) whenever the type,
   numbers, environment or credentials change.

Who can manage accounts: people with *M-Pesa & WhatsApp settings*, fully signed in (not a Quick PIN session), never from
a support session. People with sales or integration settings can see the accounts (secrets masked) and the activity.

## Callbacks

Each account has its own secret callback URL:
`https://<your-domain>/api/webhooks/mpesa/a/<account-id>/<token>`. The token is random, stored encrypted and as a
SHA-256 digest, and is compared in constant time. Daraja does not sign callbacks, so the secret path authenticates
them. A callback is applied only to the request with the same checkout id **and** the same account. Repeats change
nothing.

## Flow & states

`pending` → `success` (paid; receipt stored), `cancelled` (1032), `timeout` (1037, or no answer after 3 minutes) or
`failed` (anything else).

- **Late or lost callbacks**: after 20 s without a callback, the till's status check (and a server sweep every
  minute) asks Daraja. `ResultCode 0` from the status query also counts as paid; the M-Pesa receipt number is added
  when the callback arrives.
- **Amounts**: rounded **up** to whole shillings (Daraja accepts integers only). The amount Safaricom reports is
  stored separately and must cover what is due.

## Configuration (environment variables)

| Variable | Notes |
|---|---|
| `DATA_ENCRYPTION_KEY` | Recommended: 32 bytes (base64 or 64 hex characters) for encrypting stored credentials. Without it a key is derived from `JWT_SECRET`, so rotating `JWT_SECRET` (or setting this key later) means re-entering M-Pesa credentials. |
| `MPESA_SANDBOX_URL`, `MPESA_PRODUCTION_URL` | Optional Daraja base-URL overrides (tests use a stand-in) |
| `MPESA_ENVIRONMENT`, `MPESA_CONSUMER_KEY`, `MPESA_CONSUMER_SECRET`, `MPESA_STK_PASSKEY`, `MPESA_SHORTCODE`, `MPESA_CALLBACK_URL` | The platform's own Daraja app (roadmap 92) — see below. |

## The platform's own Daraja app (Railway variables, roadmap 92)

S'Shop's own Daraja app (e.g. the Sandbox app with M-Pesa Express) is read from the `sshop` service's variables —
the same service, no other one. It serves **only the platform owner's own business**, and only until that business adds
an account of its own; other businesses always use their own accounts.

| Variable | Value |
|---|---|
| `MPESA_ENVIRONMENT` | `sandbox` (default) or `production` |
| `MPESA_CONSUMER_KEY`, `MPESA_CONSUMER_SECRET` | From the Daraja app |
| `MPESA_STK_PASSKEY` | The Lipa na M-Pesa Online passkey (Sandbox: shown on the Daraja simulator page) |
| `MPESA_SHORTCODE` | Sandbox: `174379` |
| `MPESA_CALLBACK_URL` | `https://s-shop.store/api/webhooks/mpesa/<secret>` — the secret is at least 24 letters, digits, `-` or `_` (generate one; it authenticates Safaricom's calls) |
| `MPESA_TRANSACTION_TYPE`, `MPESA_PARTY_B` | Optional: `CustomerPayBillOnline` (default, Paybill — Sandbox 174379) or `CustomerBuyGoodsOnline` with the Till number in `MPESA_PARTY_B` |

Earlier names still work: `MPESA_ENV`, `MPESA_PASSKEY`, `MPESA_CALLBACK_TOKEN` (+ `PUBLIC_URL`).

- **Checked at start-up.** Missing or wrong values (no HTTPS, a guessable callback path, a bad shortcode) switch only
  the platform M-Pesa off — never the server — and the reason is shown on the platform's card and logged.
- **Platform → Settings → M-Pesa & WhatsApp → Platform M-Pesa (Railway)** (platform owner only):
  - shows the environment, the shortcode and the callback URL with its secret masked, and warns when that URL is not
    this server's address;
  - **Test OAuth** (Safaricom issues a token);
  - **Test STK prompt** (KSh 1 — its result arrives on the callback URL and appears in M-Pesa activity).
- **Access tokens** are cached per app and renewed a minute before they expire.

## Going live

Test in Sandbox with the Daraja test credentials and phone numbers. For production, edit the account: choose
*Production*, enter the production credentials (after Safaricom's *Go Live* approval), send a test prompt to verify,
then switch STK Push on. No code changes are needed.
