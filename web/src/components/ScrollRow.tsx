/** One horizontal row of tabs / chips that never spills out of its container (roadmap 79): scrolls smoothly (touch,
 * trackpad, mouse wheel while there is more to see), shows ‹ › only when there is more in that direction, fades the
 * clipped edge, and keeps the active item (data-active="true") in view. */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

export function ScrollRow({ children, active, className, label, bleed = true }: {
  children: ReactNode;
  /** Changes when the selection changes, so the active item is scrolled into view. */
  active?: unknown;
  className?: string;
  label?: string;
  /** Reach the screen edges on phones (the page's own padding), as the chip rows always did. */
  bleed?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ left: false, right: false });
  const measure = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const rtl = getComputedStyle(el).direction === "rtl";
    const max = el.scrollWidth - el.clientWidth;
    const pos = rtl ? -el.scrollLeft : el.scrollLeft;
    const before = pos > 2;
    const after = pos < max - 2;
    setEdges(rtl ? { left: after, right: before } : { left: before, right: after });
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    measure();
    el.addEventListener("scroll", measure, { passive: true });
    const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(measure) : null;
    ro?.observe(el);
    Array.from(el.children).forEach((c) => ro?.observe(c));
    // A vertical mouse wheel scrolls the row only while it can still move that way; the page scrolls otherwise.
    const wheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX) || el.scrollWidth <= el.clientWidth) return;
      const max = el.scrollWidth - el.clientWidth;
      const pos = Math.abs(el.scrollLeft);
      if ((e.deltaY > 0 && pos >= max - 1) || (e.deltaY < 0 && pos <= 0)) return;
      e.preventDefault();
      el.scrollLeft += getComputedStyle(el).direction === "rtl" ? -e.deltaY : e.deltaY;
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => {
      el.removeEventListener("scroll", measure);
      el.removeEventListener("wheel", wheel);
      ro?.disconnect();
    };
  }, [measure, children]);

  // Keep the active item visible (without moving the page vertically).
  useLayoutEffect(() => {
    const el = ref.current;
    const item = el?.querySelector<HTMLElement>('[data-active="true"]');
    if (!el || !item) return;
    const box = el.getBoundingClientRect();
    const r = item.getBoundingClientRect();
    const pad = 32;
    if (r.left < box.left + pad) el.scrollBy({ left: r.left - box.left - pad, behavior: "smooth" });
    else if (r.right > box.right - pad) el.scrollBy({ left: r.right - box.right + pad, behavior: "smooth" });
  }, [active]);

  const step = (dir: 1 | -1) => {
    const el = ref.current;
    if (el) el.scrollBy({ left: dir * Math.max(120, el.clientWidth * 0.7), behavior: "smooth" });
  };
  const arrow = "absolute top-1/2 z-10 hidden h-7 w-7 -translate-y-1/2 items-center justify-center rounded-full border bg-card text-foreground shadow-sm hover:bg-accent md:flex";
  return (
    <div className={cn("relative min-w-0", bleed && "-mx-3.5 md:mx-0", className)}>
      <div ref={ref} role={label ? "group" : undefined} aria-label={label}
        className={cn("scrollbar-none flex gap-1.5 overflow-x-auto scroll-smooth py-0.5", bleed ? "px-3.5 md:px-0" : "px-0.5")}
        style={{
          WebkitMaskImage: edges.left || edges.right
            ? `linear-gradient(to right, ${edges.left ? "transparent 0, #000 28px" : "#000 0"}, ${edges.right ? "#000 calc(100% - 28px), transparent 100%" : "#000 100%"})`
            : undefined,
          maskImage: edges.left || edges.right
            ? `linear-gradient(to right, ${edges.left ? "transparent 0, #000 28px" : "#000 0"}, ${edges.right ? "#000 calc(100% - 28px), transparent 100%" : "#000 100%"})`
            : undefined,
        }}>
        {children}
      </div>
      {/* Physical sides: edges are measured physically, so this is right in RTL too. */}
      {edges.left && <button type="button" className={cn(arrow, "left-0")} aria-label={t("Previous tabs")} onClick={() => step(-1)}><ChevronLeft className="h-4 w-4" /></button>}
      {edges.right && <button type="button" className={cn(arrow, "right-0")} aria-label={t("Next tabs")} onClick={() => step(1)}><ChevronRight className="h-4 w-4" /></button>}
    </div>
  );
}
