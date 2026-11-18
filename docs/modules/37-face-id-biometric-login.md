# 37 — Face ID & Biometric Login (roadmap 132, 155)

Sign in with the device's own **Face ID, Touch ID, fingerprint, Windows Hello or screen lock**, using passkeys
(WebAuthn). The device does the check itself. S'Shop receives only a one-time signature and stores only the passkey's
**public key**. No camera, face image, fingerprint or any other biometric data ever reaches S'Shop.

| Part | Where |
|---|---|
| Set-up, devices, sign-in, rules | `server/src/routes/passkeys.rs` (verification by **webauthn-rs** 0.5) |
| Schema | `migrations/0038_passkeys.sql` — `passkeys` (public key, counter, device name, last used, revoked) and `passkey_challenges` (single-use, 5 minutes); `0044_passkey_discoverable.sql` — challenges issued before the person is known |
| Rules | `platform_settings['security'].passkeys_enabled` · business `settings.security.quick_pin.passkeys` (the Quick PIN's role list applies to both) |
| Browser | `web/src/lib/passkey.ts` (WebAuthn calls, this device's passkey) · `web/src/pages/auth/Login.tsx` · `web/src/pages/settings/Passkeys.tsx` |

## How it works

1. **Set up**: User preferences → **Face ID & Biometric Login**, below Quick Login PIN. It is off by default.
   - The person must have a full sign-in (not a Quick PIN or biometric session) and re-enters their full PIN. A wrong PIN
     is reported in the dialog and the device is never asked.
   - A second button (*Use Face ID / Passkey*, by device) then starts the device's own prompt, which creates a passkey
     for **s-shop.store**. It is a separate tap on purpose; see *Why the tap matters*.
   - The server verifies the answer and keeps the public key with an optional device name. Only then does the switch show on.
   - The browser remembers only which passkey this device set up (its public id), the person's name and business.
2. **Sign in**: the login screen offers the button (*Sign in with Face ID / Passkey*, by device) wherever passkeys are on
   and the device has its own check, or this browser remembers a passkey. It appears on the Quick PIN keypad and above the
   email form.
   - No email is typed or revealed.
   - The device's prompt appears, listing its passkeys for S'Shop if nothing is remembered in this browser; when the check
     passes, the person is signed in.
   - Branch selection, permissions, session limits and audit work as for any sign-in.
   - Cancelling the prompt or failing the check never signs anyone in; the next tap can try again.
3. **Manage**: the card lists registered devices (date added, last used). Each one can be removed from a full sign-in.
   Switching the toggle off removes them all. *Set up this device* adds another phone or computer. A removed passkey also
   leaves the device's own list where the browser supports it (WebAuthn Signal API, feature-detected).

## What each device shows

The wording follows the device family (`passkeyWording` in `lib/passkey.ts`), and never promises a specific biometric: the
browser cannot say which check the device will use.

Roadmap 167: the card's title names the device's own check only where the device family settles it.

| Device | Card title (switch: *Enable …*) | Explanation |
|---|---|---|
| iPhone, iPad | Face ID / Touch ID & Passkey Login | Use your iPhone's (iPad's) Face ID, Touch ID or passcode, whichever it is set up with. |
| Android | Biometric & Passkey Login | Use your phone's fingerprint, face unlock or screen lock, whichever it is set up with. |
| Windows | Windows Hello & Passkey Login | Use Windows Hello (face, fingerprint or PIN), a phone nearby or a security key. |
| Mac, Linux, ChromeOS, other | Device Authentication & Passkeys | Use your device's own check (Touch ID, password or screen lock), a phone nearby or a security key. |

A Mac may have no Touch ID, and no Android phone is called Face ID-capable. The sign-in button reads **Sign in with
Passkey** everywhere; the device's own prompt shows its method.

| State shown | Meaning |
|---|---|
| Ready to set up | No passkey yet (from a Quick PIN session the card says to sign in fully first) |
| Setting up | The set-up dialog is open |
| Enabled | The server lists the passkey this browser remembers |
| Enabled on another device | The server lists passkeys, none known to this browser (it may still be on this device: Safari and the home-screen app do not share storage) |
| Credential removed | This browser remembers a passkey the server no longer has (removed, PIN changed, signed out everywhere) |
| Setup failed | The last set-up here was refused by the device or the server (a cancel is not a failure) |
| Authentication unavailable | No WebAuthn in this browser, another address than S'Shop's own, or switched off by the platform, the business or for this role |

The state always follows the server's list of passkeys, never the switch: turning it on opens set-up, and *Enabled*
appears only after the server has verified the passkey.

**Sign-in that does not go through (roadmap 167).** No session is ever opened without the server's verified answer.
The sign-in page shows **Authentication unsuccessful** with the reason, **Try again** (a direct tap, so iPhones show the
prompt) and **Use another sign-in method** (email and PIN, or the Quick PIN keypad):

| Case | Message | Try again |
|---|---|---|
| Cancelled, timed out, or the device check not passed (the browser does not say which, on purpose) | The check was cancelled or timed out on your device. Nothing was sent. | Yes, with the same challenge |
| Device or browser cannot (no passkey support, wrong address) | The device's reason | No |
| Passkey removed or revoked | This device's passkey is no longer active. Sign in with your email and PIN, then set it up again. | No: the full sign-in opens |
| Server refused the answer | We couldn't verify your device authentication. Please try again or use another sign-in method. | Yes, with a new challenge |
| Too many attempts | The rate-limit message | No |

S'Shop cannot tell whether a face matched: the device decides and only says the check passed.

## Why the tap matters, and the home-screen app

- **The tap.** iPhones show the prompt only straight after a tap and refuse otherwise, with the same error as a cancel
  (`NotAllowedError`). So nothing is awaited between the tap and the device call: the PIN check and challenge come first
  (set-up in two steps; the sign-in challenge is fetched when the page opens, and renewed before its 5 minutes run out).
- **Storage.** Safari and the home-screen app have separate storage, and browsers can clear it. The passkey itself is kept by
  the device (and its keychain), not by the page, so sign-in does not depend on what the page remembers: with nothing
  remembered, the server issues a `discover` challenge and the device offers its passkeys. New passkeys are requested as
  discoverable (*preferred*). Passkeys made before this that are not discoverable, on some Android devices and security keys,
  still work from the browser that remembers them; if it forgets, set the device up again.
- **The address.** The relying party is `s-shop.store` (`PUBLIC_URL`'s host unless `WEBAUTHN_RP_ID` is set), exact origin,
  no subdomains; the sign-in page learns it from `GET /api/auth/passkey/support`. The app's manifest, HTTPS, and the
  security headers (no `publickey-credentials` restriction in `Permissions-Policy`) allow WebAuthn. Offering the app on
  another address, such as `s-shop.click` or a business's domain, does not offer passkeys.
- **Updates.** The service worker caches only the app files, never API answers, and nothing about passkeys; a new release does
  not affect them. Quick sessions persist like any other session after the app closes.

## If a device is lost or replaced

Passkeys are optional: email and PIN always work, and no device can lock anyone out. Sign in with email and PIN anywhere, open
preferences and remove the lost device; the others keep working. Changing or resetting the full PIN, or *Sign out everywhere*,
ends every passkey of the person; an administrator resetting someone's PIN does the same. A new device is set up the normal way.

## Real-device verification

The automated tests run real ceremonies against a software authenticator and Chrome's virtual authenticator, emulating each
device family. They prove S'Shop's flows, not the device's own behaviour, so **iPhone and Android are not verified until this
table is done on real devices** (status as of the last release):

| Scenario | iPhone Safari | iPhone home-screen app | Android Chrome | Android installed app | Windows Hello |
|---|---|---|---|---|---|
| Set up: the prompt appears, the state becomes *Enabled on this device* | to do | to do | to do | to do | to do |
| Sign out, then sign in with the button | to do | to do | to do | to do | to do |
| Set up in one, sign in from the other (Safari ↔ app, browser ↔ installed) | to do | to do | to do | to do | n/a |
| Cancel the prompt: a clear message, PIN still works, the next tap works | to do | to do | to do | to do | to do |
| Device check unavailable (no Face ID or screen lock enrolled) | to do | to do | to do | to do | to do |
| Close and reopen the app: session kept; after a release: still signs in | to do | to do | to do | to do | n/a |
| Remove a device: others keep working; the removed one is refused or gone | to do | to do | to do | to do | to do |

If the prompt does not appear on a real device, note the device, its system version, the exact message and whether it was the
browser or the home-screen app.

## Limits

- A biometric session is a **limited (quick) session**, exactly like a Quick PIN session:
  - payments, users, roles and access, PIN changes, security settings and platform administration need the full PIN
    (*Full sign-in needed*);
  - *Sign in fully* re-enters the full PIN (`/auth/step-up`).
- **Quick PIN and biometrics are independent**: either can be on without the other.
- Passkeys work only on S'Shop's own address (`s-shop.store`). The browser refuses them anywhere else, including a
  business's custom website domain.
- Built for iPhone and iPad (Safari, and the home-screen app), Android (Chrome, installed app), Mac (Safari, Chrome) and
  Windows (Windows Hello), following the WebAuthn standard; see *Real-device verification* for what has been tried on
  real devices. A browser without passkey support shows *Unsupported browser or device* and offers no button.
- A *Passkey not recognised* answer means the device offered a passkey S'Shop no longer has; the person signs in with their PIN.

## Ends and revocation

| Event | Effect |
|---|---|
| The person removes a device / switches it off (full sign-in needed) | That device / every device can no longer sign in; the others are untouched by a single removal |
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
  - the credential's owner, including for a passkey the device offers: the person the answer names must hold that passkey
    on record, and the signature is checked against the stored public key (roadmap 155);
  - the signature counter: a cloned passkey that goes backwards is refused.
- Rate-limited per client (answers 30 per 5 minutes; challenges 120, since each open sign-in page fetches one). At most 10
  passkeys per person.
- Passkeys belong to the person's own sign-in account, never to a linked row in another business. One person can
  never see or remove another's.
- Audit events: `passkey_added`, `passkey_removed`, `passkeys_disabled`, `login` (method `passkey`) and
  `passkey_login_failed`. They never contain key material or biometric data.

## Operations

- The server image needs `libssl3` (already in the `Dockerfile`).
- On Windows development machines, OpenSSL comes from `OPENSSL_DIR` / `OPENSSL_STATIC` in the user's cargo
  configuration. To build it once with `openssl-src`, use Strawberry Perl and a target path without spaces.
- Test suite: a software authenticator (P-256, `none` attestation) in `scripts/smoke_test.py` runs real ceremonies.
  They cover registration, sign-in (remembered and device-offered), wrong origin or RP ID, no user verification, another key,
  replay, cloned counter, a passkey claiming another person, an unknown one, rules, limited sessions, several passkeys, removal
  and PIN change.
