import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, ArrowLeftRight, Bell, CheckCheck, ClipboardList, HandCoins, PackageX, ShieldCheck } from "lucide-react";
import { api } from "@/lib/api";
import { ago } from "@/lib/format";
import type { Notification } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { EmptyState, Loading, PageHeader } from "@/components/Page";

const ICONS: Record<string, typeof Bell> = {
  low_stock: AlertTriangle,
  out_of_stock: PackageX,
  new_order: ClipboardList,
  approval_pending: ShieldCheck,
  approval_decided: ShieldCheck,
  transfer_in_transit: ArrowLeftRight,
  transfer_received: ArrowLeftRight,
  credit_overdue: HandCoins,
};

export default function Notifications() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const { data, isLoading } = useQuery({
    queryKey: ["notifications"],
    queryFn: () => api<{ items: Notification[]; unread: number; pending_approvals: number }>("/notifications", { query: { limit: 30 } }),
  });
  const readAll = useMutation({ mutationFn: () => api("/notifications/read-all", { method: "POST" }), onSuccess: () => qc.invalidateQueries({ queryKey: ["notifications"] }) });
  const open = async (n: Notification) => {
    if (!n.read_at) {
      await api(`/notifications/${n.id}/read`, { method: "POST" });
      qc.invalidateQueries({ queryKey: ["notifications"] });
    }
    if (n.link) navigate(n.link);
  };
  return (
    <div className="mx-auto max-w-3xl">
      <PageHeader
        title="Notifications"
        description={data?.pending_approvals ? `${data.pending_approvals} request(s) awaiting approval` : undefined}
        actions={data?.unread ? <Button variant="outline" onClick={() => readAll.mutate()}><CheckCheck /> Mark all read</Button> : undefined}
      />
      {isLoading ? (
        <Loading />
      ) : !data?.items.length ? (
        <div className="surface"><EmptyState icon={Bell} title="You're all caught up" hint="Low stock, new orders, approvals and overdue credit show up here." /></div>
      ) : (
        <ul className="surface divide-y overflow-hidden">
          {data.items.map((n) => {
            const Icon = ICONS[n.kind] ?? Bell;
            return (
              <li key={n.id}>
                <button onClick={() => open(n)} className={cn("flex w-full items-start gap-3 px-4 py-3.5 text-start transition-colors hover:bg-accent/40", !n.read_at && "bg-primary/5")}>
                  <span className={cn("mt-0.5 rounded-lg p-2", !n.read_at ? "bg-primary/15 text-primary" : "bg-muted text-muted-foreground")}><Icon className="h-4 w-4" /></span>
                  <span className="min-w-0 flex-1">
                    <span className={cn("block", !n.read_at && "font-semibold")}>{n.title}</span>
                    {n.body && <span className="block text-sm text-muted-foreground">{n.body}</span>}
                  </span>
                  <span className="shrink-0 text-xs text-muted-foreground">{ago(n.created_at)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
