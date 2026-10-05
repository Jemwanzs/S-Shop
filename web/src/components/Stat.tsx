import type { ReactNode } from "react";
import { TrendingDown, TrendingUp } from "lucide-react";
import { cn } from "@/lib/utils";
import { toNum } from "@/lib/format";

export function StatCard({
  label,
  value,
  icon: Icon,
  change,
  hint,
  tone = "default",
  className,
}: {
  label: string;
  value: ReactNode;
  icon?: typeof TrendingUp;
  change?: number | string | null;
  hint?: ReactNode;
  tone?: "default" | "primary" | "success" | "warning" | "danger";
  className?: string;
}) {
  const c = change === null || change === undefined ? null : toNum(change);
  const iconTone = {
    default: "text-muted-foreground bg-muted",
    primary: "text-primary bg-primary/10",
    success: "text-success bg-success/10",
    warning: "text-warning bg-warning/15",
    danger: "text-destructive bg-destructive/10",
  }[tone];
  return (
    <div className={cn("surface flex min-w-0 flex-col gap-1.5 p-3 animate-fade-up lg:gap-2 lg:p-4", className)}>
      <div className="flex items-center gap-2">
        {Icon && (
          <span className={cn("rounded-md p-1", iconTone)}>
            <Icon className="h-3.5 w-3.5" />
          </span>
        )}
        <span className="label-caps truncate">{label}</span>
      </div>
      <div className="num truncate text-lg font-semibold lg:text-2xl">{value}</div>
      {(c !== null || hint) && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {c !== null && (
            <span className={cn("inline-flex items-center gap-0.5 font-medium", c >= 0 ? "text-success" : "text-destructive")}>
              {c >= 0 ? <TrendingUp className="h-3 w-3" /> : <TrendingDown className="h-3 w-3" />}
              {Math.abs(c)}%
            </span>
          )}
          {hint}
        </div>
      )}
    </div>
  );
}
