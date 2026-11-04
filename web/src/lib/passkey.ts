/** Face ID & biometric sign-in (roadmap 132) with passkeys (WebAuthn). The device checks the face, fingerprint or screen
 * lock itself; S'Shop only receives a signed one-time challenge — never a camera image or biometric data. The browser
 * keeps only which passkey this device set up (a public id) plus who it belongs to, so the sign-in screen can offer it. */
import { api, ApiError } from "@/lib/api";
import type { Profile } from "@/lib/types";

export interface PasskeyDevice {
  credential_id: string;
  email: string;
  name: string;
  business: string;
  slug?: string;
}

const KEY = "sshop.passkey";

export function passkeyDevice(): PasskeyDevice | null {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null");
    return v && typeof v.credential_id === "string" ? v : null;
  } catch {
    return null;
  }
}

export function rememberPasskey(d: PasskeyDevice) {
  try {
    localStorage.setItem(KEY, JSON.stringify(d));
  } catch {
    /* private mode: biometric sign-in simply is not offered next time */
  }
}

export function forgetPasskey() {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* nothing kept */
  }
}

/** This browser can create and use passkeys at all. */
export const passkeysSupported = () => typeof window !== "undefined" && !!window.PublicKeyCredential && !!navigator.credentials?.create;

/** A built-in Face ID / fingerprint / screen-lock check is available (else a phone or security key may still work). */
export async function platformAuthenticator(): Promise<boolean> {
  try {
    return passkeysSupported() && (await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable());
  } catch {
    return false;
  }
}

export type DevicePlatform = "iphone" | "ipad" | "android" | "windows" | "mac" | "other";

/** Which device family this is, for wording only (never a promise of a method): the browser cannot say which check
 * the device will use, or let S'Shop choose it. */
export function devicePlatform(): DevicePlatform {
  const ua = navigator.userAgent;
  if (/iPhone|iPod/.test(ua)) return "iphone";
  if (/iPad/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1)) return "ipad";
  if (/Android/.test(ua)) return "android";
  if (/Windows/.test(ua)) return "windows";
  if (/Macintosh/.test(ua)) return "mac";
  return "other";
}

/** What this device calls it: the switch, its explanation, the button that starts the device's check, and sign-in. */
export function passkeyWording(platform = devicePlatform()) {
  switch (platform) {
    case "iphone":
      return { toggle: "Enable Face ID / Passkey Login", hint: "Use your iPhone's supported device authentication.", action: "Use Face ID / Passkey", signIn: "Sign in with Face ID / Passkey" };
    case "ipad":
      return { toggle: "Enable Face ID / Passkey Login", hint: "Use your iPad's supported device authentication.", action: "Use Face ID / Passkey", signIn: "Sign in with Face ID / Passkey" };
    case "android":
      return { toggle: "Enable Biometric / Passkey Login", hint: "Use fingerprint, supported face authentication or device screen lock.", action: "Use Biometric / Passkey", signIn: "Sign in with Biometric / Passkey" };
    case "windows":
      return { toggle: "Enable Windows Hello / Passkey Login", hint: "Use Windows Hello or another supported authenticator.", action: "Use Windows Hello / Passkey", signIn: "Sign in with Windows Hello / Passkey" };
    case "mac":
      return { toggle: "Enable Touch ID / Passkey Login", hint: "Use Touch ID, your Mac's password or another supported authenticator.", action: "Use Touch ID / Passkey", signIn: "Sign in with Touch ID / Passkey" };
    default:
      return { toggle: "Enable Passkey Login", hint: "Use your device's screen lock or another supported authenticator.", action: "Use a Passkey", signIn: "Sign in with a Passkey" };
  }
}

/** Passkeys work only on S'Shop's own address. */
export const onPasskeySite = (rpId: string | null | undefined) => !rpId || location.hostname === rpId || location.hostname.endsWith(`.${rpId}`);

/** A passkey the server no longer accepts should leave the device's own list too, where the browser allows it. */
export function signalRemoved(rpId: string | null | undefined, credentialId: string | undefined) {
  const P = window.PublicKeyCredential as unknown as { signalUnknownCredential?: (o: { rpId: string; credentialId: string }) => Promise<void> } | undefined;
  if (rpId && credentialId) void P?.signalUnknownCredential?.({ rpId, credentialId }).catch(() => undefined);
}

/** After a removal: the passkeys of this person that still count (the device may drop the others). */
export function signalAccepted(rpId: string | null | undefined, userId: string, allAcceptedCredentialIds: string[]) {
  const P = window.PublicKeyCredential as unknown as { signalAllAcceptedCredentials?: (o: { rpId: string; userId: string; allAcceptedCredentialIds: string[] }) => Promise<void> } | undefined;
  if (rpId) void P?.signalAllAcceptedCredentials?.({ rpId, userId, allAcceptedCredentialIds }).catch(() => undefined);
}

// ── JSON ⇄ WebAuthn buffers (base64url, as webauthn-rs sends and expects) ──────────────────────────────────────────────

const fromB64 = (s: string): ArrayBuffer => {
  const b = atob(s.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(s.length / 4) * 4, "="));
  const out = new Uint8Array(b.length);
  for (let i = 0; i < b.length; i++) out[i] = b.charCodeAt(i);
  return out.buffer;
};
const toB64 = (buf: ArrayBuffer | null | undefined): string | undefined => {
  if (!buf) return undefined;
  let s = "";
  for (const c of new Uint8Array(buf)) s += String.fromCharCode(c);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};

type Desc = { id: string; type: PublicKeyCredentialType; transports?: AuthenticatorTransport[] };
const descs = (l?: Desc[]) => l?.map((c) => ({ ...c, id: fromB64(c.id) }));

