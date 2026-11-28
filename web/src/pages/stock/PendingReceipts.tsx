/** Roadmap 203: receipts of the same stock still waiting for approval, shown while receiving (product chosen, item label
 * scanned) and enforced again by the server when the receipt is submitted. Details follow the person's access. */
import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { BellRing, Clock3, ExternalLink, X } from "lucide-react";
import { Link } from "@/lib/router";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { count, dateTime } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import type { ReceivingControls } from "@/lib/types";

export const PENDING_TITLE = "Stock Receiving Pending Approval";

export interface PendingReceipt {
  id: string; reference: string; kind: "barcode" | "duplicate" | "additional"; blocking: boolean;
  product_name: string; product_code: string; barcodes: string[]; branch_name: string | null; quantity: number;
  delivery_note: string | null; submitted_by: string | null; submitted_at: string; status: string;
  level: number | null; levels: number | null; approvers: string[] | null; may_view: boolean;
}
interface PendingResponse { items: PendingReceipt[]; controls: ReceivingControls; title: string; message: string }

export function usePendingReceipts(p: { productId: string | null; branchId: string; barcodes?: string[]; quantity?: number; reference?: string; supplierId?: string }) {
  const barcodes = (p.barcodes ?? []).join(",");
  return useQuery({
    queryKey: ["receive-pending", p.productId, p.branchId, barcodes, p.quantity, p.reference ?? "", p.supplierId ?? ""],
    queryFn: () => api<PendingResponse>("/stock/receive/pending", {
      query: { product_id: p.productId, branch_id: p.branchId || undefined, barcodes, quantity: p.quantity || undefined, reference: p.reference || undefined, supplier_id: p.supplierId || undefined },
    }),
    enabled: !!p.productId,
    placeholderData: (prev) => prev,
    staleTime: 0,
  });
}

/** Checks one scanned item label against waiting receipts (any branch). */
export async function pendingForLabel(productId: string, branchId: string, code: string): Promise<PendingReceipt | undefined> {
  const r = await api<PendingResponse>("/stock/receive/pending", { query: { product_id: productId, branch_id: branchId || undefined, barcodes: code } }).catch(() => null);
  return r?.items.find((i) => i.kind === "barcode");
}

export function PendingReceiptsNotice({ data }: { data: PendingResponse | undefined }) {
  const [closed, setClosed] = useState<string>("");
  const items = data?.items ?? [];
  const key = items.map((i) => `${i.id}:${i.blocking}`).join("|");
  const notify = useMutation({
    mutationFn: (id: string) => api(`/stock/receive/pending/${id}/notify`, { method: "POST" }),
    onSuccess: () => toast.success("The approvers were notified"),
    onError: (e) => toast.error(e),
  });
  if (!items.length || closed === key) return null;
  const blocking = items.some((i) => i.blocking);
  return (
    <div role="alert" data-testid="pending-receipts"
      className={cn("rounded-xl border p-3 text-sm", blocking ? "border-warning/40 bg-warning/10" : "border-primary/30 bg-primary/5")}>
      <div className="flex items-start gap-2">
        <Clock3 className={cn("mt-0.5 h-4 w-4 shrink-0", blocking ? "text-warning" : "text-primary")} />
        <div className="min-w-0 flex-1 space-y-1">
          <p className="font-semibold">{t(blocking ? PENDING_TITLE : "Waiting for approval at this branch")}</p>
          <p className="text-xs text-muted-foreground">
            {blocking
              ? t(data!.message)
              : t("Another delivery of this product is waiting for approval here. You can still receive a separate delivery — make sure it is not the same stock.")}
          </p>
        </div>
        <button type="button" aria-label={t("Close")} onClick={() => setClosed(key)} className="rounded-md p-1 text-muted-foreground hover:bg-muted"><X className="h-4 w-4" /></button>
      </div>
      <ul className="mt-2 space-y-2">
        {items.map((i) => (
          <li key={i.id} className="rounded-lg bg-card/80 p-2.5 text-xs">
            <div className="flex flex-wrap items-baseline justify-between gap-x-2">
              <span className="font-medium">{i.product_name} <span className="num text-muted-foreground">· {i.product_code}</span></span>
              <span className="num text-muted-foreground">{i.reference}</span>
            </div>
            <dl className="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-muted-foreground">
              {i.barcodes.length > 0 && <><dt>{t("Barcode")}</dt><dd className="num break-all text-foreground">{i.barcodes.join(", ")}</dd></>}
              <dt>{t("Branch")}</dt><dd className="text-foreground">{i.branch_name ?? "—"}</dd>
              <dt>{t("Pending quantity")}</dt><dd className="num text-foreground">{count(i.quantity)}</dd>
              {i.delivery_note && <><dt>{t("Delivery note")}</dt><dd className="text-foreground">{i.delivery_note}</dd></>}
              <dt>{t("Submitted by")}</dt><dd className="text-foreground">{i.submitted_by ?? "—"} · {dateTime(i.submitted_at)}</dd>
              <dt>{t("Status")}</dt><dd className="text-foreground">{t("Pending approval")}{i.levels && i.levels > 1 ? ` · ${t("level")} ${i.level} / ${i.levels}` : ""}</dd>
              {i.approvers && i.approvers.length > 0 && <><dt>{t("Approver")}</dt><dd className="text-foreground">{i.approvers.join(", ")}</dd></>}
            </dl>
            <div className="mt-2 flex flex-wrap gap-2">
              {i.may_view && <Button size="sm" variant="outline" asChild><Link to="/approvals"><ExternalLink /> {t("View pending request")}</Link></Button>}
              {data?.controls.notify_approvers && (
                <Button size="sm" variant="outline" disabled={notify.isPending} onClick={() => notify.mutate(i.id)}><BellRing /> {t("Notify approver")}</Button>
              )}
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
