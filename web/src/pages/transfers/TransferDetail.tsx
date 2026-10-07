import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, ArrowRight, Check, PackageCheck, Send, Truck, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { count, date, dateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { StatusBadge } from "@/components/Badges";
import { ConfirmDialog, Field } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Segments } from "@/components/Filters";
import { t as tr } from "@/lib/i18n";
import type { TransferRow } from "./TransfersList";

interface Detail {
  transfer: TransferRow;
  items: {
    id: string; product_id: string; product_name: string; product_code: string; quantity: number; barcode: string | null;
    short_qty: number; damaged_qty: number; received_qty: number;
  }[];
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
  const [receiving, setReceiving] = useState(false);
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["transfer", id], queryFn: () => api<Detail>(`/transfers/${id}`) });
  const act = useMutation({
    mutationFn: ({ action, body }: { action: string; body?: unknown }) => api<{ status: string }>(`/transfers/${id}/${action}`, { method: "POST", body }),
    onSuccess: (r) => {
      toast.success(`Transfer ${r.status.replace("_", " ")}`);
      setCancelling(false);
      setReceiving(false);
      qc.invalidateQueries({ queryKey: ["transfer", id] });
      qc.invalidateQueries({ queryKey: ["transfers"] });
      qc.invalidateQueries({ queryKey: ["stock"] });
    },
    onError: (e) => toast.error(e),
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
        <div className="surface card-body mb-5">
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
        <Section title={`Items · ${count(t.total_units)} ${tr("units")}`}>
          <ul className="divide-y">
            {data.items.map((i) => (
              <li key={i.id} className="flex items-center justify-between gap-3 py-2.5 text-sm">
                <div>
                  <div className="font-medium">{i.product_name}</div>
                  <div className="num text-xs text-muted-foreground">{i.product_code}{i.barcode && ` · ${i.barcode}`}</div>
                </div>
                <div className="text-end">
                  <span className="num font-semibold">{count(i.quantity)}</span>
                  {i.short_qty + i.damaged_qty > 0 && (
                    <div className="num text-xs text-warning">
                      {count(i.received_qty)} {tr("received")}
                      {i.short_qty > 0 && ` · ${count(i.short_qty)} ${tr("short")}`}
                      {i.damaged_qty > 0 && ` · ${count(i.damaged_qty)} ${tr("damaged")}`}
                    </div>
                  )}
                </div>
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
            {t.short_units + t.damaged_units > 0 && (
              <div className="mt-2 flex gap-2 rounded-lg bg-warning/10 p-3 text-sm text-warning">
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                <div>
                  <div className="font-medium">
                    {tr("Received with discrepancies")}: {count(t.short_units)} {tr("short")}, {count(t.damaged_units)} {tr("damaged")}
                  </div>
                  <div className="text-xs">{t.discrepancy_reason}</div>
                </div>
              </div>
            )}
          </Section>
          <div className="flex flex-col gap-2">
            {/* Lifecycle: Draft → Submit for approval → Awaiting approval → Approved → Dispatch → In transit → Receive → Received.
                The next step is shown as the action when this user may take it, otherwise as its waiting state. */}
            {data.can.submit && (
              <ActionButton online busy={act.isPending && act.variables?.action === "submit"} disabled={act.isPending} busyLabel="Submitting…"
                onAction={() => act.mutateAsync({ action: "submit" })}><Send /> {tr("Submit for approval")}</ActionButton>
            )}
            {t.status === "pending_approval" && <ActionButton variant="outline" blockedBy={[REASONS.awaitingApproval]}>{null}</ActionButton>}
            {data.can.dispatch && (
              <ActionButton online busy={act.isPending && act.variables?.action === "dispatch"} disabled={act.isPending} busyLabel="Dispatching…"
                onAction={() => act.mutateAsync({ action: "dispatch" })}><Truck /> {tr("Dispatch")}</ActionButton>
            )}
            {t.status === "approved" && !data.can.dispatch && <ActionButton variant="outline" blockedBy={["Awaiting dispatch"]}>{null}</ActionButton>}
            {data.can.receive && (
              <ActionButton variant="success" online disabled={act.isPending} onAction={() => setReceiving(true)}><PackageCheck /> {tr("Confirm receipt")}</ActionButton>
            )}
            {t.status === "dispatched" && !data.can.receive && <ActionButton variant="outline" blockedBy={["In transit"]}>{null}</ActionButton>}
            {data.can.cancel && <Button variant="outline" className="text-destructive" onClick={() => setCancelling(true)}><X /> {tr("Cancel transfer")}</Button>}
            {t.status === "dispatched" && !data.can.receive && <p className="text-center text-sm text-muted-foreground">{tr("Waiting for")} {t.to_branch_name} to confirm receipt.</p>}
          </div>
        </div>
      </div>
      <ReceiveDialog
        open={receiving}
        onOpenChange={setReceiving}
        items={data.items}
        busy={act.isPending}
        onConfirm={(body) => act.mutateAsync({ action: "receive", body })}
      />
      <ConfirmDialog
        open={cancelling}
        onOpenChange={setCancelling}
        title="Cancel this transfer?"
        description="No stock has moved yet, so nothing needs reversing."
        destructive
        requireReason
        confirmLabel="Cancel transfer"
        busy={act.isPending}
        onConfirm={(reason) => act.mutateAsync({ action: "cancel", body: { reason } })}
      />
    </>
  );
}

type Line = { short: number; damaged: number };

/** Receipt check: what arrived, what is short and what is damaged — only good units become sellable. */
function ReceiveDialog({ open, onOpenChange, items, busy, onConfirm }: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  items: Detail["items"];
  busy: boolean;
  onConfirm: (body: { lines: { id: string; short: number; damaged: number }[]; reason: string }) => void;
}) {
  const [lines, setLines] = useState<Record<string, Line>>({});
  const [reason, setReason] = useState("");
  const get = (id: string) => lines[id] ?? { short: 0, damaged: 0 };
  const set = (id: string, patch: Partial<Line>, max: number) => {
    const next = { ...get(id), ...patch };
    next.short = Math.max(0, Math.min(next.short, max));
    next.damaged = Math.max(0, Math.min(next.damaged, max - next.short));
    setLines({ ...lines, [id]: next });
  };
  const sent = items.reduce((a, i) => a + i.quantity, 0);
  const short = items.reduce((a, i) => a + get(i.id).short, 0);
  const damaged = items.reduce((a, i) => a + get(i.id).damaged, 0);
  const off = short + damaged > 0;
  const valid = !off || reason.trim().length >= 3;
  const num = (v: string) => Number(v.replace(/\D/g, "")) || 0;

  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={(o) => {
        if (!o) { setLines({}); setReason(""); }
        onOpenChange(o);
      }}
      title="Confirm receipt"
      description="Check what arrived. Short or damaged units are recorded against this transfer and are not added to sellable stock."
      footer={
        <Button
          variant="success"
          className="w-full md:w-auto"
          disabled={!valid || busy}
          onClick={() => onConfirm({ lines: items.map((i) => ({ id: i.id, ...get(i.id) })).filter((l) => l.short + l.damaged > 0), reason: reason.trim() })}
        >
          <PackageCheck /> {off ? tr("Receive with discrepancies") : tr("Everything arrived")}
        </Button>
      }
    >
      <ul className="divide-y">
        {items.map((i) => {
          const l = get(i.id);
          return (
            <li key={i.id} className="space-y-2 py-3">
              <div className="flex items-start justify-between gap-3 text-sm">
                <div className="min-w-0">
                  <div className="truncate font-medium">{i.product_name}</div>
                  <div className="num truncate text-xs text-muted-foreground">{i.product_code}{i.barcode && ` · ${i.barcode}`}</div>
                </div>
                <span className="num shrink-0 text-xs text-muted-foreground">{tr("Sent")} {count(i.quantity)}</span>
              </div>
              {i.barcode ? (
                <Segments
                  value={l.short ? "short" : l.damaged ? "damaged" : "ok"}
                  onChange={(v) => set(i.id, { short: v === "short" ? 1 : 0, damaged: v === "damaged" ? 1 : 0 }, 1)}
                  options={[{ value: "ok", label: "Received" }, { value: "short", label: "Missing" }, { value: "damaged", label: "Damaged" }]}
                />
              ) : (
                <div className="grid grid-cols-3 gap-2">
                  <Field label="Received"><Input readOnly className="num bg-muted" value={count(i.quantity - l.short - l.damaged)} /></Field>
                  <Field label="Short"><Input inputMode="numeric" className="num" value={l.short || ""} placeholder="0" onChange={(e) => set(i.id, { short: num(e.target.value) }, i.quantity)} /></Field>
                  <Field label="Damaged"><Input inputMode="numeric" className="num" value={l.damaged || ""} placeholder="0" onChange={(e) => set(i.id, { damaged: num(e.target.value) }, i.quantity)} /></Field>
                </div>
              )}
            </li>
          );
        })}
      </ul>
      <div className={cn("mt-2 rounded-lg p-3 text-sm", off ? "bg-warning/10 text-warning" : "bg-success/10 text-success")}>
        <span className="num font-medium">{count(sent - short - damaged)}</span> {tr("of")} <span className="num">{count(sent)}</span> {tr("units go into stock")}
        {off && <span className="num"> · {count(short)} {tr("short")} · {count(damaged)} {tr("damaged")}</span>}
      </div>
      {off && (
        <Field label="What happened?" className="mt-3" hint="Required — saved with the transfer, the stock record and the audit trail">
          <Textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={tr("e.g. 1 bottle broken in the box, 2 not in the carton")} />
        </Field>
      )}
    </ResponsiveDialog>
  );
}