/** The person closed or cancelled the device's check, or it timed out (the browser does not say which, on purpose). */
export class PasskeyCancelled extends Error {}
/** The device refused for a stated reason; `message` is what to tell the person. */
export class PasskeyProblem extends Error {
  constructor(public kind: "exists" | "site" | "unsupported" | "failed", message: string) {
    super(message);
  }
}

function rethrow(e: unknown): never {
  const name = e instanceof DOMException ? e.name : "";
  if (name === "NotAllowedError" || name === "AbortError") throw new PasskeyCancelled(name);
  if (name === "InvalidStateError") throw new PasskeyProblem("exists", "This device already has a passkey for your account — sign out and use it to sign in, nothing more to set up.");
  if (name === "SecurityError") throw new PasskeyProblem("site", "Passkeys work only on S'Shop's own address.");
  if (name === "NotSupportedError") throw new PasskeyProblem("unsupported", "This device or browser cannot create a passkey.");
  throw new PasskeyProblem("failed", "The device could not complete the check — try again, or use your PIN.");
}

// Roadmap 155: iPhones show the Face ID prompt only straight after a tap, and refuse (with the same error as a cancel) when
// the page first waited on the network. So each ceremony has two parts: the server part runs before the tap, and the device
// part is called directly from the tap, with nothing awaited in between.

export interface PreparedRegistration { challenge_id: string; publicKey: PublicKeyCredentialCreationOptions }

/** Before the tap: the full PIN is checked on the server and the challenge issued. */
export async function prepareRegistration(currentPin: string): Promise<PreparedRegistration> {
  const start = await api<{ challenge_id: string; options: { publicKey: Record<string, unknown> & { challenge: string; user: { id: string; name: string; displayName: string }; excludeCredentials?: Desc[]; authenticatorSelection?: AuthenticatorSelectionCriteria } } }>(
    "/auth/passkeys/register/start",
    { body: { current_pin: currentPin } },
  );
  const pk = start.options.publicKey;
  return {
    challenge_id: start.challenge_id,
    publicKey: {
      ...(pk as unknown as PublicKeyCredentialCreationOptions),
      challenge: fromB64(pk.challenge),
      user: { ...pk.user, id: fromB64(pk.user.id) },
      excludeCredentials: descs(pk.excludeCredentials),
      // A passkey the device can offer by itself, so sign-in needs nothing remembered by the browser. Preferred, not
      // required: an older security key that cannot store one still works from the device that set it up.
      authenticatorSelection: { ...pk.authenticatorSelection, residentKey: "preferred", requireResidentKey: false },
    },
  };
}

/** Call directly from a tap: the device checks the person and creates the passkey; the server verifies the answer. */
export async function finishRegistration(prepared: PreparedRegistration, deviceName: string): Promise<{ id: string; credential_id: string }> {
  let cred: PublicKeyCredential;
  try {
    cred = (await navigator.credentials.create({ publicKey: prepared.publicKey })) as PublicKeyCredential;
  } catch (e) {
    rethrow(e);
  }
  const r = cred.response as AuthenticatorAttestationResponse;
  return api("/auth/passkeys/register/finish", {
    body: {
      challenge_id: prepared.challenge_id,
      device_name: deviceName,
      credential: {
        id: cred.id,
        rawId: toB64(cred.rawId),
        type: cred.type,
        response: { attestationObject: toB64(r.attestationObject), clientDataJSON: toB64(r.clientDataJSON), transports: r.getTransports?.() },
        extensions: cred.getClientExtensionResults(),
      },
    },
  });
}

export interface PreparedSignIn { challenge_id: string; publicKey: PublicKeyCredentialRequestOptions; at: number }

/** A challenge lasts 5 minutes on the server; a new one is fetched before that. */
export const SIGN_IN_FRESH_MS = 4 * 60 * 1000;

/** Before the tap: the challenge, for the passkey this browser remembers or (none) any the device offers. */
export async function prepareSignIn(credentialId?: string): Promise<PreparedSignIn> {
  const start = await api<{ challenge_id: string; options: { publicKey: Record<string, unknown> & { challenge: string; allowCredentials?: Desc[] } } }>(
    "/auth/passkey/login/start",
    { body: { credential_id: credentialId ?? "" } },
  );
  const pk = start.options.publicKey;
  return {
    challenge_id: start.challenge_id,
    at: Date.now(),
    publicKey: { ...(pk as unknown as PublicKeyCredentialRequestOptions), challenge: fromB64(pk.challenge), allowCredentials: descs(pk.allowCredentials) },
  };
}

/** Call directly from a tap: the device checks the person and signs the challenge; the server verifies the answer. */
export async function finishSignIn(prepared: PreparedSignIn): Promise<{ token: string; profile: Profile; credential_id: string }> {
  let cred: PublicKeyCredential;
  try {
    cred = (await navigator.credentials.get({ publicKey: prepared.publicKey })) as PublicKeyCredential;
  } catch (e) {
    rethrow(e);
  }
  const r = cred.response as AuthenticatorAssertionResponse;
  try {
    return await api("/auth/passkey/login/finish", {
      body: {
        challenge_id: prepared.challenge_id,
        credential: {
          id: cred.id,
          rawId: toB64(cred.rawId),
          type: cred.type,
          response: { authenticatorData: toB64(r.authenticatorData), clientDataJSON: toB64(r.clientDataJSON), signature: toB64(r.signature), userHandle: toB64(r.userHandle) },
          extensions: cred.getClientExtensionResults(),
        },
      },
    });
  } catch (e) {
    if (e instanceof ApiError && e.title === "Passkey not recognised") signalRemoved(prepared.publicKey.rpId, cred.id);
    throw e;
  }
}
