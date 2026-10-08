/** Thin fetch wrapper for the S'Shop API. */
import { locationHeader } from "./location";

const TOKEN_KEY = "sshop.token";
const BRANCH_KEY = "sshop.branch";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function write(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    /* storage unavailable (private mode) — session lasts for this tab only */
  }
}

let token = read(TOKEN_KEY);
let branchId = read(BRANCH_KEY);

export const session = {
  get token() {
    return token;
  },
  setToken(t: string | null) {
    token = t;
    write(TOKEN_KEY, t);
  },
  get branchId() {
    return branchId;
  },
  setBranch(id: string | null) {
    branchId = id;
    write(BRANCH_KEY, id);
  },
};

export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
    public code: string,
    /** Short heading for the error popup ("Barcode mismatch"); `message` is then the explanation. */
    public title?: string,
  ) {
    super(message);
  }
}

export type Query = Record<string, string | number | boolean | null | undefined>;

export function qs(query?: Query): string {
  if (!query) return "";
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) {
    if (v !== undefined && v !== null && v !== "") p.set(k, String(v));
  }
  const s = p.toString();
  return s ? `?${s}` : "";
}

interface Options {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  query?: Query;
  /** Use a different bearer token (ordering portal) — `null` sends none. */
  token?: string | null;
  /** Extra headers that override the defaults (offline sync sends the sale's own branch and location). */
  headers?: Record<string, string>;
}

export async function api<T = unknown>(path: string, opts: Options = {}): Promise<T> {
  const headers: Record<string, string> = {};
  const bearer = opts.token === undefined ? token : opts.token;
  if (bearer) headers.Authorization = `Bearer ${bearer}`;
  // Auth calls never carry a branch: a stale stored branch must not block sign-in or /auth/me.
  if (opts.token === undefined && branchId && !path.startsWith("/auth/")) headers["X-Branch-Id"] = branchId;
  // Geofencing: the device position goes with staff requests while the business requires it.
  const where = opts.token === undefined ? locationHeader() : null;
  if (where) headers["X-Location"] = where;
  Object.assign(headers, opts.headers);
  let body: BodyInit | undefined;
  if (opts.body instanceof FormData) body = opts.body;
  else if (opts.body !== undefined) {
    headers["Content-Type"] = "application/json";
    body = JSON.stringify(opts.body);
  }

  let res: Response;
  try {
    res = await fetch(`/api${path}${qs(opts.query)}`, { method: opts.method ?? (body ? "POST" : "GET"), headers, body });
  } catch {
    throw new ApiError("You appear to be offline. Check your connection and try again.", 0, "network");
  }

  if (res.status === 401 && opts.token === undefined) {
    session.setToken(null);
    window.dispatchEvent(new Event("sshop:unauthorized"));
  }
  const text = await res.text();
  const data = text ? safeJson(text) : null;
  if (!res.ok) {
    const err = (data as { error?: { message?: string; code?: string; title?: string | null } } | null)?.error;
    throw new ApiError(err?.message ?? `Request failed (${res.status})`, res.status, err?.code ?? "error", err?.title ?? undefined);
  }
  return data as T;
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/** Download a file endpoint (e.g. Excel export) with auth headers. */
export async function download(path: string, query: Query, filename: string) {
  const headers: Record<string, string> = {};
  if (token) headers.Authorization = `Bearer ${token}`;
  if (branchId) headers["X-Branch-Id"] = branchId;
  const res = await fetch(`/api${path}${qs(query)}`, { headers });
  if (!res.ok) {
    const data = safeJson(await res.text()) as { error?: { message?: string } };
    throw new ApiError(data?.error?.message ?? "Download failed", res.status, "download");
  }
  saveBlob(await res.blob(), filename);
}

export function saveBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export const photoUrl = (id?: string | null) => (id ? `/api/photos/${id}` : undefined);

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : "Something went wrong";
}
