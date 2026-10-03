# Migrating from Pablo Loyalty

The legacy app (Lovable + Supabase, `points-hub.lovable.app`) is imported with one command. The import is **read-only on
the source** and runs in **one transaction** on S'Shop: it either completes fully or changes nothing.

```bash
sshop import-legacy "postgresql://postgres:<password>@db.<project>.supabase.co:5432/postgres" --tenant <slug>
```

- `--tenant` — the business slug to import into (default `pablo-loyalty`, created if missing).
- Refuses to run twice into the same business (checks for existing legacy sales).
- Get the connection string from Supabase → Project settings → Database.

## Mapping

| Legacy | S'Shop |
|---|---|
| `app_users` (plain-text PIN, fixed role) | `users` with Argon2-hashed PIN; roles mapped: admin → Tenant Administrator, manager → Manager, assistant_manager → Branch Manager, edit_update → Salesperson, print_communicate → Auditor, view_only → View Only. Existing emails are skipped. |
| `products` (code, name, threshold, points_per) | `products` with the same code; threshold/points become the product's loyalty rule. No stock (the old app had none). |
| `customers` | `customers`; mobile normalised (2547…); location/city/country kept as custom fields; spend, points, redeemed carried over; own vs referral points split. Unusable mobiles are listed and skipped. |
| `referrals` | `referrals` + one `referral` ledger entry per bonus |
| `transactions` + `transaction_items` | Legacy sales `LEG-000001…` (`is_legacy`, payment “Legacy import”) with items, plus `earn` ledger entries. **No stock movements.** |
| balances | A final `adjust` ledger entry per customer reconciles the ledger to the legacy balance. |
| `award_periods`, `award_winners` | Copied with ranks |

## Behaviour changes users will notice

- Sign-in is email + PIN as before, but PINs are hashed and 5 wrong attempts lock the account for 15 minutes.
  The hard-coded admin secret is gone; administrators reset PINs in Settings → Users or with `sshop reset-pin`.
- Opening a new award round **no longer resets everyone's points or deletes transactions**.
- Deleting customers, products and referrals is replaced by deactivation, keeping history intact.
