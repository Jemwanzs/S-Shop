/**
 * Offline POS: a per-branch snapshot of sellable products and a queue of sales made without a connection.
 * Stored in IndexedDB on this device, keyed by business, branch and user. Queued sales carry the `client_ref`
 * the server de-duplicates on, so a sale that reaches the server twice is recorded once.
 */
import { useSyncExternalStore } from "react";
import { api, ApiError } from "@/lib/api";

const DB = "sshop-offline";
const SALES = "sales";
const SNAPSHOTS = "snapshots";

export interface QueuedSale {
  client_ref: string;
  tenant_id: string;
  branch_id: string;
  user_id: string;
  /** When the sale was made on the device. */
  sold_at: string;
  /** The POST /sales body (with offline_at). */
  body: Record<string, unknown>;
  /** X-Location at the moment of sale (geofencing). */
  location: string | null;
  total: number;
  items: number;
  status: "pending" | "failed";
  error?: string;
  error_title?: string;
}

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => {
      req.result.createObjectStore(SALES, { keyPath: "client_ref" });
      req.result.createObjectStore(SNAPSHOTS);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function tx<T>(store: string, mode: IDBTransactionMode, run: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await open();
  return new Promise((resolve, reject) => {
    const t = db.transaction(store, mode);
    const req = run(t.objectStore(store));
    t.oncomplete = () => resolve(req.result);
    t.onerror = () => reject(t.error);
  });
}

// ── Product snapshot (what the till can sell offline) ──
export const saveSnapshot = (key: string, value: unknown) => tx(SNAPSHOTS, "readwrite", (s) => s.put({ at: Date.now(), value }, key)).catch(() => undefined);
export async function loadSnapshot<T>(key: string): Promise<{ at: number; value: T } | undefined> {
  return tx<{ at: number; value: T } | undefined>(SNAPSHOTS, "readonly", (s) => s.get(key)).catch(() => undefined);
}

// ── Sale queue ──
let queue: QueuedSale[] = [];
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
async function refresh() {
  queue = await tx<QueuedSale[]>(SALES, "readonly", (s) => s.getAll()).catch(() => []);
  emit();
}

export async function enqueueSale(sale: QueuedSale) {
  await tx(SALES, "readwrite", (s) => s.put(sale));
  await refresh();
}
export async function discardSale(ref: string) {
  await tx(SALES, "readwrite", (s) => s.delete(ref));
  await refresh();
}

let syncing = false;
/** Sends queued sales for this business/user. Network errors keep them queued; refusals mark them for attention. */
export async function syncSales(tenantId: string, userId: string): Promise<{ sent: number; failed: number }> {
  if (syncing || !navigator.onLine) return { sent: 0, failed: 0 };
  syncing = true;
  let sent = 0;
  let failed = 0;
  try {
    await refresh();
    for (const q of queue.filter((x) => x.tenant_id === tenantId && x.user_id === userId && x.status === "pending")) {
      try {
        await api("/sales", { body: q.body, headers: { "X-Branch-Id": q.branch_id, ...(q.location ? { "X-Location": q.location } : {}) } });
        await tx(SALES, "readwrite", (s) => s.delete(q.client_ref));
        sent++;
      } catch (e) {
        if (e instanceof ApiError && e.code === "network") break; // still offline — try later
        if (e instanceof ApiError && e.status === 401) break; // signed out — keep it for the next sign-in
        const err = e instanceof ApiError ? e : null;
        await tx(SALES, "readwrite", (s) => s.put({ ...q, status: "failed", error: err?.message ?? String(e), error_title: err?.title }));
        failed++;
      }
    }
  } finally {
    syncing = false;
    await refresh();
  }
  return { sent, failed };
}

export async function retrySale(ref: string) {
  const q = queue.find((x) => x.client_ref === ref);
  if (!q) return;
  await tx(SALES, "readwrite", (s) => s.put({ ...q, status: "pending", error: undefined, error_title: undefined }));
  await refresh();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};
/** Queued sales for this business and user (pending and failed). */
export function useOfflineQueue(tenantId?: string, userId?: string) {
  const all = useSyncExternalStore(subscribe, () => queue);
  return all.filter((q) => q.tenant_id === tenantId && q.user_id === userId);
}

// Online/offline status
const onlineListeners = new Set<() => void>();
if (typeof window !== "undefined") {
  window.addEventListener("online", () => onlineListeners.forEach((l) => l()));
  window.addEventListener("offline", () => onlineListeners.forEach((l) => l()));
  refresh();
}
export function useOnline() {
  return useSyncExternalStore(
    (l) => {
      onlineListeners.add(l);
      return () => onlineListeners.delete(l);
    },
    () => navigator.onLine,
  );
}

/** Is this cart sellable offline? Only what needs no live check on the server. */
export function offlineBlocker(o: { method: string; stk: boolean; customerNew: boolean; redeem: number; deposit: boolean; supervisor: boolean; tracked: boolean; clearance: boolean }) {
  if (o.method === "credit") return "Credit sales need a connection";
  if (o.method === "mpesa" && o.stk) return "Push STK needs a connection — record the M-Pesa payment manually";
  if (o.customerNew) return "Adding a new customer needs a connection";
  if (o.redeem > 0) return "Redeeming points needs a connection";
  if (o.deposit) return "Deposits need a connection";
  if (o.supervisor) return "Supervisor approval needs a connection";
  if (o.tracked || o.clearance) return "Items that must be scanned and verified need a connection";
  return null;
}
