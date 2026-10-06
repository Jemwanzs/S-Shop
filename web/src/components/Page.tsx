import type { ReactNode } from "react";
import { AlertTriangle, Inbox, Loader2 } from "lucide-react";
import { Link } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import { cn } from "@/lib/utils";
import { errorMessage } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { t, tx } from "@/lib/i18n";

export function PageHeader({
  eyebrow,
  title,
  description,
  actions,
  back,
}: {
  eyebrow?: string;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  back?: string;
}) {
  return (
    <header className="mb-4 flex flex-wrap items-end justify-between gap-3 animate-fade-up lg:mb-6">
      <div className="min-w-0">
        {back && (
          <Link to={back} className="mb-2 inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground">
            <ArrowLeft className="h-4 w-4 rtl:rotate-180" /> {t("Back")}
          </Link>
        )}
        {eyebrow && <p className="label-caps mb-1">{t(eyebrow)}</p>}
        <h1 className="truncate text-xl font-semibold tracking-tight lg:text-2xl">{tx(title)}</h1>
        {description && <p className="mt-1 text-sm text-muted-foreground">{tx(description)}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}

export function Section({ title, action, children, className }: { title?: ReactNode; action?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={cn("surface card-body min-w-0", className)}>
      {(title || action) && (
        <div className="mb-3 flex items-center justify-between gap-2">
          {title && <h2 className="label-caps">{tx(title)}</h2>}
          {action}
        </div>
      )}
      {children}
    </section>
  );
}

export function EmptyState({ icon: Icon = Inbox, title, hint, action }: { icon?: typeof Inbox; title: string; hint?: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-4 py-12 text-center">
      <div className="mb-1 rounded-full bg-muted p-3">
        <Icon className="h-6 w-6 text-muted-foreground" />
      </div>
      <p className="font-medium">{t(title)}</p>
      {hint && <p className="max-w-sm text-sm text-muted-foreground">{t(hint)}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function Loading({ label = "Loading…", className }: { label?: string; className?: string }) {
  return (
    <div className={cn("flex items-center justify-center gap-2 py-12 text-sm text-muted-foreground", className)}>
      <Loader2 className="h-4 w-4 animate-spin" /> {t(label)}
    </div>
  );
}

export function ErrorState({ error, retry }: { error: unknown; retry?: () => void }) {
  return (
    <div className="flex flex-col items-center gap-3 px-4 py-12 text-center">
      <AlertTriangle className="h-6 w-6 text-destructive" />
      <p className="text-sm text-muted-foreground">{t(errorMessage(error))}</p>
      {retry && (
        <Button variant="outline" size="sm" onClick={retry}>
          {t("Try again")}
        </Button>
      )}
    </div>
  );
}

/** Key/value row used in detail panels. Values never wrap awkwardly. */
export function KV({ label, children, className }: { label: ReactNode; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex items-baseline justify-between gap-4 py-2 text-sm", className)}>
      <span className="text-muted-foreground">{label}</span>
      <span className="min-w-0 text-end font-medium">{children}</span>
    </div>
  );
}
