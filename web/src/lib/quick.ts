/** Quick Login PIN on this device (roadmap 83). The browser keeps only the device secret the server issued (never a
 * PIN) plus who it belongs to, so the sign-in screen can greet them. Cleared when the server says the device needs a
 * full sign-in again, or when the person turns Quick PIN off. */
export interface QuickDevice {
  token: string;
  device_id: string;
  email: string;
  name: string;
  business: string;
}

const KEY = "sshop.quick";
const METHOD = "sshop.login-method";

export function quickDevice(): QuickDevice | null {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null");
    return v && typeof v.token === "string" ? v : null;
  } catch {
    return null;
  }
}

export function rememberQuickDevice(d: QuickDevice) {
  try {
    localStorage.setItem(KEY, JSON.stringify(d));
    localStorage.setItem(METHOD, "quick");
  } catch {
    /* private mode: Quick PIN simply is not offered next time */
  }
}

export function forgetQuickDevice() {
  try {
    localStorage.removeItem(KEY);
    localStorage.removeItem(METHOD);
  } catch {
    /* nothing kept */
  }
}

/** The sign-in method this device used last ("quick" | "full"). */
export function preferredMethod(): "quick" | "full" {
  try {
    return localStorage.getItem(METHOD) === "full" ? "full" : "quick";
  } catch {
    return "quick";
  }
}

export function rememberMethod(m: "quick" | "full") {
  try {
    localStorage.setItem(METHOD, m);
  } catch {
    /* not kept */
  }
}
