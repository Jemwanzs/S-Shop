/** The notification centre. Roadmap 194: only notifications this person may see — by their *Notifications* permissions,
 * module permissions and branches, re-checked by the server on every read, count and open — with All / Unread / Read,
 * category and branch filters, mark one or all as read, and opening the record (refused if access was lost since). */
import { useState } from "react";
import { useNavigate } from "@/lib/router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, ArrowLeftRight, Bell, Check, CheckCheck, ClipboardList, Globe, HandCoins, PackageX, Receipt, ShieldAlert, ShieldCheck, Wallet } from "lucide-react";
import { api, ApiError } from "@/lib/api";
import { ago } from "@/lib/format";
import { toast } from "@/lib/toast";
import type { Notification } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ActionButton } from "@/components/ActionButton";
import { Button } from "@/components/ui/button";
import { EmptyState, Loading, PageHeader } from "@/components/Page";
import { Chip, Segments } from "@/components/Filters";
import { Select } from "@/components/Form";
import { t } from "@/lib/i18n";

const ICONS: Record<string, typeof Bell> = {
  low_stock: AlertTriangle,
  out_of_stock: PackageX,
  new_order: ClipboardList,
  approval_pending: ShieldCheck,
  approval_decided: ShieldCheck,
  approval_handled: ShieldCheck,
  transfer_in_transit: ArrowLeftRight,
  transfer_received: ArrowLeftRight,
  transfer_discrepancy: ArrowLeftRight,
  credit_overdue: HandCoins,
  mpesa_attention: Receipt,
};
const CATEGORY_ICONS: Record<string, typeof Bell> = { website: Globe, admin: ShieldAlert, expenses: Wallet, credit: HandCoins, orders: ClipboardList, stock: AlertTriangle };

interface Centre {
  items: (Notification & { category?: string | null; branch_name?: string | null })[];
  unread: number;
  pending_approvals: number;
  categories: { key: string; label: string; unread: number }[];
  branches: { id: string; name: string }[];
}

export default function Notifications() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [status, setStatus] = useState<"all" | "unread" | "read">("all");
  const [category, setCategory] = useState("");
  const [branch, setBranch] = useState("");
  const { data, isLoading } = useQuery({
    queryKey: ["notifications", "centre", status, category, branch],
    queryFn: () => api<Centre>("/notifications", { query: { limit: 100, status, category: category || undefined, branch_id: branch || undefined } }),
  });
  const refresh = () => qc.invalidateQueries({ queryKey: ["notifications"] });
  const readAll = useMutation({ mutationFn: () => api("/notifications/read-all", { method: "POST" }), onSuccess: refresh });
  const markRead = useMutation({ mutationFn: (id: string) => api(`/notifications/${id}/read`, { method: "POST" }), onSuccess: refresh });
  const open = async (n: Notification) => {
    try {
      // The server re-checks access before giving the link back.
      const r = await api<{ link: string }>(`/notifications/${n.id}/read`, { method: "POST" });
      refresh();
      if (r.link) navigate(r.link);
    } catch (e) {
      refresh();
      toast.error(e instanceof ApiError && e.status === 404 ? t("This notification is no longer available to you") : e);
    }
  };
  return (
    <div className="mx-auto max-w-3xl">
      <PageHeader
        title="Notifications"
        description={data?.pending_approvals ? `${data.pending_approvals} ${t("request(s) awaiting your approval")}` : undefined}
        actions={<ActionButton variant="outline" online busy={readAll.isPending} blockedBy={[!data?.unread && "All read"]} onAction={() => readAll.mutateAsync()}><CheckCheck /> {t("Mark all read")}</ActionButton>}
      />
      <div className="mb-3 space-y-2">
        <div className="flex flex-wrap items-center gap-2">
          <Segments value={status} onChange={setStatus} options={[{ value: "all", label: "All" }, { value: "unread", label: `${t("Unread")}${data?.unread ? ` (${data.unread})` : ""}` }, { value: "read", label: "Read" }]} />
          {(data?.branches.length ?? 0) > 1 && (
            <div className="ms-auto w-44"><Select value={branch} onChange={setBranch} aria-label={t("Branch")}><option value="">{t("All my branches")}</option>{data!.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}</Select></div>
          )}
        </div>
        {(data?.categories.length ?? 0) > 1 && (
          <div className="flex flex-wrap gap-1.5" data-testid="notification-categories">
            <Chip active={!category} onClick={() => setCategory("")}>{t("All")}</Chip>
            {data!.categories.map((c) => (
              <Chip key={c.key} active={category === c.key} onClick={() => setCategory(category === c.key ? "" : c.key)}>
                {t(c.label)}{c.unread > 0 && <span className="num ms-1 rounded-full bg-primary px-1.5 text-[10px] text-primary-foreground">{c.unread}</span>}
              </Chip>
            ))}
          </div>
        )}
      </div>
      {isLoading ? (
        <Loading />
      ) : !data?.items.length ? (
        <div className="surface"><EmptyState icon={Bell} title={status === "unread" ? "You're all caught up" : "No notifications"} hint="Only notifications for your role, modules and branches appear here." /></div>
      ) : (
        <ul className="surface divide-y overflow-hidden" data-testid="notification-list">
          {data.items.map((n) => {
            const Icon = ICONS[n.kind] ?? CATEGORY_ICONS[n.category ?? ""] ?? Bell;
            return (
              <li key={n.id} className={cn("flex items-start", !n.read_at && "bg-primary/5")}>
                <button onClick={() => open(n)} className="flex min-w-0 flex-1 items-start gap-3 px-4 py-3.5 text-start transition-colors hover:bg-accent/40">
                  <span className={cn("mt-0.5 rounded-lg p-2", !n.read_at ? "bg-primary/15 text-primary" : "bg-muted text-muted-foreground")}><Icon className="h-4 w-4" /></span>
                  <span className="min-w-0 flex-1">
                    <span className={cn("block", !n.read_at && "font-semibold")}>{t(n.title)}</span>
                    {n.body && <span className="block text-sm text-muted-foreground">{t(n.body)}</span>}
                    <span className="mt-0.5 block text-xs text-muted-foreground">{ago(n.created_at)}{n.branch_name && ` · ${n.branch_name}`}</span>
                  </span>
                </button>
                {!n.read_at && (
                  <Button variant="ghost" size="icon-sm" className="m-2 shrink-0" aria-label={t("Mark as read")} onClick={() => markRead.mutate(n.id)}><Check /></Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
