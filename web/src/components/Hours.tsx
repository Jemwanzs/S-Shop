import { useEffect, useState } from "react";
import { Clock } from "lucide-react";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { Hours } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Form";

export const DAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
export const DEFAULT_HOURS: Hours = { days: [true, true, true, true, true, true, true], open: "00:00", close: "00:00" };

const mins = (hhmm: string) => {
  const [h, m] = hhmm.split(":").map(Number);
  return h * 60 + m;
};

/** Minutes after midnight that still belong to the previous business day (mirrors the server). */
export function dayShift(h: Hours) {
  const o = mins(h.open);
  const c = mins(h.close);
  return c <= o ? c : 0;
}

export const isAllDay = (h: Hours) => h.open === h.close;
export const crossesMidnight = (h: Hours) => !isAllDay(h) && mins(h.close) < mins(h.open) && mins(h.close) > 0;

/** "Mon–Fri · 08:00–20:00", "Every day · open 24 hours". */
export function hoursLabel(h: Hours) {
  const on = h.days.map((d, i) => (d ? i : -1)).filter((i) => i >= 0);
  const run = on.length > 1 && on.every((d, i) => i === 0 || d === on[i - 1] + 1);
  const days =
    on.length === 7 ? t("Every day") : run ? `${t(DAY_LABELS[on[0]])}–${t(DAY_LABELS[on[on.length - 1]])}` : on.map((i) => t(DAY_LABELS[i])).join(", ");
  const time = isAllDay(h) ? t("open 24 hours") : `${h.open}–${h.close}${crossesMidnight(h) ? ` (${t("next day")})` : ""}`;
  return `${days} · ${time}`;
}

/** Open right now in the business time zone? Days follow the business day, like the server check. */
export function isOpen(h: Hours, timeZone: string, now = new Date()) {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat("en-GB", { timeZone, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hourCycle: "h23" })
      .formatToParts(now)
      .map((p) => [p.type, p.value]),
  );
  const local = Date.UTC(+parts.year, +parts.month - 1, +parts.day, +parts.hour, +parts.minute);
  const tm = +parts.hour * 60 + +parts.minute;
  const o = mins(h.open);
  const c = mins(h.close);
  const inHours = c > o ? tm >= o && tm < c : tm >= o || tm < c;
  const businessDay = new Date(local - dayShift(h) * 60_000).getUTCDay();
  return inHours && !!h.days[(businessDay + 6) % 7];
}

/** True when a record's business day is not the calendar day of its timestamp (late trading after midnight). */
export function lateTrade(createdAt: string, businessDate: string | undefined, timeZone: string | undefined) {
  if (!businessDate || !timeZone) return false;
  return new Intl.DateTimeFormat("en-CA", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date(createdAt)) !== businessDate;
}

/** Re-evaluates every minute so a banner flips at opening/closing time. */
export function useOpenNow(h: Hours | undefined, timeZone: string | undefined) {
  const [, tick] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => tick((n) => n + 1), 60_000);
    return () => window.clearInterval(id);
  }, []);
  return h && timeZone ? isOpen(h, timeZone) : true;
}

/** Working days + opening and closing time. */
export function HoursEditor({ value, onChange }: { value: Hours; onChange: (h: Hours) => void }) {
  return (
    <div className="space-y-4">
      <div>
        <p className="mb-1.5 text-[0.85rem] font-medium">{t("Working days")}</p>
        <div className="grid grid-cols-7 gap-1.5">
          {DAY_LABELS.map((d, i) => (
            <button
              key={d}
              type="button"
              aria-pressed={value.days[i]}
              onClick={() => onChange({ ...value, days: value.days.map((x, j) => (j === i ? !x : x)) })}
              className={cn(
                "h-control rounded-lg border text-xs font-medium transition-colors",
                value.days[i] ? "border-primary bg-primary text-primary-foreground" : "bg-background text-muted-foreground hover:bg-accent",
              )}
            >
              {t(d)}
            </button>
          ))}
        </div>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <Field label="Opens">
          <Input type="time" value={value.open} onChange={(e) => e.target.value && onChange({ ...value, open: e.target.value })} />
        </Field>
        <Field label="Closes">
          <Input type="time" value={value.close} onChange={(e) => e.target.value && onChange({ ...value, close: e.target.value })} />
        </Field>
      </div>
      <p className="flex items-start gap-2 text-xs text-muted-foreground">
        <Clock className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        <span>
          {isAllDay(value)
            ? t("Same opening and closing time: open 24 hours, the business day starts at that time.")
            : crossesMidnight(value)
              ? t("Closes after midnight: sales until closing time count for the day that opened.")
              : t("Sales count for the calendar day.")}
        </span>
      </p>
    </div>
  );
}
