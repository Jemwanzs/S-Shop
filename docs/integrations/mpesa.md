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

**Sandbox and Production are kept apart.**
- **Sandbox**: requests always use Safaricom's test shortcode **174379** as a Paybill (`CustomerPayBillOnline`, PartyB
  174379), with the Sandbox app's key, secret and passkey. The business's own Till and Store numbers are never put into
  a Sandbox request: Daraja's Sandbox refuses them with *Invalid TransactionType*.
- **Production**: a Buy Goods Till sends the Store / Head Office number as `BusinessShortCode` and the Till number as
  `PartyB` (`CustomerBuyGoodsOnline`); a Paybill sends its number for both (`CustomerPayBillOnline`). Safaricom must
  have enabled M-Pesa Express (STK Push) for that Till or Paybill — not every Till has it.

Each account shows three checks, apart: **OAuth** (credentials), **STK request accepted** (verification) and **Test
payment confirmed** (the latest test prompt was paid). Daraja's refusals are explained in plain words (transaction type,
passkey, credentials, shortcode, phone, callback). The server logs the environment, transaction type, shortcode, till,
amount and Safaricom's error — never credentials, the generated password or access tokens.

1. **Test connection** checks the consumer key and secret: Safaricom issues an access token. That proves the
   credentials only, not STK Push.
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
the same service, no other one. It serves **only the platform owner's own business and demo businesses** (such as the
Pablo Niche demo, which shows S'Shop to prospects), and only until they add an account of their own; customers'
businesses always use their own accounts. The *Platform M-Pesa (Railway)* card shows to the platform owner also inside
a demo or support session, and says whether the configuration serves that business.

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

## Going live (roadmap 97)

Test in Sandbox first. Then, for a Buy Goods Till approved for *Lipa na M-Pesa Production*:

1. In the business that owns the till (not a demo), go to **Settings → M-Pesa & WhatsApp → M-Pesa → Add account**:
   - *Buy Goods Till*, *Production*;
   - Store / Head Office number (e.g. `7928587`) and Till number (e.g. `4489262`), kept as distinct numbers;
   - the production consumer key, consumer secret and passkey (encrypted, never shown again);
   - all branches, or the branches that use this till.
   Saving never switches STK Push on.
2. **Test OAuth connection** confirms the production key and secret (`https://api.safaricom.co.ke`).
3. **Test STK Push** sends a real prompt of KSh 1–100 to your own phone. The request is sent as `BusinessShortCode` =
   Store number, `PartyB` = Till number, `CustomerBuyGoodsOnline`, with the password built from the Store number,
   the production passkey and the timestamp. Safaricom accepting it verifies the account.
4. Pay the prompt, then **Check payment status**: Safaricom confirms the payment (also arriving by callback with the
   M-Pesa receipt). *Test payment confirmed* turns green.
5. Switch **STK Push** on. Cashiers at those branches now see *Manual M-PESA | STK Push*.

Any of Safaricom's refusals is explained on the card (e.g. a till not enabled for M-Pesa Express). Businesses without
STK keep recording M-Pesa manually.
