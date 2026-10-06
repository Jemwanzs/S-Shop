import { Check } from "lucide-react";
import { cn } from "@/lib/utils";
import mark from "@/assets/sshop-mark.png";
import { t } from "@/lib/i18n";

export function PortalHeader({ name, tagline, logo }: { name: string; tagline?: string; logo?: string | null }) {
  return (
    <header className="border-b px-5 pb-6 pt-8 text-center">
      {logo ? (
        <img src={logo} alt="" className="mx-auto h-24 w-24 rounded-2xl object-cover shadow-lift" />
      ) : (
        <div className="mx-auto flex h-20 w-20 items-center justify-center rounded-2xl bg-primary text-3xl font-bold text-primary-foreground shadow-lift">{name[0]}</div>
      )}
      <h1 className="mt-4 text-2xl font-semibold tracking-tight sm:text-3xl">{name}</h1>
      {tagline && <p className="mx-auto mt-2 max-w-md text-muted-foreground">{tagline}</p>}
    </header>
  );
}

export interface Step {
  status: string;
  label: string;
  done: boolean;
  current: boolean;
}

/** Order Received ✓ → Preparing ✓ → On Delivery ● → Delivered → Completed */
export function Steps({ steps, compact }: { steps: Step[]; compact?: boolean }) {
  return (
    <ol className={cn("space-y-3", compact && "space-y-2")}>
      {steps.map((s) => (
        <li key={s.status} className="flex items-center gap-3">
          <span
            className={cn(
              "flex shrink-0 items-center justify-center rounded-full border-2",
              compact ? "h-6 w-6" : "h-8 w-8",
              s.done ? "border-success text-success" : "border-border text-muted-foreground",
              s.current && "bg-success text-success-foreground ring-4 ring-success/20",
            )}
          >
            {s.done ? <Check className={compact ? "h-3.5 w-3.5" : "h-4 w-4"} /> : <span className="h-1.5 w-1.5 rounded-full bg-current" />}
          </span>
          <span className={cn(s.done ? "font-medium" : "text-muted-foreground", compact && "text-sm")}>{s.label}</span>
        </li>
      ))}
    </ol>
  );
}

/** Subtle platform credit on customer-facing pages; the business brand stays primary. */
export function PoweredBy() {
  return (
    <a href="/" className="mx-auto flex w-fit items-center gap-1.5 py-6 text-xs text-muted-foreground hover:text-foreground">
      <img src={mark} alt="" className="h-4 w-4" /> {t("Powered by")} <span className="text-brand font-semibold">{t("S'Shop")}</span>
    </a>
  );
}
