/**
 * Roadmap 118 — a new S'Shop release while the app is open (phones keep it open for days). The build's version is
 * compared with /version.json when the app comes back to the foreground, comes online, and every 10 minutes. When a
 * new one is out, the app offers Refresh, and refreshes by itself on the next change of screen when nothing is
 * unsaved (carts and offline sales are kept on the device, so nothing is lost).
 */
import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { hasUnsaved } from "./unsaved";

declare const __APP_VERSION__: string;
export const APP_VERSION = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev";
const EVERY = 10 * 60_000;

async function published(): Promise<string | null> {
  try {
    const r = await fetch(`/version.json?t=${Date.now()}`, { cache: "no-store" });
    if (!r.ok || !(r.headers.get("content-type") ?? "").includes("json")) return null;
    return ((await r.json()) as { version?: string }).version ?? null;
  } catch {
    return null; // offline: keep working on this version
  }
}

export function useAppUpdate() {
  const [ready, setReady] = useState(false);
  const { pathname } = useLocation();
  const shownOn = useRef<string | null>(null);

  useEffect(() => {
    if (ready) return;
    let live = true;
    const check = async () => {
      const v = await published();
      if (live && v && v !== APP_VERSION) {
        setReady(true);
        // The service worker fetches its newest version as well.
        navigator.serviceWorker?.getRegistration().then((r) => r?.update()).catch(() => undefined);
      }
    };
    const onVisible = () => {
      if (document.visibilityState === "visible") void check();
    };
    void check();
    const timer = window.setInterval(() => void check(), EVERY);
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("online", check);
    return () => {
      live = false;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("online", check);
    };
  }, [ready]);

  // Applied quietly on the next change of screen, never over unsaved work.
  useEffect(() => {
    if (!ready) return;
    if (shownOn.current === null) {
      shownOn.current = pathname;
      return;
    }
    if (pathname !== shownOn.current && !hasUnsaved()) window.location.reload();
  }, [ready, pathname]);

  return { ready, apply: () => window.location.reload() };
}
