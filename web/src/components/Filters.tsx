import { forwardRef, useState, type ReactNode } from "react";
import { CalendarRange, Search, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { todayIso } from "@/lib/format";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { t, tChildren, tx } from "@/lib/i18n";

export interface PeriodValue {
  period?: string;
  from?: string;
  to?: string;
}

const PRESETS = [
  ["today", "Today"],
  ["yesterday", "Yesterday"],
  ["week", "This week"],
  ["month", "This month"],
  ["year", "This year"],
] as const;

/** Quick period chips + specific date / date range. Horizontal scroll on phones. */
export function PeriodFilter({ value, onChange, presets = PRESETS.map((p) => p[0]) }: { value: PeriodValue; onChange: (v: PeriodValue) => void; presets?: string[] }) {
  const custom = !!value.from;
  const [from, setFrom] = useState(value.from ?? todayIso());
  const [to, setTo] = useState(value.to ?? todayIso());
  const [open, setOpen] = useState(false);
  return (
    <div className="scrollbar-none -mx-3.5 flex gap-1.5 overflow-x-auto px-3.5 md:mx-0 md:flex-wrap md:px-0">
      {PRESETS.filter(([k]) => presets.includes(k)).map(([k, label]) => (
        <Chip key={k} active={!custom && value.period === k} onClick={() => onChange({ period: k })}>
          {t(label)}
        </Chip>
      ))}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Chip active={custom}>
            <CalendarRange className="h-3.5 w-3.5" />
            {custom ? (value.from === value.to ? value.from : `${value.from} → ${value.to}`) : t("Dates")}
          </Chip>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-72 space-y-3">
          <label className="block text-sm">
            <span className="label-caps">{t("From")}</span>
            <Input type="date" value={from} max={to} onChange={(e) => setFrom(e.target.value)} />
          </label>
          <label className="block text-sm">
            <span className="label-caps">{t("To")}</span>
            <Input type="date" value={to} min={from} onChange={(e) => setTo(e.target.value)} />
          </label>
          <div className="flex gap-2">
            <Button variant="outline" className="flex-1" onClick={() => { onChange({ from, to: from }); setOpen(false); }}>
              {t("Single day")}
            </Button>
            <ActionButton className="flex-1" blockedBy={[!!from && !!to && from > to && "Check the dates", value.from === from && value.to === to && REASONS.noChanges]}
              onAction={() => { onChange({ from, to }); setOpen(false); }}>
              {t("Apply")}
            </ActionButton>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );
}

export const Chip = forwardRef<HTMLButtonElement, { active?: boolean; children: ReactNode } & React.ButtonHTMLAttributes<HTMLButtonElement>>(function Chip(
  { active, children, className, ...props },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      {...props}
      className={cn(
        "inline-flex h-9 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full border px-3.5 text-sm transition-colors",
        active ? "border-foreground bg-foreground text-background" : "bg-card text-muted-foreground hover:text-foreground",
        className,
      )}
    >
      {tChildren(children)}
    </button>
  );
});

export function SearchInput({ value, onChange, placeholder = "Search…", className, autoFocus, trailing }: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  className?: string;
  autoFocus?: boolean;
  trailing?: ReactNode;
}) {
  return (
    <div className={cn("relative", className)}>
      <Search className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
      <Input value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} className="bg-card ps-9 pe-20" autoFocus={autoFocus} />
      <div className="absolute end-1.5 top-1/2 flex -translate-y-1/2 items-center gap-1">
        {value && (
          <button type="button" onClick={() => onChange("")} className="rounded-full p-1.5 text-muted-foreground hover:bg-muted" aria-label="Clear">
            <X className="h-4 w-4" />
          </button>
        )}
        {trailing}
      </div>
    </div>
  );
}

/** Segmented tabs that scroll horizontally on phones. */
export function Segments<T extends string>({ value, onChange, options }: { value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; count?: number }[] }) {
  return (
    <div className="scrollbar-none -mx-3.5 flex gap-1.5 overflow-x-auto px-3.5 md:mx-0 md:flex-wrap md:px-0">
      {options.map((o) => (
        <Chip key={o.value} active={o.value === value} onClick={() => onChange(o.value)}>
          {tx(o.label)}
          {o.count ? <span className={cn("num rounded-full px-1.5 text-xs", o.value === value ? "bg-background/20" : "bg-muted")}>{o.count}</span> : null}
        </Chip>
      ))}
    </div>
  );
}
