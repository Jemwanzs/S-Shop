/**
 * The S'Shop loader: the official S'Shop mark, still, inside a rotating brand-gradient ring.
 * One component for every loading state in the app (buttons, cards, dialogs, pages).
 *
 * - `sm` buttons and compact spots (shown at once, no text) · `md` cards, tables, dialogs · `lg` full page.
 * - md / lg fade in after a short delay, so quick loads never flash a loader.
 * - Reduced motion: the ring stays still.
 * - Announced to assistive technology as a status; the visible label is optional.
 */
import { useId } from "react";
import mark from "@/assets/sshop-mark.png";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const SIZES = {
  sm: { ring: 20, stroke: 2.5, mark: 12 },
  md: { ring: 48, stroke: 3.5, mark: 26 },
  lg: { ring: 80, stroke: 4.5, mark: 44 },
} as const;

export type LoaderSize = keyof typeof SIZES;

export function SShopLoader({ size = "md", label, showLabel, className }: {
  size?: LoaderSize;
  /** What is loading ("Loading…" by default); read by screen readers, shown when `showLabel`. */
  label?: string;
  showLabel?: boolean;
  className?: string;
}) {
  const id = useId();
  const s = SIZES[size];
  const r = (s.ring - s.stroke) / 2;
  const c = 2 * Math.PI * r;
  const text = t(label ?? "Loading…");
  return (
    <span
      role="status"
      aria-live="polite"
      className={cn(
        "inline-flex shrink-0 flex-col items-center justify-center gap-2.5 align-middle",
        size !== "sm" && "sshop-loader-in",
        className,
      )}
    >
      <span className="relative inline-flex shrink-0 items-center justify-center" style={{ width: s.ring, height: s.ring }}>
        <svg
          viewBox={`0 0 ${s.ring} ${s.ring}`}
          className="absolute inset-0 animate-spin motion-reduce:animate-none"
          style={{ width: s.ring, height: s.ring, animationDuration: "1.1s" }}
          aria-hidden
        >
          <defs>
            <linearGradient id={`${id}-g`} x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="hsl(var(--brand-from))" />
              <stop offset="100%" stopColor="hsl(var(--brand-to))" />
            </linearGradient>
          </defs>
          <circle cx={s.ring / 2} cy={s.ring / 2} r={r} fill="none" stroke="hsl(var(--muted))" strokeWidth={s.stroke} />
          <circle
            cx={s.ring / 2}
            cy={s.ring / 2}
            r={r}
            fill="none"
            stroke={`url(#${id}-g)`}
            strokeWidth={s.stroke}
            strokeLinecap="round"
            strokeDasharray={`${c * 0.32} ${c}`}
          />
        </svg>
        <img src={mark} alt="" draggable={false} className="relative select-none object-contain" style={{ width: s.mark, height: s.mark }} />
      </span>
      {showLabel ? <span className="text-sm text-muted-foreground">{text}</span> : <span className="sr-only">{text}</span>}
    </span>
  );
}
