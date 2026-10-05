import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, Check, PackageCheck, Send, Truck, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, errorMessage } from "@/lib/api";
import { count, date, dateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { StatusBadge } from "@/components/Badges";
import { ConfirmDialog } from "@/components/Form";
import type { TransferRow } from "./TransfersList";

interface Detail {
  transfer: TransferRow;
  items: { id: string; product_id: string; product_name: string; product_code: string; quantity: number; barcode: string | null }[];
  can: { submit: boolean; dispatch: boolean; receive: boolean; cancel: boolean };
}

const STEPS = [
  ["draft", "Draft"],
  ["pending_approval", "Approval"],
  ["approved", "Approved"],
  ["dispatched", "In transit"],
  ["received", "Received"],
] as const;

export default function TransferDetail() {
  const { id } = useParams();
  const qc = useQueryClient();
  const [cancelling, setCancelling] = useState(false);
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["transfer", id], queryFn: () => api<Detail>(`/transfers/${id}`) });
  const act = useMutation({
    mutationFn: ({ action, body }: { action: string; body?: unknown }) => api<{ status: string }>(`/transfers/${id}/${action}`, { method: "POST", body }),
    onSuccess: (r) => {
      toast.success(`Transfer ${r.status.replace("_", " ")}`);
      setCancelling(false);
      qc.invalidateQueries({ queryKey: ["transfer", id] });
      qc.invalidateQueries({ queryKey: ["transfers"] });
      qc.invalidateQueries({ queryKey: ["stock"] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  if (error) return <ErrorState error={error} retry={refetch} />;
  if (isLoading || !data) return <Loading />;
  const t = data.transfer;
  const idx = STEPS.findIndex(([s]) => s === t.status);
  const terminal = ["cancelled", "rejected"].includes(t.status);

  return (
    <>
      <PageHeader
        back="/transfers"
        eyebrow={`Created ${dateTime(t.created_at)}`}
        title={<span className="num">{t.transfer_no}</span>}
        actions={<StatusBadge status={t.status === "dispatched" ? "in_transit" : t.status} />}
      />
      {!terminal && (
        <div className="surface mb-5 p-5">
          <ol className="grid grid-cols-5 gap-1">
            {STEPS.map(([s, label], i) => (
              <li key={s} className="flex flex-col items-center gap-1.5 text-center">
                <span className={cn("flex h-8 w-8 items-center justify-center rounded-full border-2 text-xs", i <= idx ? "border-success bg-success text-success-foreground" : "text-muted-foreground")}>
                  {i <= idx ? <Check className="h-4 w-4" /> : i + 1}
                </span>
                <span className={cn("text-[11px] leading-tight", i <= idx ? "font-medium" : "text-muted-foreground")}>{label}</span>
              </li>
            ))}
          </ol>
        </div>
      )}
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_360px]">
        <Section title={`Items · ${count(t.total_units)} units`}>
          <ul className="divide-y">
            {data.items.map((i) => (
              <li key={i.id} className="flex items-center justify-between gap-3 py-2.5 text-sm">
                <div>
                  <div className="font-medium">{i.product_name}</div>
                  <div className="num text-xs text-muted-foreground">{i.product_code}{i.barcode && ` · ${i.barcode}`}</div>
                </div>
                <span className="num font-semibold">{count(i.quantity)}</span>
              </li>
            ))}
          </ul>
        </Section>
        <div className="space-y-5">
          <Section title="Route">
            <div className="mb-3 flex items-center gap-2 font-medium">{t.from_branch_name} <ArrowRight className="h-4 w-4 text-muted-foreground" /> {t.to_branch_name}</div>
            <KV label="Transfer date">{date(t.transfer_date)}</KV>
            <KV label="Created by">{t.created_by_name ?? "—"}</KV>
            {t.approved_by_name && <KV label="Approved by">{t.approved_by_name}</KV>}
            {t.dispatched_at && <KV label="Dispatched">{t.dispatched_by_name} · {dateTime(t.dispatched_at)}</KV>}
            {t.received_at && <KV label="Received">{t.received_by_name} · {dateTime(t.received_at)}</KV>}
            {t.notes && <p className="mt-2 rounded-lg bg-muted p-3 text-sm">{t.notes}</p>}
          </Section>
          <div className="flex flex-col gap-2">
            {data.can.submit && <Button onClick={() => act.mutate({ action: "submit" })} disabled={act.isPending}><Send /> Submit</Button>}
            {data.can.dispatch && <Button onClick={() => act.mutate({ action: "dispatch" })} disabled={act.isPending}><Truck /> Dispatch</Button>}
            {data.can.receive && <Button variant="success" onClick={() => act.mutate({ action: "receive" })} disabled={act.isPending}><PackageCheck /> Confirm receipt</Button>}
            {data.can.cancel && <Button variant="outline" className="text-destructive" onClick={() => setCancelling(true)}><X /> Cancel transfer</Button>}
            {t.status === "dispatched" && !data.can.receive && <p className="text-center text-sm text-muted-foreground">Waiting for {t.to_branch_name} to confirm receipt.</p>}
          </div>
        </div>
      </div>
      <ConfirmDialog
        open={cancelling}
        onOpenChange={setCancelling}
        title="Cancel this transfer?"
        description="No stock has moved yet, so nothing needs reversing."
        destructive
        requireReason
        confirmLabel="Cancel transfer"
        busy={act.isPending}
        onConfirm={(reason) => act.mutate({ action: "cancel", body: { reason } })}
      />
    </>
  );
}
