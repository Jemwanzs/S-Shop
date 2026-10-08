# 22 — Action button states & floating quick actions (roadmap 47–48)

## 47 — Action button state system

One component for every button that commits or triggers something: `web/src/components/ActionButton.tsx`.
Screens state *what must be true first*; the button derives everything else.

```
permission (perm) ─────────┐
connectivity (online) ─────┤
blockers: record / workflow├─▶ hidden │ unavailable (short reason = label) │ ready │ processing │ done │ ready
  state, unsaved changes,  │
  validation, configuration│
backend processing ────────┘  (busy, or the promise returned by onAction)
```

```tsx
<ActionButton
  perm="stock.transfer"                       // hidden without it (module-aware through the session)
  online                                       // "Offline" without a connection
  blockedBy={[!valid && REASONS.completeFields, !dirty && REASONS.nothingToSave]}
  busyLabel="Saving…" doneLabel="Saved"
  onAction={() => save.mutateAsync(draft)}     // return the promise
>Save</ActionButton>
```

* **Blockers** — the first string wins and becomes the label (and tooltip): *Nothing to save*, *No changes*, *Select a
  file*, *Complete required fields*, *Enter a reason*, *Awaiting approval*, *Nothing to receive*, *No outstanding
  balance*, *STK not configured*, *No records to export*, *Cart is empty* (`REASONS`), or a short screen-specific one
  (*Enter the M-Pesa code*, *Scan the items*). *Nothing to save* is used only for editable forms (with `isDirty(saved,
  draft)`); workflow actions use their real state.
* **Processing** — while `onAction`'s promise runs (or `busy` is set) the button is disabled and shows a spinner and its
  `busyLabel`; a second click before React re-renders is also ignored. Success (`doneLabel`) appears only after the promise
  resolved, i.e. the server confirmed. On failure the button is ready again and the form is untouched; the screen's
  `onError` shows the message.
* **Lifecycles** — e.g. transfers: *Submit for approval* → *Awaiting approval* → *Dispatch* / *Awaiting dispatch* →
  *In transit* → *Confirm receipt*: the next step is the action when this user may take it, otherwise its waiting state.

Converted: the settings save bar, the customer ordering portal (continue, verify, *Submit order*), `ConfirmDialog` (cancel, void, write-off, deactivate, PIN reset …), POS *Complete sale*
and *Push STK*, exchange, order status moves, phone orders, product form, receive stock, stock count, adjustments,
transfers (new / submit / dispatch / receive / cancel), credit repayments and recall, customers and points, expenses,
loyalty referrals and award periods, approvals (approve / reject / withdraw), users, roles, branches, PIN reset, custom
fields, workflows, preferences, access requests, billing (*Pay now*, accept quotation, plan, issue, record payment, void,
verify), vendor details, photo uploads, report export, date-range *Apply*, notifications *Mark all read*, opening a
business. New screens use `ActionButton` instead of wiring `disabled`/spinners by hand.

### Duplicate submissions — server side

`server/src/dedupe.rs` guards every write request (POST/PUT/PATCH/DELETE under `/api`, except sign-in, webhooks and the
event stream). A request is fingerprinted by sender (session token, else client address), method, path + query, branch
header, an optional `Idempotency-Key` header and body (the web app sends no key, so a double click is recognised by its
content; a client that names its requests — the test suite does — decides itself what counts as the same request):

* identical request **still running** → 409 *Already processing*;
* identical request within **3 seconds after it succeeded** → the same response again (`x-duplicate: replayed`), nothing
  runs twice;
* failures are not remembered → retrying after an error works normally.

This sits in front of the domain-level protections that already exist (sales `client_ref`, Paystack references, unique
constraints, row locks).

## 48 — Floating quick sale

`web/src/components/FloatingQuickAction.tsx` (reusable) + `QuickSaleShortcut` in the app shell.

* Small warm-brown bubble (token `--fab`), centre of the right edge, above content but below dialogs; tooltip
  *Record sale* on desktop. On phones and tablets it is tucked into the screen edge as a tab (30 of 44 px showing, in the
  page margin) so it does not cover buttons or figures at the end of rows.
* Shown only when the user may record a sale now: `sales.create` (module-aware, so the Sales/POS package too), a Current
  Branch, billing not suspended, and — when the business blocks sales outside trading hours and the user has no override —
  an open branch. Hidden on the Record Sale screen itself. Offline: the existing Record Sale screen's offline rules apply.
* Tap/click opens the existing Record Sale screen (`/pos`) — no separate flow.
* *Settings → User preferences → Quick actions*: **Allow the floating sale button to be dragged** (default OFF, stored
  per user). ON: drag it; on release it snaps to the nearest left/right edge, stays inside safe margins (header, phone
  bottom navigation, notches) and the position is remembered on that device (`localStorage`). A press only becomes a drag
  after 6 px of movement, and a drag never opens the sale screen. Re-placed on resize/rotation.
* Another shortcut = another `<FloatingQuickAction id icon label onAction draggable />`.
