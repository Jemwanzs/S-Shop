/**
 * Floating quick action (roadmap 48): a small bubble on the screen edge that opens an existing screen.
 * Reusable for other shortcuts: give it an id (its remembered position), an icon, a label and where it goes.
 *
 * - Fixed at the centre of the right edge by default.
 * - With `draggable`, the user can move it; on release it snaps to the nearest left/right edge, the vertical position is
 *   kept inside safe margins (header, phone bottom navigation, notches) and remembered on this device.
 * - A tap or click always runs the action; a drag (pointer moved more than a few pixels) only repositions it — it never
 *   triggers the action. Keyboard: Enter / Space run it.
 * - Re-placed safely when the window is resized or the device rotates.
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const SIZE = 48; // px — small but comfortably tappable
const EDGE = 10; // gap to the screen edge
const DRAG_THRESHOLD = 6; // px of movement before a press becomes a drag

interface Placement {
  side: "left" | "right";
  /** Vertical centre as a fraction of the window height (survives rotation and resizing). */
  y: number;
}

const DEFAULT: Placement = { side: "right", y: 0.5 };

function load(id: string): Placement {
  try {
    const v = JSON.parse(localStorage.getItem(`sshop.fab.${id}`) ?? "null");
    if (v && (v.side === "left" || v.side === "right") && typeof v.y === "number") return { side: v.side, y: Math.min(Math.max(v.y, 0), 1) };
  } catch {
    /* private mode / blocked storage: default position */
  }
  return DEFAULT;
}

function save(id: string, p: Placement) {
  try {
    localStorage.setItem(`sshop.fab.${id}`, JSON.stringify(p));
  } catch {
    /* not remembered on this device */
  }
}

/** Top/bottom limits that keep the bubble off the header and the phone bottom navigation. */
function bounds() {
  const h = window.innerHeight;
  const phone = window.innerWidth < 1024;
  const top = (phone ? 64 : 80) + EDGE;
  const bottom = h - (phone ? 72 : 24) - SIZE - EDGE; // bottom navigation on phones and tablets
  return { top, bottom: Math.max(bottom, top) };
}

function topFor(y: number) {
  const { top, bottom } = bounds();
  return Math.min(Math.max(y * window.innerHeight - SIZE / 2, top), bottom);
}

export function FloatingQuickAction({
  id,
  icon,
  label,
  onAction,
  draggable = false,
  className,
}: {
  id: string;
  icon: ReactNode;
  label: string;
  onAction: () => void;
  draggable?: boolean;
  className?: string;
}) {
  const [place, setPlace] = useState<Placement>(() => (draggable ? load(id) : DEFAULT));
  const [, setViewport] = useState(0);
  const [drag, setDrag] = useState<{ x: number; y: number } | null>(null);
  const press = useRef<{ x: number; y: number; id: number; moved: boolean } | null>(null);

  // Fixed position when dragging is off; the remembered one when it is on.
  useEffect(() => setPlace(draggable ? load(id) : DEFAULT), [draggable, id]);
  // Re-place on resize / rotation (position is stored as a fraction, clamped to the new safe area).
  useEffect(() => {
    const on = () => setViewport((n) => n + 1);
    window.addEventListener("resize", on);
    window.addEventListener("orientationchange", on);
    return () => {
      window.removeEventListener("resize", on);
      window.removeEventListener("orientationchange", on);
    };
  }, []);

  const onPointerDown = (e: React.PointerEvent<HTMLButtonElement>) => {
    if (!draggable || e.button !== 0) return;
    press.current = { x: e.clientX, y: e.clientY, id: e.pointerId, moved: false };
    // Capture now: a quick flick can leave the bubble before the first move event is seen.
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLButtonElement>) => {
    const p = press.current;
    if (!p || p.id !== e.pointerId) return;
    if (!p.moved && Math.hypot(e.clientX - p.x, e.clientY - p.y) < DRAG_THRESHOLD) return;
    p.moved = true;
    setDrag({ x: e.clientX - SIZE / 2, y: e.clientY - SIZE / 2 });
  };
  const finish = useCallback(
    (e: React.PointerEvent<HTMLButtonElement>) => {
      const p = press.current;
      if (!p || p.id !== e.pointerId) return;
      if (p.moved) {
        // Snap to the nearest edge and remember it on this device.
        const next: Placement = {
          side: e.clientX < window.innerWidth / 2 ? "left" : "right",
          y: Math.min(Math.max(e.clientY / window.innerHeight, 0), 1),
        };
        setPlace(next);
        save(id, next);
      }
      setDrag(null);
      // Keep the "moved" flag until the click event that follows this pointerup has been ignored.
      setTimeout(() => (press.current = null), 0);
    },
    [id],
  );
  const onClick = () => {
    if (press.current?.moved) return; // the press was a drag
    onAction();
  };

  const style: React.CSSProperties = drag
    ? { left: drag.x, top: drag.y, transition: "none" }
    : { top: topFor(place.y), [place.side]: `calc(${EDGE}px + env(safe-area-inset-${place.side}, 0px))` };

  return (
    <button
      type="button"
      aria-label={t(label)}
      title={t(label)}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={finish}
      onPointerCancel={finish}
      onClick={onClick}
      style={style}
      className={cn(
        "group fixed z-30 flex h-11 w-11 items-center justify-center rounded-full text-white opacity-90 shadow-lg ring-1 ring-black/10 transition-[top,left,right,transform,opacity] duration-200 hover:opacity-100 lg:h-12 lg:w-12 lg:opacity-100",
        "bg-[hsl(var(--fab))] hover:scale-105 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring active:scale-95 print:hidden",
        draggable ? "cursor-grab touch-none active:cursor-grabbing" : "cursor-pointer",
        drag && "scale-110 opacity-90",
        className,
      )}
    >
      <span className="[&_svg]:h-5 [&_svg]:w-5">{icon}</span>
      {/* Desktop hover label, on the side away from the edge. */}
      <span
        className={cn(
          "pointer-events-none absolute hidden whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-xs font-medium text-background opacity-0 shadow transition-opacity lg:block",
          "group-hover:opacity-100",
          place.side === "right" ? "right-full mr-2" : "left-full ml-2",
        )}
      >
        {t(label)}
      </span>
    </button>
  );
}
