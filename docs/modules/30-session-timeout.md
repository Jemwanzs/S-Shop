# 30 — Session timeout & automatic logout (roadmap 87)

People are logged out automatically after a period without activity. The server enforces it; the browser only shows
the warning and ends the session on time.

| Part | Where |
|---|---|
| Sessions, limits, idle check | `server/src/sessions.rs` (`issue`, `check`, `effective`, `end`, `refresh_limit`, `alive`) |
| Schema | `migrations/0025_user_sessions.sql` — `user_sessions` (kind full / quick / support, `last_active_at`, `idle_seconds`, `expires_at`, `ended_at`, `end_reason` logout / inactivity / replaced), platform limits in `platform_settings['security']` |
| Request check | `server/src/auth.rs` — every staff token carries `ses`; `Ctx` refuses idle sessions with 401 `session_expired`, other ended or expired sessions (and tokens without one) with 401 `unauthorized` |
| Heartbeat, logout, business rule | `server/src/routes/quickpin.rs` — `/auth/activity`, `/auth/logout`, `/security/sessions` |
| Timer, warning, tabs | `web/src/components/IdleGuard.tsx` (in `AppShell`), `web/src/lib/unsaved.ts` |
| Settings | Settings → **Security → Session management** (`web/src/pages/settings/Sessions.tsx`) · Platform → **Security** |

## Limits

- **Platform** (Platform → Security): default timeout (5 min), the range businesses may choose from (1–120 min), the
  default warning (30 s) and the longest session (12 h, even while active).
- **Business** (administrators, full sign-in, never from a support session): 1, 2, 3, 5, 10, 15, 20, 30 or 60 minutes,
  or a custom value within the platform's range. The warning is 10–120 seconds. Sensitive roles can have stricter
  (shorter) timeouts. The card shows the business timeout, your own effective timeout and the longest session.
- A business that has not chosen a timeout has none stored (`security.session` absent) and follows the platform
  default, also after other settings are saved.
- **Effective timeout** = the business timeout, or the role's stricter one, always clamped to the platform's range. The
  warning is never more than half the timeout. Saving shortens open sessions at once; other changes apply at the next
  heartbeat. Saves are audited (`settings.session_policy`).

## Activity

- Activity is only what a person does: taps, clicks, keys, wheel/scroll and typing.
- The app sends a heartbeat (`POST /auth/activity`) at most every 20 s (or a quarter of the timeout) after real input.
  Changes the person makes carry `X-User-Activity: 1`.
- Background reads, polling, notifications, the live event stream and offline sales sync (`X-User-Activity: 0`) never
  extend a session. The live stream closes when its session ends.
- Unsaved work never extends a session.

## Warning and logout

- The warning appears the configured number of seconds before logout: *Your Session Is About to Expire*, a countdown,
  **Stay Logged In** (renews the session) and **Log Out Now**. While it shows, only its buttons count: a stray tap
  neither dismisses it nor swallows *Log Out Now*.
- If the screen has unsaved changes (settings pages, the website editor, the session card), the warning says they are
  not saved automatically. Nothing is ever submitted on the person's behalf. The POS cart is kept on the device as
  before.
- **On expiry**:
  - the server session is ended (`inactivity`, audited `auth.session_expired`);
  - the device's session data (token, cached profile, cached data) is cleared;
  - the sign-in screen shows *Session expired due to inactivity. Please log in again.* until the next sign-in.
  - On a trusted device the Quick PIN keypad is still offered (module 28).
- **Every tab** shares one clock per session in local storage. Activity in any tab keeps all tabs open. The warning
  shows in each, and a logout in one tab (manual or inactivity) signs every tab out.
- **Sleep, locked phones, background tabs**: the timer uses wall-clock time, so a device that wakes after the limit is
  logged out at once.
- **The server agrees**: a session idle longer than its limit (plus 30 s grace for the heartbeat interval) is refused
  and ended on its next request, even if the browser was closed or tampered with.
- **Offline**: the browser timer still logs out without a connection. Offline sales wait in the queue and sync only
  after the next sign-in.
- **Stricter rules apply at once**: a business's shorter timeout and role timeouts, and the platform's lower ceiling
  or shorter longest session, shorten open sessions immediately (idle ones too); longer limits apply at the next
  heartbeat.
- **No extension without a PIN**: switching business, returning from support and opening the platform owner's own
  business continue the session with its original end (never past the longest session counted from the last PIN).
  A Quick session stays Quick. Hand-overs are not audited as sign-outs.
- **Honest messages**: only inactivity answers `session_expired`; a sign-out, a replaced session, the longest session
  or a token from before this release answers `unauthorized` (*Please sign in to continue*).
- Sign-out from the user menu also ends the server session (`POST /auth/logout`). A PIN change, opening another
  business, support sessions and switching business end the old session (`replaced`) and start a new one.

## Roadmap 118 — branch selection, per-person device data & app updates (mobile)

Brief: *Mobile Session Cache, Branch Selection & Login Initialization Fix* (James Sammy, SyncScore). Audit findings and
fixes:

- **Branch carried across sign-ins (the reported bypass)** — the Current Branch was stored on the device and survived
  a sign-out, so the next sign-in (full or Quick PIN, same person or another) could reuse it and skip *Where are you
  working today?*. Now the branch belongs to the session: sign-out, automatic logout, a sign-out in another tab and a
  rejected session all forget it, and every sign-in chooses afresh from the branches the server just returned — the
  only branch, the person's default branch (roadmap 64), otherwise the branch picker. Full sign-in and Quick PIN share
  this one path.
- **Validation** — the server sends only active branches assigned to the person, and checks the branch on every
  request (`X-Branch-Id`); a restored session's branch is checked against the fresh profile and dropped when no longer
  allowed. Screens wait (S'Shop loader) until the profile has loaded, the one-time-PIN check is done and a branch is
  in force: *Authenticate → load the person, permissions and branches → validate the branch → choose if needed → the
  landing page*.
- **Device data per person** — the New Sale cart and stock-count drafts were kept per branch only, so another person on
  the same phone could see them. They are now kept per business, person and branch (a draft saved before this release
  moves over once). The saved-state hook reads the new key's own value when the person, business or branch changes.
  The offline profile copy is tied to its session token; offline sales and product lists are per business.
- **App updates** — every build has a version (`/version.json`). The open app checks it when it comes back to the
  foreground, comes online and every 10 minutes; a newer release shows *A new version of S'Shop is ready* with
  **Refresh**, and is applied by itself on the next change of screen when nothing is unsaved (carts and offline sales
  stay on the device). `index.html` is always revalidated; fingerprinted assets are cached forever; when a new
  release's page arrives, the service worker drops the previous build's files. The service worker never caches API
  responses or credentials.
