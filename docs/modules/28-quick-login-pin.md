# 28 — Quick Login PIN (roadmap 83)

A 4–6 digit PIN for fast sign-in on devices a person trusts. The full **email + PIN** sign-in stays the default and is
always required first on a new or shared device.

| Part | Where |
|---|---|
| Rules, set-up, devices, Quick sign-in, resets | `server/src/routes/quickpin.rs` |
| Schema | `migrations/0023_quick_pin.sql` — `users.quick_pin_hash / quick_failed / quick_locked_until`, `trusted_devices`, `platform_settings['security']` |
| Quick sessions & full-sign-in guard | `server/src/auth.rs` (`Claims.qp`, `Ctx.quick`, `Ctx::require_full`, `issue_quick_token`) |
| Sign-in screen (keypad) | `web/src/pages/auth/Login.tsx`, `web/src/components/PinPad.tsx`, `web/src/lib/quick.ts` |
| Settings | User preferences → **Quick Login PIN** · Settings → **Security** (business rule + support access) · Platform → **Security** · Settings → Users (*Reset Quick PIN*) — `web/src/pages/settings/QuickPin.tsx` |

## How it works

1. **Set up** (User preferences, full sign-in): the person's current full PIN, then a new Quick PIN twice and an
   optional device name. The Quick PIN must be digits only, from the platform minimum (4, 5 or 6) up to 6, and cannot
   be a single repeated digit (1111), a straight run (1234, 9876) or the full PIN. This browser becomes a **trusted
   device**:
   - the server returns a random device secret once, and stores only its SHA-256;
   - the browser keeps the secret with the person's name, email and business. It never keeps a PIN.
2. **Other devices**: *Trust this device* asks for a full sign-in on that device and the full PIN.
3. **Sign in**: on a trusted device the login screen opens on the keypad, with:
   - *Welcome back, Name*;
   - masked dots, automatic submit at 6 digits (or OK);
   - *Use password instead* and *Forgot PIN?*, which leads to a full sign-in and then a new Quick PIN.
   The method last used on the device is remembered. *Login with Quick PIN* is offered under the full form.
4. **Quick sessions** are marked (`Ctx.quick`, `profile.quick`). The user menu shows *Signed in with Quick PIN · Sign in
   fully*. They last up to the platform's *Quick sign-in lasts* hours (default 12). Switching business keeps them
   Quick — never upgraded.

## Full sign-in still required

The server refuses these from a Quick session with *Full sign-in needed*:
- payments and accepting quotations;
- creating or changing users, roles, user access, PIN resets and linking people;
- changing the full PIN;
- Quick PIN set-up and trusting devices;
- the business's security and support-access rules, and decisions on support sessions;
- every platform-owner endpoint.

## Ends, locks and revocation

| Event | Effect |
|---|---|
| Wrong Quick PIN × *max attempts* (default 5) | 15-minute lock (the right PIN is refused too); the full sign-in still works |
| Wrong × twice the limit | This device is revoked — full sign-in needed |
| Full PIN changed, reset by link, reset by an administrator, replaced by a one-time PIN | Every trusted device revoked (and older sessions end) |
| Quick PIN changed | Other devices revoked; the current one is trusted again |
| *Switch off* / administrator *Reset Quick PIN* | Quick PIN deleted, every device revoked |
| *Sign out everywhere* | Every device revoked and every session ended (this one too) |
| Device lifetime (default 90 days) | Expires — full sign-in |
| Person deactivated / one-time PIN pending / business deactivated | Quick sign-in refused |
| Business turns it off / narrows roles; platform turns it off | Affected devices revoked at once; rules re-checked at every Quick sign-in |

## Rules

- **Platform → Security**:
  - Quick PIN on or off;
  - shortest PIN (4 / 5 / 6);
  - Quick session length (1–24 h);
  - wrong attempts before a lock (3–10);
  - trusted-device lifetime (7–365 days).
- **Settings → Security** (business administrators, fully signed in, never from a support session): allow Quick PIN
  sign-in, and for which roles (none selected = every role). This can only narrow the platform's rules.
- **Nobody can see a Quick PIN.** Administrators and the platform can only switch it off or reset it. A person linked
  from another business is reset in their own business.

## Security & audit

- Quick PINs are stored as salted Argon2 hashes, like full PINs. Device secrets are stored as SHA-256 only.
- Quick sign-in is rate-limited per client, on top of the per-person lock.
- Audit events are recorded without any PIN or secret: `quick_pin_enabled` / `changed` / `disabled`, `device_trusted`,
  `device_revoked`, `signed_out_everywhere`, `login` (method `quick_pin`), `quick_login_failed` (attempt, lock, device
  revoked), `users.quick_pin_reset`, `settings.quick_pin_policy`, `platform.security_settings`.
- There is no MFA today, so there is nothing for a Quick PIN to bypass. When MFA is added, Quick sign-in must stay
  limited to what MFA allows.
