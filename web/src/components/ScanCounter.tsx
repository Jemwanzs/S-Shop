/** Roadmap 153: the live count of a scanning session, shown beside the scanning field — the product last scanned with its
 * quantity and controls, the other products, and the session total. The same panel inside the camera scanner (dark) and
 * under the inline field (light). Shared barcodes count units; an individual barcode is always exactly one unit.
 * Roadmap 165: a large quantity with green +, then −, then Delete. Tapping the quantity sets it, or adds to it. The controls
 * never touch the camera's 3-second clock. An individual barcode keeps + and − switched off: it is one physical item. */
import { useEffect, useState } from "react";
import { Check, Minus, Plus, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { Link } from "@/lib/router";
import { useSession } from "@/lib/session";
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
  /** The product, for the way to its tracking setting when an individual barcode cannot count several items. */
  productId?: string;
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
  const { can } = useSession();
  const [picked, setPicked] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  useEffect(() => { setPicked(null); setEditing(false); }, [activeKey]);
  if (lines.length === 0) return null;
  const line = lines.find((l) => l.key === picked) ?? lines.find((l) => l.key === activeKey) ?? lines[lines.length - 1];
  const others = lines.filter((l) => l.key !== line.key);
  const total = lines.reduce((a, l) => a + l.quantity, 0);
  const dark = tone === "dark";
  const min = line.min ?? 1;
  const shared = line.kind === "shared";
  const adjustable = shared && !!onSet;
  const atMax = line.max != null && line.quantity >= line.max;
  const clamp = (q: number) => Math.max(min, line.max != null ? Math.min(line.max, q) : q);
  const set = (q: number) => {
    onSet?.(line.key, clamp(q));
    onDone?.();
  };
  const button = "flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border transition-colors active:scale-95 disabled:pointer-events-none disabled:opacity-35";
  const outline = dark ? "border-white/25 hover:bg-white/10" : "border-input bg-card hover:bg-accent";
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
          {t(shared ? "Shared barcode" : "Individual barcode")}
        </span>
      </div>
      <div className="mt-3 flex items-end justify-between gap-2">
        <div className="min-w-0">
          <p className={cn("label-caps mb-1 text-[10px]", dark && "text-white/60")}>{t("Captured quantity")}</p>
          <button type="button" disabled={!adjustable} onClick={() => setEditing(true)} aria-label={t("Change the quantity")}
            className={cn("rounded-xl border-2 px-3 py-1 text-start transition-colors", adjustable ? (dark ? "border-white/30 hover:bg-white/10" : "border-primary/40 bg-primary/5 hover:bg-primary/10") : "border-transparent px-0")}>
            <span role="status" aria-live="polite" key={`${line.key}:${line.quantity}`} className="num inline-block animate-pop text-3xl font-bold leading-tight">{units(line.quantity)}</span>
          </button>
        </div>
        <div className="flex items-center gap-1.5">
          {(onSet || !shared) && (
            <>
              <button type="button" className={cn(button, "border-transparent bg-emerald-600 text-white hover:bg-emerald-700")} disabled={!adjustable || atMax}
                onClick={() => set(line.quantity + 1)} aria-label={t("Increase quantity")}><Plus className="h-5 w-5" strokeWidth={2.75} /></button>
              <button type="button" className={cn(button, outline)} disabled={!adjustable || line.quantity <= min}
                onClick={() => set(line.quantity - 1)} aria-label={t("Decrease quantity")}><Minus className="h-5 w-5" strokeWidth={2.75} /></button>
            </>
          )}
          {onRemove && (
            <button type="button" className={cn(button, outline, dark ? "text-red-300" : "text-destructive")} onClick={() => { onRemove(line.key); onDone?.(); }} aria-label={t("Remove from session")}>
              <Trash2 className="h-5 w-5" />
            </button>
          )}
        </div>
      </div>
      {editing && adjustable && (
        <QuantityEditor value={line.quantity} min={min} max={line.max ?? null} dark={dark} onScanned={onScanned}
          onCommit={(q) => { setEditing(false); set(q); }} onCancel={() => { setEditing(false); onDone?.(); }} />
      )}
      {shared && atMax && <p className={cn("mt-1 text-xs", dark ? "text-amber-200" : "text-warning")}>{t("That is the most allowed here")} ({count(line.max ?? 0)})</p>}
      {!shared && (
        <p className={cn("mt-2 text-xs", dark ? "text-white/65" : "text-muted-foreground")}>
          {t("Each item has its own barcode, so + and − are off: scan each item's label.")}
          {line.productId && can("products.edit") && (
            <> {t("Items that share one barcode?")} <Link to={`/products/${line.productId}/edit`} className={cn("font-medium underline", dark ? "text-white" : "text-primary")}>{t("Use Shared barcode for this product")}</Link></>
          )}
        </p>
      )}
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

/** Roadmap 165: type a quantity and choose whether it replaces the count (Set to) or is added to it (Add). Applies on Confirm
 * or Enter. A code arriving at scanner speed is passed on as a scan, never taken as a quantity. */
function QuantityEditor({ value, min, max, dark, onCommit, onCancel, onScanned }: {
  value: number; min: number; max: number | null; dark: boolean;
  onCommit: (q: number) => void; onCancel: () => void; onScanned?: (code: string) => void;
}) {
  const [mode, setMode] = useState<"set" | "add">("set");
  const [draft, setDraft] = useState("");
  const timing = useKeyTiming();
  const n = parseInt(draft.replace(/\D/g, ""), 10);
  const result = Number.isNaN(n) ? null : mode === "set" ? n : value + n;
  const problem = result === null ? null
    : result < min ? `${t("At least")} ${count(min)}`
    : max != null && result > max ? `${t("That is the most allowed here")} (${count(max)})`
    : null;
  const confirm = () => {
    const typed = draft.trim();
    if (timing.source(typed) === "scanner") {
      setDraft("");
      onScanned?.(typed);
      return;
    }
    if (result !== null && !problem) onCommit(result);
  };
  const seg = (on: boolean) => cn("h-9 flex-1 rounded-lg text-xs font-medium", on ? (dark ? "bg-white text-black" : "bg-primary text-primary-foreground") : dark ? "text-white/75" : "text-muted-foreground");
  return (
    <div className={cn("mt-3 space-y-2 rounded-xl p-2.5", dark ? "bg-black/40" : "bg-muted/60")}>
      <div className={cn("flex gap-1 rounded-xl p-1", dark ? "bg-white/10" : "bg-card")} role="radiogroup" aria-label={t("Quantity")}>
        <button type="button" role="radio" aria-checked={mode === "set"} className={seg(mode === "set")} onClick={() => setMode("set")}>{t("Set quantity to")}</button>
        <button type="button" role="radio" aria-checked={mode === "add"} className={seg(mode === "add")} onClick={() => setMode("add")}>{t("Add to quantity")}</button>
      </div>
      <div className="flex gap-2">
        <input autoFocus inputMode="numeric" aria-label={t(mode === "set" ? "New quantity" : "Units to add")} placeholder={mode === "set" ? String(value) : "+1"}
          className={cn("num h-11 min-w-0 flex-1 rounded-xl border bg-transparent px-3 text-lg font-semibold outline-hidden focus-visible:ring-2 focus-visible:ring-ring", dark ? "border-white/25" : "border-input bg-card")}
          value={draft} onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { timing.onKeyDown(e); if (e.key === "Enter") { e.preventDefault(); confirm(); } if (e.key === "Escape") onCancel(); }} />
        <button type="button" className={cn("h-11 rounded-xl px-4 text-sm font-semibold disabled:opacity-40", dark ? "bg-white text-black" : "bg-primary text-primary-foreground")} disabled={result === null || !!problem} onClick={confirm}>{t("Confirm")}</button>
        <button type="button" className={cn("h-11 rounded-xl px-3 text-sm", dark ? "text-white/80" : "text-muted-foreground")} onClick={onCancel}>{t("Cancel")}</button>
      </div>
      <p className={cn("text-xs", problem ? (dark ? "text-amber-200" : "text-warning") : dark ? "text-white/65" : "text-muted-foreground")}>
        {problem ?? (result === null ? t(mode === "set" ? "The count becomes the number you type." : "The number you type is added to the count.") : `${units(value)} → ${units(result)}`)}
      </p>
    </div>
  );
}
