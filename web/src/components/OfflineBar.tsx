import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, CloudOff, RefreshCw, Trash2 } from "lucide-react";
import { useSession } from "@/lib/session";
import { discardSale, retrySale, syncSales, useOfflineQueue, useOnline } from "@/lib/offline";
import { toast } from "@/lib/toast";
import { money, dateTime } from "@/lib/format";
import { t } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ConfirmDialog } from "@/components/Form";

/** Connection state, automatic sync of sales made offline, and the list of any the server refused. */
export function OfflineBar() {
  const { profile } = useSession();
  const qc = useQueryClient();
  const online = useOnline();
  const queue = useOfflineQueue(profile?.tenant.id, profile?.user.id);
  const pending = queue.filter((q) => q.status === "pending");
  const failed = queue.filter((q) => q.status === "failed");
  const [open, setOpen] = useState(false);
  const [discarding, setDiscarding] = useState<string | null>(null);

  // Sync on reconnect and every 30 s while anything is waiting.
  useEffect(() => {
    if (!profile || !online || pending.length === 0) return;
    let stop = false;
    const run = async () => {
      const r = await syncSales(profile.tenant.id, profile.user.id);
      if (stop) return;
      if (r.sent) {
        toast.success(`${r.sent} ${t("offline sale(s) synced")}`);
        qc.invalidateQueries({ queryKey: ["sales"] });
        qc.invalidateQueries({ queryKey: ["pos-products"] });
        qc.invalidateQueries({ queryKey: ["dashboard"] });
      }
      if (r.failed) toast.error(`${r.failed} ${t("offline sale(s) need attention")}`);
    };
    run();
    const id = window.setInterval(run, 30_000);
    return () => {
      stop = true;
      window.clearInterval(id);
    };
  }, [profile, online, pending.length, qc]);

  if (online && queue.length === 0) return null;
  return (
    <>
      <div className="flex items-center gap-2 bg-warning/15 px-4 py-2 text-xs text-warning no-print md:px-6 lg:px-8">
        {online ? <RefreshCw className="h-3.5 w-3.5 shrink-0" /> : <CloudOff className="h-3.5 w-3.5 shrink-0" />}
        <span className="flex-1">
          {!online && t("You're offline. Cash sales are saved on this device and sent when the connection is back.")}
          {online && pending.length > 0 && `${pending.length} ${t("offline sale(s) syncing…")}`}
          {pending.length > 0 && !online && ` · ${pending.length} ${t("waiting")}`}
        </span>
        {failed.length > 0 && (
          <button className="rounded-md bg-destructive px-2 py-0.5 font-medium text-destructive-foreground" onClick={() => setOpen(true)}>
            {failed.length} {t("need attention")}
          </button>
        )}
      </div>
      <ResponsiveDialog open={open} onOpenChange={setOpen} title="Offline sales needing attention" description="The server could not record these. Fix the cause (for example receive the stock), then retry — or discard a sale that should not be recorded.">
        <ul className="divide-y">
          {failed.map((q) => (
            <li key={q.client_ref} className="space-y-2 py-3 text-sm">
              <div className="flex items-center justify-between gap-3">
                <span className="num font-medium">{money(q.total)} · {q.items} {t("item(s)")}</span>
                <span className="text-xs text-muted-foreground">{dateTime(q.sold_at)}</span>
              </div>
              <p className="flex gap-1.5 text-xs text-destructive">
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>{q.error_title ? `${q.error_title}: ` : ""}{q.error}</span>
              </p>
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={() => retrySale(q.client_ref)}><RefreshCw /> {t("Retry")}</Button>
                <Button size="sm" variant="ghost" className="text-destructive" onClick={() => setDiscarding(q.client_ref)}><Trash2 /> {t("Discard")}</Button>
              </div>
            </li>
          ))}
          {failed.length === 0 && <li className="py-4 text-sm text-muted-foreground">{t("Nothing needs attention.")}</li>}
        </ul>
      </ResponsiveDialog>
      <ConfirmDialog
        open={!!discarding}
        onOpenChange={(o) => !o && setDiscarding(null)}
        title="Discard this offline sale?"
        description="It will not be recorded. Only discard a sale that did not really happen, or that you have recorded again."
        destructive
        confirmLabel="Discard"
        onConfirm={async () => {
          if (discarding) await discardSale(discarding);
          setDiscarding(null);
        }}
      />
    </>
  );
}
