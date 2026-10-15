/**
 * Roadmap 115 — the sale date: today by default (the business's time zone); people allowed to backdate may pick any
 * earlier day, never a future one (the server checks the same). Cart lines may carry their own date.
 */
import { CalendarDays } from "lucide-react";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { EARLIEST_SALE_DATE } from "@/lib/dates";

export { dayLabel, todayIn } from "@/lib/dates";

/** A date input that only accepts today or earlier. */
export function SaleDateInput({ value, today, onChange, disabled, className, label = "Sale Date" }: {
  value: string;
  today: string;
  onChange: (day: string) => void;
  disabled?: boolean;
  className?: string;
  label?: string;
}) {
  return (
    <input
      type="date"
      aria-label={t(label)}
      value={value}
      min={EARLIEST_SALE_DATE}
      max={today}
      disabled={disabled}
      onChange={(e) => {
        const v = e.target.value;
        if (v && v >= EARLIEST_SALE_DATE && v <= today) onChange(v);
      }}
      className={cn("num min-w-0 bg-transparent font-medium outline-hidden disabled:opacity-100", className)}
    />
  );
}

/** The compact "Sale Date" control beside the Sale Owner on New Sale. */
export function SaleDateBar({ value, today, onChange, editable }: { value: string; today: string; onChange: (day: string) => void; editable: boolean }) {
  const earlier = value < today;
  return (
    <label
      className={cn("flex shrink-0 items-center gap-2 rounded-xl border bg-card px-3 py-2 text-sm", earlier && "border-warning/60 bg-warning/5")}
      title={editable ? undefined : t("Only people allowed to record sales with an earlier date can change it")}
    >
      <CalendarDays className={cn("h-4 w-4 shrink-0", earlier ? "text-warning" : "text-muted-foreground")} />
      <span className="hidden text-muted-foreground sm:inline">{t("Sale Date")}:</span>
      {editable ? <SaleDateInput value={value} today={today} onChange={onChange} /> : <span className="num font-medium">{t("Today")}</span>}
    </label>
  );
}
