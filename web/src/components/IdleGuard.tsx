/** Automatic logout after inactivity (roadmap 87).
 *
 * - **Activity** is what the person does: taps, clicks, keys, scrolling, typing (throttled). Background requests,
 *   polling, notifications and the live stream never count. Activity reaches the server as a heartbeat (and changes
 *   the person makes carry `X-User-Activity`), so the server's own idle check agrees with this timer.
 * - **One clock for every tab**: the last activity is kept per session in local storage; any tab's activity keeps all
 *   tabs signed in, a logout in one tab signs every tab out. Wall-clock based, so a locked phone or a backgrounded
 *   tab is judged correctly the moment it comes back.
 * - **Warning** a configurable number of seconds before logout: *Stay logged in* (renews the session) or *Log out now*.
 *   Unsaved changes are mentioned; nothing is submitted. While it shows, only its buttons count. */
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "@/lib/router";
import { TimerReset } from "lucide-react";
import { api, session, SESSION_EXPIRED } from "@/lib/api";
import { LOGOUT_SIGNAL as OUT, useSession } from "@/lib/session";
import { rememberExpiredBusiness } from "@/lib/brand";
import { hasUnsaved } from "@/lib/unsaved";
import { t } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

const key = (token: string) => `sshop.last-activity.${token.slice(-24)}`;
const read = (k: string) => {
  try {
    return Number(localStorage.getItem(k)) || 0;
  } catch {
    return 0;
  }
};
const write = (k: string, v: number) => {
  try {
    localStorage.setItem(k, String(v));
  } catch {
    /* private mode: this tab keeps its own clock */
  }
};

export function IdleGuard() {
  const { profile, signOut, signOutIdle } = useSession();
  const slug = profile?.tenant.slug;
  const navigate = useNavigate();
  const token = session.token;
  // The heartbeat returns the current limits: a policy changed while the app is open applies here at once.
  const [live, setLive] = useState<{ idle_seconds: number; warning_seconds: number } | null>(null);
  useEffect(() => setLive(null), [token]); // a new session (another business) starts from its own profile
  const limits = live ?? profile?.session;
  const idle = (limits?.idle_seconds ?? 300) * 1000;
  const warning = (limits?.warning_seconds ?? 30) * 1000;
  const [left, setLeft] = useState<number | null>(null);
  const lastLocal = useRef(Date.now());
  const lastBeat = useRef(0);
  const ended = useRef(false);
  // While the warning shows, only its buttons decide: a stray tap must not dismiss it (or swallow "Log out now").
  const warned = useRef(false);
  warned.current = left !== null;

  const finish = useCallback((reason: "inactivity" | "manual") => {
    if (ended.current) return;
    ended.current = true;
    if (token) {
      try { localStorage.removeItem(key(token)); } catch { /* nothing kept */ }
    }
    if (reason === "inactivity") {
      try { sessionStorage.setItem(SESSION_EXPIRED, "1"); } catch { /* message not shown */ }
      rememberExpiredBusiness(slug);
    }
    // Ends the session on the server (which also ends idle sessions on its own) and clears this device's data.
    if (reason === "inactivity") signOutIdle();
    else signOut();
    navigate("/login", { replace: true });
  }, [token, slug, signOut, signOutIdle, navigate]);

  const beat = useCallback(() => {
    lastBeat.current = Date.now();
    api<{ idle_seconds: number; warning_seconds: number }>("/auth/activity", { body: {} })
      .then((l) => setLive((cur) => (cur && cur.idle_seconds === l.idle_seconds && cur.warning_seconds === l.warning_seconds ? cur : l)))
      .catch(() => undefined);
  }, []);

  const active = useCallback((force = false) => {
    if (!token || ended.current) return;
    const now = Date.now();
    if (!force && now - lastLocal.current < 1000) return;
    lastLocal.current = now;
    write(key(token), now);
    setLeft(null);
    // Heartbeat while active, often enough for the shortest limits.
    if (force || now - lastBeat.current > Math.min(20_000, idle / 4)) beat();
  }, [token, idle, beat]);

  useEffect(() => {
    if (!token) return;
    ended.current = false;
    if (!read(key(token))) write(key(token), Date.now());
    // Clocks of earlier sessions on this device are no longer needed.
    try {
      Object.keys(localStorage).filter((k) => k.startsWith("sshop.last-activity.") && k !== key(token)).forEach((k) => localStorage.removeItem(k));
    } catch { /* storage unavailable */ }
    const on = () => { if (!warned.current) active(); };
    const events = ["pointerdown", "keydown", "wheel", "touchstart", "input"] as const;
    events.forEach((e) => window.addEventListener(e, on, { passive: true, capture: true }));
    window.addEventListener("scroll", on, { passive: true, capture: true });
    const storage = (e: StorageEvent) => {
      // Another tab logged out: follow without a second logout call (that session is already ended).
      if (e.key === OUT && e.newValue && !ended.current) {
        ended.current = true;
        if (e.newValue.endsWith(":inactivity")) {
          try { sessionStorage.setItem(SESSION_EXPIRED, "1"); } catch { /* message not shown */ }
          rememberExpiredBusiness(slug);
        }
        signOut();
        navigate("/login", { replace: true });
      }
      if (e.key === key(token)) setLeft(null);
    };
    window.addEventListener("storage", storage);
    const tick = window.setInterval(() => {
      const last = read(key(token)) || lastLocal.current;
      const remaining = last + idle - Date.now();
      if (remaining <= 0) finish("inactivity");
      else if (remaining <= warning) setLeft(Math.ceil(remaining / 1000));
      else setLeft(null);
    }, 1000);
    return () => {
      events.forEach((e) => window.removeEventListener(e, on, { capture: true }));
      window.removeEventListener("scroll", on, { capture: true });
      window.removeEventListener("storage", storage);
      window.clearInterval(tick);
    };
  }, [token, slug, idle, warning, active, finish, signOut, navigate]);

  if (left === null) return null;
  return (
    <ResponsiveDialog open onOpenChange={(o) => { if (!o) active(true); }} title={t("Your Session Is About to Expire")}
      footer={<div className="flex w-full flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <Button variant="outline" onClick={() => finish("manual")}>{t("Log Out Now")}</Button>
        <Button onClick={() => active(true)}>{t("Stay Logged In")}</Button>
      </div>}>
      <div className="flex gap-3 text-sm">
        <TimerReset className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
        <div className="space-y-2">
          <p>{t("You have been inactive. For your security, S'Shop will log you out in")} <b className="num">{left}</b> {t("seconds.")}</p>
          {hasUnsaved() && <p className="rounded-lg bg-warning/10 p-2 text-xs text-warning">{t("You have unsaved changes on this screen. They are not saved automatically.")}</p>}
        </div>
      </div>
    </ResponsiveDialog>
  );
}
