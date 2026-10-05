import { useSyncExternalStore, type ReactNode } from "react";
import { toast as sonner, type ExternalToast } from "sonner";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { t } from "@/lib/i18n";

/**
 * App-wide notifications. Confirmations (success/info) are brief toasts at the top centre;
 * errors are centred alerts that stay until dismissed, so they are never missed.
 * Import `toast` from here, not from "sonner".
 */
interface Alert {
  id: number;
  message: ReactNode;
  description?: ReactNode;
}

let queue: Alert[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

function showError(message: ReactNode, data?: ExternalToast) {
  // The same message twice in a row (e.g. a retried request) is shown once.
  if (queue.some((a) => a.message === message)) return 0;
  const alert = { id: nextId++, message, description: data?.description as ReactNode };
  queue = [...queue, alert];
  emit();
  return alert.id;
}

function dismiss(id: number) {
  queue = queue.filter((a) => a.id !== id);
  emit();
}

export const toast = Object.assign(((message: ReactNode, data?: ExternalToast) => sonner(message, data)) as typeof sonner, sonner, {
  error: showError,
});

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

/** Renders the oldest pending error; dismissing it reveals the next. Mounted once in App. */
export function AlertHost() {
  const alerts = useSyncExternalStore(subscribe, () => queue);
  const current = alerts[0];
  return (
    <Dialog open={!!current} onOpenChange={(open) => !open && current && dismiss(current.id)}>
      {current && (
        <DialogContent className="w-[calc(100%-2rem)] max-w-sm gap-0 rounded-2xl p-0 text-center" role="alertdialog">
          <div className="space-y-3 px-6 pb-5 pt-8">
            <span className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-destructive/10 text-destructive">
              <AlertTriangle className="h-6 w-6" />
            </span>
            <DialogTitle className="text-base leading-snug">{current.message}</DialogTitle>
            {current.description ? (
              <DialogDescription>{current.description}</DialogDescription>
            ) : (
              <DialogDescription className="sr-only">Error</DialogDescription>
            )}
          </div>
          <div className="border-t p-3">
            <Button className="w-full" onClick={() => dismiss(current.id)} autoFocus>
              {t("OK")}{alerts.length > 1 && <span className="text-xs opacity-80"> · {alerts.length - 1} {t("more")}</span>}
            </Button>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
