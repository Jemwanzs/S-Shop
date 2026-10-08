import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "@/lib/toast";
import { session } from "./api";

/** Which query caches each live topic invalidates. */
const TOPICS: Record<string, string[][]> = {
  notification: [["notifications"], ["support-access"]],
  // Roadmap 71: a business approved, declined or ended support access.
  support: [["platform-accounts"], ["platform-account"], ["notifications"]],
  // The Orders badge (pending new orders) lives in the notifications feed.
  order: [["orders"], ["order"], ["dashboard"], ["notifications"]],
  stock: [["stock"], ["pos-products"], ["products"], ["product"]],
  sale: [["sales"], ["dashboard"], ["customers"]],
  approval: [["approvals"], ["notifications"]],
  mpesa: [["mpesa"]],
};

/** A short, soft two-tone chime (no audio file; silently skipped where audio is blocked). */
function chime() {
  try {
    const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    [880, 1320].forEach((f, i) => {
      const o = ctx.createOscillator();
      const g = ctx.createGain();
      o.frequency.value = f;
      const at = ctx.currentTime + i * 0.14;
      g.gain.setValueAtTime(0.0001, at);
      g.gain.exponentialRampToValueAtTime(0.15, at + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, at + 0.22);
      o.connect(g).connect(ctx.destination);
      o.start(at);
      o.stop(at + 0.24);
    });
    setTimeout(() => ctx.close(), 800);
  } catch {
    /* audio unavailable */
  }
}

export interface AlertPrefs { in_app_alerts?: boolean; sound_alerts?: boolean }

/** Subscribe to the server's SSE stream while signed in; reconnects automatically. Alerts follow the user's
 * preferences (roadmap 69); the lists, bell and badges always refresh. */
export function useLiveEvents(enabled: boolean, prefs: AlertPrefs = {}) {
  const qc = useQueryClient();
  const prefsRef = useRef(prefs);
  prefsRef.current = prefs;
  useEffect(() => {
    if (!enabled || !session.token || typeof EventSource === "undefined") return;
    const es = new EventSource(`/api/events?access_token=${encodeURIComponent(session.token)}`);
    for (const [topic, keys] of Object.entries(TOPICS)) {
      es.addEventListener(topic, (ev) => {
        keys.forEach((queryKey) => qc.invalidateQueries({ queryKey }));
        if (topic === "notification") {
          try {
            const n = JSON.parse((ev as MessageEvent).data);
            if (prefsRef.current.in_app_alerts !== false) toast(n.title, { description: n.body });
            if (prefsRef.current.sound_alerts && n.kind === "new_order") chime();
          } catch {
            /* malformed payload — the list refetch still happens */
          }
        }
      });
    }
    return () => es.close();
  }, [enabled, qc]);
}
