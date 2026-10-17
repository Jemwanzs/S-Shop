/** Face ID & biometric sign-in (roadmap 132) with passkeys (WebAuthn). The device checks the face, fingerprint or screen
 * lock itself; S'Shop only receives a signed one-time challenge — never a camera image or biometric data. The browser
 * keeps only which passkey this device set up (a public id) plus who it belongs to, so the sign-in screen can offer it. */
import { api } from "@/lib/api";
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

/** What the person will recognise on this device. */
export function biometricLabel(): string {
  const ua = navigator.userAgent;
  if (/iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1)) return "Face ID / Touch ID";
  if (/Macintosh/.test(ua)) return "Touch ID";
  if (/Windows/.test(ua)) return "Windows Hello";
  if (/Android/.test(ua)) return "fingerprint or face unlock";
  return "Face ID / Biometrics";
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

/** Thrown when the person closes or cancels the device's check (not an error worth alarming them about). */
export class PasskeyCancelled extends Error {}

function rethrow(e: unknown): never {
  if (e instanceof DOMException && (e.name === "NotAllowedError" || e.name === "AbortError")) throw new PasskeyCancelled(e.message);
  if (e instanceof DOMException && e.name === "InvalidStateError") throw new Error("This device is already set up");
  throw e;
}

/** Sets up this device: full PIN re-entered on the server, then the device creates the passkey. */
export async function registerPasskey(currentPin: string, deviceName: string): Promise<{ id: string; credential_id: string }> {
  const start = await api<{ challenge_id: string; options: { publicKey: Record<string, unknown> & { challenge: string; user: { id: string; name: string; displayName: string }; excludeCredentials?: Desc[] } } }>(
    "/auth/passkeys/register/start",
    { body: { current_pin: currentPin } },
  );
  const pk = start.options.publicKey;
  let cred: PublicKeyCredential;
  try {
    cred = (await navigator.credentials.create({
      publicKey: {
        ...(pk as unknown as PublicKeyCredentialCreationOptions),
        challenge: fromB64(pk.challenge),
        user: { ...pk.user, id: fromB64(pk.user.id) },
        excludeCredentials: descs(pk.excludeCredentials),
      },
    })) as PublicKeyCredential;
  } catch (e) {
    rethrow(e);
  }
  const r = cred.response as AuthenticatorAttestationResponse;
  return api("/auth/passkeys/register/finish", {
    body: {
      challenge_id: start.challenge_id,
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

/** Signs in with this device's passkey (no email typed; the device does the face / fingerprint / screen-lock check). */
export async function signInWithPasskey(credentialId: string): Promise<{ token: string; profile: Profile }> {
  const start = await api<{ challenge_id: string; options: { publicKey: Record<string, unknown> & { challenge: string; allowCredentials?: Desc[] } } }>(
    "/auth/passkey/login/start",
    { body: { credential_id: credentialId } },
  );
  const pk = start.options.publicKey;
  let cred: PublicKeyCredential;
  try {
    cred = (await navigator.credentials.get({
      publicKey: { ...(pk as unknown as PublicKeyCredentialRequestOptions), challenge: fromB64(pk.challenge), allowCredentials: descs(pk.allowCredentials) },
    })) as PublicKeyCredential;
  } catch (e) {
    rethrow(e);
  }
  const r = cred.response as AuthenticatorAssertionResponse;
  return api("/auth/passkey/login/finish", {
    body: {
      challenge_id: start.challenge_id,
      credential: {
        id: cred.id,
        rawId: toB64(cred.rawId),
        type: cred.type,
        response: { authenticatorData: toB64(r.authenticatorData), clientDataJSON: toB64(r.clientDataJSON), signature: toB64(r.signature), userHandle: toB64(r.userHandle) },
        extensions: cred.getClientExtensionResults(),
      },
    },
  });
}
