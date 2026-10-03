import { cn } from "@/lib/utils";
import { count, titleCase } from "@/lib/format";

const TONES = {
  neutral: "bg-muted text-muted-foreground",
  primary: "bg-primary/10 text-primary",
  success: "bg-success/12 text-success",
  warning: "bg-warning/15 text-warning",
  danger: "bg-destructive/12 text-destructive",
  info: "bg-chart-2/12 text-chart-2",
} as const;
export type Tone = keyof typeof TONES;

export function Pill({ tone = "neutral", children, className }: { tone?: Tone; children: React.ReactNode; className?: string }) {
  return (
    <span className={cn("inline-flex items-center gap-1 whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium", TONES[tone], className)}>
      {children}
    </span>
  );
}

const STATUS_TONE: Record<string, Tone> = {
  completed: "success",
  paid: "success",
  received: "success",
  applied: "success",
  approved: "success",
  delivered: "success",
  in_stock: "success",
  active: "success",
  new: "primary",
  confirmed: "info",
  preparing: "info",
  dispatched: "info",
  on_delivery: "info",
  in_transit: "info",
  reserved: "info",
  pending: "warning",
  pending_approval: "warning",
  partially_paid: "warning",
  partially_returned: "warning",
  outstanding: "warning",
  draft: "neutral",
  overdue: "danger",
  cancelled: "danger",
  rejected: "danger",
  returned: "danger",
  written_off: "danger",
  void: "danger",
  sold: "neutral",
  inactive: "danger",
};

const LABELS: Record<string, string> = {
  new: "New",
  on_delivery: "On delivery",
  dispatched: "Dispatched",
  pending_approval: "Pending approval",
  partially_paid: "Partially paid",
  partially_returned: "Part returned",
  written_off: "Written off",
  in_transit: "In transit",
  in_stock: "In stock",
};

export function StatusBadge({ status, label, className }: { status: string; label?: string; className?: string }) {
  return (
    <Pill tone={STATUS_TONE[status] ?? "neutral"} className={className}>
      {label ?? LABELS[status] ?? titleCase(status)}
    </Pill>
  );
}

/** Legacy "(own|referral)" points pill. */
export function PointsPill({ own, referral, className }: { own: number; referral?: number; className?: string }) {
  return (
    <span className={cn("num inline-flex items-center rounded-full border border-points/30 bg-points/10 px-2.5 py-0.5 text-xs font-semibold text-points", className)}>
      {referral === undefined ? `${count(own)} pts` : `(${count(own)}|${count(referral)})`}
    </span>
  );
}

const MEDALS: Record<string, { icon: string; cls: string }> = {
  Gold: { icon: "🥇", cls: "text-gold" },
  Silver: { icon: "🥈", cls: "text-silver" },
  Bronze: { icon: "🥉", cls: "text-bronze" },
};

export function Medal({ tier, showLabel = false }: { tier?: string | null; showLabel?: boolean }) {
  if (!tier) return null;
  const m = MEDALS[tier];
  if (!m) return <Pill tone="primary">{tier}</Pill>;
  return (
    <span className={cn("inline-flex items-center gap-1 text-xs font-semibold", m.cls)} title={tier}>
      <span aria-hidden>{m.icon}</span>
      {showLabel && tier}
    </span>
  );
}

export function StockIndicator({ available, threshold = 3 }: { available: number; threshold?: number }) {
  if (available <= 0) return <Pill tone="danger">Out of stock</Pill>;
  if (available <= threshold) return <Pill tone="warning">{count(available)} left</Pill>;
  return <Pill tone="success">{count(available)} in stock</Pill>;
}
