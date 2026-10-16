/**
 * Roadmap 128 — S'Shop on the iPhone home screen (Progressive Web App). iPhones never offer an install button, so on
 * Safari (not yet installed) a small tip says how: Share → Add to Home Screen. Dismissed once, per device.
 * `useHomeScreenIdentity` lets a page (a business's ordering link) install as itself: its own name, icon and start
 * page; leaving the page puts S'Shop's back.
 */
import { useEffect, useState } from "react";
import { Share, X } from "lucide-react";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const isIos = () =>
  typeof navigator !== "undefined" &&
  (/iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1));
const isStandalone = () =>
  typeof window !== "undefined" &&
  ((navigator as Navigator & { standalone?: boolean }).standalone === true || window.matchMedia?.("(display-mode: standalone)").matches);

export function InstallHint({ name = "S'Shop", storageKey = "sshop.install-hint", className }: { name?: string; storageKey?: string; className?: string }) {
  const [show, setShow] = useState(() => {
    if (!isIos() || isStandalone()) return false;
    try {
      return localStorage.getItem(storageKey) !== "dismissed";
    } catch {
      return true;
    }
  });
  if (!show) return null;
  const dismiss = () => {
    try {
      localStorage.setItem(storageKey, "dismissed");
    } catch {
      /* shown again next time */
    }
    setShow(false);
  };
  return (
    <div role="note" className={cn("flex items-start gap-2 rounded-xl border bg-card p-3 text-xs text-muted-foreground", className)}>
      <Share className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
      <p className="flex-1">
        <span className="font-medium text-foreground">{t("Install")} {name}:</span> {t("tap Share, then Add to Home Screen.")}
      </p>
      <button type="button" onClick={dismiss} className="rounded p-0.5 hover:bg-muted" aria-label={t("Dismiss")}>
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

/** While mounted, "Add to Home Screen" installs this page as `name`, with `icon` and its own manifest. */
export function useHomeScreenIdentity(identity: { manifest: string; name: string; icon?: string | null } | null) {
  const manifest = identity?.manifest, name = identity?.name, icon = identity?.icon;
  useEffect(() => {
    if (!manifest || !name) return;
    const set = (selector: string, attr: string, value: string) => {
      const el = document.querySelector<HTMLElement>(selector);
      if (!el) return () => {};
      const before = el.getAttribute(attr);
      el.setAttribute(attr, value);
      return () => (before === null ? el.removeAttribute(attr) : el.setAttribute(attr, before));
    };
    const undo = [
      set('link[rel="manifest"]', "href", manifest),
      set('meta[name="apple-mobile-web-app-title"]', "content", name),
      ...(icon ? [set('link[rel="apple-touch-icon"]', "href", icon)] : []),
    ];
    return () => undo.forEach((u) => u());
  }, [manifest, name, icon]);
}
