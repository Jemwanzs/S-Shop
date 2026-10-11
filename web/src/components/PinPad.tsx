/** Compact numeric keypad with masked dots (roadmap 83; one dot per digit entered, roadmap 89): tap or type digits; Backspace deletes; submits itself when
 * the PIN reaches `length` digits (or on Enter once at least `min` are entered). */
import { useEffect } from "react";
import { Delete } from "lucide-react";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

export function PinPad({ value, onChange, onSubmit, min = 4, max = 6, disabled, error }: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (v: string) => void;
  min?: number;
  max?: number;
  disabled?: boolean;
  error?: boolean;
}) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (disabled || e.ctrlKey || e.metaKey || e.altKey) return;
      if (/^\d$/.test(e.key) && value.length < max) onChange(value + e.key);
      else if (e.key === "Backspace") onChange(value.slice(0, -1));
      else if (e.key === "Enter" && value.length >= min) onSubmit(value);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [value, onChange, onSubmit, min, max, disabled]);

  const press = (d: string) => {
    if (disabled || value.length >= max) return;
    const next = value + d;
    onChange(next);
    if (next.length === max) onSubmit(next);
  };
  return (
    <div className="space-y-5">
      {/* Only the digits entered, centred: no empty placeholders, so the PIN's length is never shown. The row keeps its
          height when empty so the keypad never moves. */}
      <div className={cn("flex h-3.5 items-center justify-center gap-3", error && "animate-[shake_.35s_ease] motion-reduce:animate-none")}
        role="status" aria-live="polite" aria-label={`${value.length} ${t("digits entered")}`}>
        {Array.from({ length: value.length }, (_, i) => (
          <span key={i} className="h-3.5 w-3.5 rounded-full bg-primary animate-pop motion-reduce:animate-none" />
        ))}
      </div>
      <div className="mx-auto grid max-w-[272px] grid-cols-3 gap-2.5">
        {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((d) => (
          <Key key={d} onClick={() => press(d)} disabled={disabled}>{d}</Key>
        ))}
        <Key onClick={() => value.length >= min && onSubmit(value)} disabled={disabled || value.length < min} aria-label={t("Sign in")} className="text-sm font-semibold text-primary">OK</Key>
        <Key onClick={() => press("0")} disabled={disabled}>0</Key>
        <Key onClick={() => onChange(value.slice(0, -1))} disabled={disabled || !value} aria-label={t("Delete")}><Delete className="h-5 w-5" /></Key>
      </div>
    </div>
  );
}

function Key({ className, ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" {...props}
      className={cn("flex h-14 items-center justify-center rounded-2xl border bg-card text-xl font-medium shadow-sm transition active:scale-95 disabled:opacity-40 motion-reduce:transition-none", className)} />
  );
}
