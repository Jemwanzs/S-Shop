# 37 — Face ID & Biometric Login (roadmap 132)

Sign in with the device's own **Face ID, Touch ID, fingerprint, Windows Hello or screen lock**, using passkeys
(WebAuthn). The device does the check itself. S'Shop receives only a one-time signature and stores only the passkey's
**public key**. No camera, face image, fingerprint or any other biometric data ever reaches S'Shop.

| Part | Where |
|---|---|
| Set-up, devices, sign-in, rules | `server/src/routes/passkeys.rs` (verification by **webauthn-rs** 0.5) |
| Schema | `migrations/0038_passkeys.sql` — `passkeys` (public key, counter, device name, last used, revoked) and `passkey_challenges` (single-use, 5 minutes) |
| Rules | `platform_settings['security'].passkeys_enabled` · business `settings.security.quick_pin.passkeys` (the Quick PIN's role list applies to both) |
| Browser | `web/src/lib/passkey.ts` (WebAuthn calls, this device's passkey) · `web/src/pages/auth/Login.tsx` · `web/src/pages/settings/Passkeys.tsx` |

## How it works

1. **Set up**: User preferences → **Face ID & Biometric Login**, below Quick Login PIN. It is off by default.
   - The person must have a full sign-in (not a Quick PIN or biometric session) and re-enters their full PIN.
   - The device then shows its own prompt (Face ID, fingerprint, Windows Hello…) and creates a passkey for
     **s-shop.store**.
   - The server verifies the answer and keeps the public key with an optional device name.
   - The browser remembers only which passkey this device set up (its public id), the person's name and business.
2. **Sign in**: the login screen shows *Sign in with Face ID / Biometrics* only on a device that set one up. It appears
   on the Quick PIN keypad and above the email form.
   - No email is typed or revealed.
   - The device's prompt appears; when the check passes, the person is signed in.
   - Branch selection, permissions, session limits and audit work as for any sign-in.
   - Cancelling the prompt or failing the check never signs anyone in.
3. **Manage**: the card lists registered devices (date added, last used). Each one can be removed. Switching the toggle
   off removes them all. *Set up this device* adds another phone or computer.

## Limits

- A biometric session is a **limited (quick) session**, exactly like a Quick PIN session:
  - payments, users, roles and access, PIN changes, security settings and platform administration need the full PIN
    (*Full sign-in needed*);
  - *Sign in fully* re-enters the full PIN (`/auth/step-up`).
- **Quick PIN and biometrics are independent**: either can be on without the other.
- Passkeys work only on S'Shop's own address (`s-shop.store`). The browser refuses them anywhere else, including a
  business's custom website domain.
- Supported on iPhone and iPad (Safari, and the home-screen app), Android (Chrome), Mac (Safari, Chrome) and Windows
  (Windows Hello). A browser without passkey support simply doesn't offer it.

## Ends and revocation

| Event | Effect |
|---|---|
| The person removes a device / switches it off | That device / every device can no longer sign in |
| Full PIN changed, reset by link, reset by an administrator, replaced by a one-time PIN | Every passkey of the person revoked |
| *Sign out everywhere* | Every passkey revoked and every session ended |
| Person deactivated / one-time PIN pending / business deactivated | Biometric sign-in refused |
| Business switches it off or narrows roles; platform switches it off | Refused at sign-in and set-up (rules checked every time) |

## Security & audit

- Verification is server-side with **webauthn-rs**, a maintained and independently reviewed library. On every
  ceremony the server checks:
  - the challenge: random, single use, expires in 5 minutes and is deleted even after a wrong answer;
  - the origin and the RP ID (`WEBAUTHN_RP_ID`, otherwise the host of `PUBLIC_URL`);
  - user verification (a presence-only touch is refused);
  - the credential's owner;
  - the signature counter: a cloned passkey that goes backwards is refused.
- Rate-limited per client. At most 10 passkeys per person.
- Passkeys belong to the person's own sign-in account, never to a linked row in another business. One person can
  never see or remove another's.
- Audit events: `passkey_added`, `passkey_removed`, `passkeys_disabled`, `login` (method `passkey`) and
  `passkey_login_failed`. They never contain key material or biometric data.

## Operations

- The server image needs `libssl3` (already in the `Dockerfile`).
- On Windows development machines, OpenSSL comes from `OPENSSL_DIR` / `OPENSSL_STATIC` in the user's cargo
  configuration. To build it once with `openssl-src`, use Strawberry Perl and a target path without spaces.
- Test suite: a software authenticator (P-256, `none` attestation) in `scripts/smoke_test.py` runs real ceremonies.
  They cover registration, sign-in, wrong origin or RP ID, no user verification, another key, replay, cloned counter,
  rules, removal and PIN change.
