/** Roadmap 153: the live count of a scanning session, shown beside the scanning field — the product last scanned with its
 * quantity and controls, the other products, and the session total. The same panel inside the camera scanner (dark) and
 * under the inline field (light). Shared barcodes count units; an individual barcode is always exactly one unit. */
import { useEffect, useState } from "react";
import { Check, Minus, Plus, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { count } from "@/lib/format";
import { useKeyTiming } from "@/lib/scan";

export interface ScanCountLine {
  key: string;
  name: string;
  /** As this person may see it (individual barcodes masked where the screen masks them). */
  barcode?: string | null;
  kind: "shared" | "unique";
  quantity: number;
  /** Lowest quantity the controls allow (1 by default; 0 on a stock take, where zero is a real count). */
  min?: number;
  /** Highest quantity the controls allow (stock at the till, quantity entry locked); none = no limit. */
  max?: number | null;
}

export interface ScanCounterProps {
  lines: ScanCountLine[];
  /** The line last scanned: it stays the large one until the next scan. */
  activeKey?: string | null;
  onSet?: (key: string, quantity: number) => void;
  onRemove?: (key: string) => void;
  /** A handheld scanner fired into the quantity field: it is a scan, not a quantity. */
  onScanned?: (code: string) => void;
  /** Controls were used: hand focus back to the scanning field so the next scan lands there. */
  onDone?: () => void;
  tone?: "dark" | "light";
  className?: string;
}

const units = (n: number) => `${count(n)} ${t(n === 1 ? "unit" : "units")}`;

export function ScanCounter({ lines, activeKey, onSet, onRemove, onScanned, onDone, tone = "light", className }: ScanCounterProps) {
  const [picked, setPicked] = useState<string | null>(null);
  useEffect(() => setPicked(null), [activeKey]);
  if (lines.length === 0) return null;
  const line = lines.find((l) => l.key === picked) ?? lines.find((l) => l.key === activeKey) ?? lines[lines.length - 1];
  const others = lines.filter((l) => l.key !== line.key);
  const total = lines.reduce((a, l) => a + l.quantity, 0);
  const dark = tone === "dark";
  const min = line.min ?? 1;
  const atMax = line.max != null && line.quantity >= line.max;
  const set = (q: number) => {
    onSet?.(line.key, Math.max(min, line.max != null ? Math.min(line.max, q) : q));
    onDone?.();
  };
  const stepper = cn("flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border transition-colors disabled:opacity-40", dark ? "border-white/25 hover:bg-white/10" : "border-input hover:bg-accent");
  return (
    <div data-testid="scan-counter" className={cn("rounded-xl p-3 text-sm", dark ? "bg-white/10 text-white" : "border bg-card", className)}>
      <div className="flex items-start gap-2">
        <Check className={cn("mt-0.5 h-4 w-4 shrink-0", dark ? "text-emerald-300" : "text-success")} />
        <div className="min-w-0 flex-1">
          <p className="truncate font-semibold">{line.name}</p>
          <p className={cn("num truncate text-xs", dark ? "text-white/60" : "text-muted-foreground")}>
            {line.barcode ? <>{t("Barcode")}: {line.barcode}</> : t("No barcode scanned")}
          </p>
        </div>
        <span className={cn("shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium", dark ? "bg-white/15" : "bg-muted")}>
          {t(line.kind === "shared" ? "Shared barcode" : "Individual barcode")}
        </span>
      </div>
      <div className="mt-2 flex items-center justify-between gap-3">
        <p role="status" aria-live="polite" className="min-w-0">
          <span key={`${line.key}:${line.quantity}`} className="num inline-block animate-pop text-2xl font-bold">{units(line.quantity)}</span>{" "}
          <span className={cn("text-xs", dark ? "text-white/70" : "text-muted-foreground")}>{t("captured")}</span>
        </p>
        <div className="flex items-center gap-1.5">
          {line.kind === "shared" && onSet && (
            <>
              <button type="button" className={stepper} disabled={line.quantity <= min} onClick={() => set(line.quantity - 1)} aria-label={t("Decrease quantity")}><Minus className="h-4 w-4" /></button>
              <QuantityField key={`${line.key}:${line.quantity}`} value={line.quantity} dark={dark} onCommit={set} onScanned={onScanned} />
              <button type="button" className={stepper} disabled={atMax} onClick={() => set(line.quantity + 1)} aria-label={t("Increase quantity")}><Plus className="h-4 w-4" /></button>
            </>
          )}
          {onRemove && (
            <button type="button" className={cn(stepper, "w-11", dark ? "text-red-300" : "text-destructive")} onClick={() => { onRemove(line.key); onDone?.(); }} aria-label={t("Remove from session")}>
              <Trash2 className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>
      {line.kind === "shared" && atMax && <p className={cn("mt-1 text-xs", dark ? "text-amber-200" : "text-warning")}>{t("That is the most allowed here")} ({count(line.max ?? 0)})</p>}
      {others.length > 0 && (
        <>
          <ul className={cn("mt-2 max-h-24 divide-y overflow-y-auto border-t", dark ? "divide-white/10 border-white/10" : "")}>
            {[...others].reverse().map((l) => (
              <li key={l.key}>
                <button type="button" className="flex w-full items-center justify-between gap-2 py-1.5 text-start text-xs" onClick={() => setPicked(l.key)}>
                  <span className="min-w-0 truncate">{l.name}{l.kind === "unique" && l.barcode ? ` · ${l.barcode}` : ""}</span>
                  <span className="num shrink-0 font-semibold">{count(l.quantity)}</span>
                </button>
              </li>
            ))}
          </ul>
          <p className={cn("mt-1 text-end text-xs font-medium", dark ? "text-white/80" : "text-muted-foreground")} data-testid="scan-session-total">
            {t("This session")}: <span className="num">{units(total)}</span> · <span className="num">{count(lines.length)}</span> {t(new Set(lines.map((l) => l.name)).size === 1 ? "entries" : "products")}
          </p>
        </>
      )}
    </div>
  );
}

/** Typed corrections apply on Enter or on leaving the field. A code arriving at scanner speed is passed on as a scan. */
function QuantityField({ value, dark, onCommit, onScanned }: { value: number; dark: boolean; onCommit: (q: number) => void; onScanned?: (code: string) => void }) {
  const [draft, setDraft] = useState(String(value));
  const timing = useKeyTiming();
  const finish = () => {
    const typed = draft.trim();
    if (timing.source(typed) === "scanner") {
      setDraft(String(value));
      onScanned?.(typed);
      return;
    }
    const q = parseInt(typed.replace(/\D/g, ""), 10);
    if (Number.isNaN(q)) setDraft(String(value));
    else if (q !== value) onCommit(q);
  };
  return (
    <input
      inputMode="numeric"
      aria-label={t("Quantity")}
      className={cn("num h-11 w-14 rounded-xl border bg-transparent text-center font-semibold outline-hidden focus-visible:ring-2 focus-visible:ring-ring", dark ? "border-white/25" : "border-input")}
      value={draft}
      onFocus={(e) => e.target.select()}
      onKeyDown={(e) => {
        timing.onKeyDown(e);
        if (e.key === "Enter") {
          e.preventDefault();
          finish();
        }
      }}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={finish}
    />
  );
}
