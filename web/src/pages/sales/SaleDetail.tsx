import { useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, RefreshCcw, RotateCcw, ScanLine } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { amount, dateTime, money, toNum } from "@/lib/format";
import type { Outcome, SaleDetail as Detail } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { Pill, StatusBadge } from "@/components/Badges";
import { ConfirmDialog, Field, Select, ToggleRow } from "@/components/Form";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";
import { t } from "@/lib/i18n";
import { ReceiptPanel } from "@/components/ReceiptPanel";
import { OwnerPicker, type Owner } from "./SaleOwner";
import { Textarea } from "@/components/ui/textarea";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ActionButton } from "@/components/ActionButton";
import { UserRoundCog } from "lucide-react";

export default function SaleDetail() {
  const { id } = useParams();
  const { can, profile } = useSession();
  const qc = useQueryClient();
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["sale", id], queryFn: () => api<Detail>(`/sales/${id}`) });
  const [returning, setReturning] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [qty, setQty] = useState<Record<string, number>>({});
  const [scanReturn, setScanReturn] = useState(false);
  const qtyRef = useRef(qty);
  qtyRef.current = qty;
  const [restock, setRestock] = useState(true);
  const [refundMethod, setRefundMethod] = useState("");

  const done = (r: Outcome<unknown>, msg: string) => {
    toast.success(r.pending_approval ? "Sent for approval" : msg);
    setReturning(false);
    setCancelling(false);
    setQty({});
    qc.invalidateQueries({ queryKey: ["sale", id] });
    qc.invalidateQueries({ queryKey: ["sales"] });
    // The updated (adjustment) receipt appears straight away (roadmap 67).
    qc.invalidateQueries({ queryKey: ["receipts", id] });
  };
  const ret = useMutation({
    mutationFn: (reason: string) =>
      api<Outcome<unknown>>(`/sales/${id}/return`, {
        body: { reason, restock, refund_method: refundMethod, items: Object.entries(qty).filter(([, q]) => q > 0).map(([sale_item_id, quantity]) => ({ sale_item_id, quantity })) },
      }),
    onSuccess: (r) => done(r, "Return processed"),
    onError: (e) => toast.error(e),
  });
  const cancel = useMutation({
    mutationFn: (reason: string) => api<Outcome<unknown>>(`/sales/${id}/cancel`, { body: { reason, refund_method: refundMethod } }),
    onSuccess: (r) => done(r, "Sale cancelled"),
    onError: (e) => toast.error(e),
  });

  if (error) return <ErrorState error={error} retry={refetch} />;
  if (isLoading || !data) return <Loading />;
  const s = data.sale;
  const reversible = !s.is_legacy && !["cancelled", "returned"].includes(s.status) && !data.pending_approval_id;
  const returnable = data.items.filter((i) => i.quantity > i.returned_qty);
  // Scanning a returned item ticks it: its own barcode for tracked items, otherwise the product's barcode.
  const onReturnScan = async (code: string): Promise<ScanOutcome> => {
    let item = returnable.find((i) => i.barcode === code);
    if (!item) {
      try {
        const r = await api<{ product: { id: string } }>("/products/lookup", { query: { code } });
        item = returnable.find((i) => i.product_id === r.product.id && (qtyRef.current[i.id] ?? 0) < i.quantity - i.returned_qty) ?? returnable.find((i) => i.product_id === r.product.id);
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 404)) return { tone: "error", title: errorMessage(e) };
      }
    }
    if (!item) return { tone: "error", title: "Not part of this sale", detail: "Only items sold on this receipt can be returned here." };
    const max = item.quantity - item.returned_qty;
    const current = qtyRef.current[item.id] ?? 0;
    if (current >= max) return { tone: "info", title: `All returnable ${item.product_name} already selected` };
    setQty((q) => ({ ...q, [item.id]: current + 1 }));
    return { tone: "success", title: `${item.product_name} · returning ${current + 1}` };
  };

  return (
    <>
      <PageHeader
        back="/sales"
        eyebrow={dateTime(s.created_at)}
        title={<span className="num">{s.receipt_no}</span>}
        actions={
          <div className="flex flex-wrap gap-2 no-print">

          </div>
        }
      />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        {/* Receipt: the stored 50 mm receipt(s), with every sharing channel (roadmap 65–67) */}
        <div className="surface p-3 sm:p-4">
          <ReceiptPanel saleId={s.id} />
        </div>

        {/* Side panel */}
        <div className="space-y-5 no-print">
          <div className="flex flex-wrap items-center gap-2"><StatusBadge status={s.status} />{s.is_legacy && <Pill>{t("Imported")}</Pill>}<span className="text-xs text-muted-foreground">{s.branch_name}</span></div>
          {data.pending_approval_id && <div className="rounded-xl bg-warning/10 p-4 text-sm text-warning">{t("A return, exchange or cancellation for this sale is awaiting approval. Stock, loyalty points and the receipt change only once it is approved.")}</div>}
          <OwnershipPanel data={data} onChanged={() => { qc.invalidateQueries({ queryKey: ["sale", id] }); qc.invalidateQueries({ queryKey: ["sales"] }); }} />
          {s.order_no && <Section title="Order"><Link to={`/orders/${s.order_id}`} className="num text-primary">{s.order_no}</Link></Section>}
          {s.approved_by_name && <Section title="Discount approval"><p className="text-sm">{t("Approved by")} {s.approved_by_name}</p></Section>}
          {data.credit && (
            <Section title="Credit" action={<Link to={`/credit/${data.credit.id}`} className="text-xs text-primary">{t("Open")}</Link>}>
              <KV label="Due">{data.credit.due_date}</KV>
              <KV label="Status"><StatusBadge status={data.credit.status} /></KV>
            </Section>
          )}
          {data.returns.length > 0 && (
            <Section title="Returns & reversals">
              <ReconciliationSummary data={data} />
              <ul className="space-y-3 text-sm">
                {data.returns.map((r) => (
                  <li key={r.id}>
                    <div className="flex justify-between font-medium"><span className="num">{r.return_no}</span><span className="num">{money(r.refund_amount)}</span></div>
                    <div className="text-xs text-muted-foreground">{r.kind} · {r.reason} · {r.user_name} · {dateTime(r.created_at)}{r.points_reversed > 0 && ` · −${r.points_reversed} pts`}</div>
                  </li>
                ))}
              </ul>
            </Section>
          )}
          {reversible && (can("sales.return") || can("sales.cancel")) && (
            <Section title="Corrections">
              <div className="flex flex-col gap-2">
                {can("sales.return") && returnable.length > 0 && <Button variant="outline" onClick={() => setReturning(true)}><RotateCcw /> {t("Return items / refund")}</Button>}
                {can("sales.return") && can("sales.create") && returnable.length > 0 && s.payment_method !== "credit" && (
                  <Button variant="outline" asChild><Link to={`/sales/${id}/exchange`}><RefreshCcw /> {t("Exchange items")}</Link></Button>
                )}
                {can("sales.cancel") && s.status === "completed" && <Button variant="outline" className="text-destructive" onClick={() => setCancelling(true)}><Ban /> {t("Cancel sale")}</Button>}
              </div>
              <p className="mt-2 text-xs text-muted-foreground">{t("Stock, loyalty points, customer totals and credit are reversed automatically.")}</p>
            </Section>
          )}
        </div>
      </div>

      <ConfirmDialog
        open={returning}
        onOpenChange={setReturning}
        title="Return items"
        description="Choose what is coming back."
        confirmLabel="Process return"
        requireReason
        busy={ret.isPending}
        onConfirm={(reason) => ret.mutateAsync(reason)}
      >
        <Button type="button" variant="outline" size="sm" className="w-full" onClick={() => setScanReturn(true)}><ScanLine /> {t("Scan returned items")}</Button>
        <ul className="divide-y rounded-xl border">
          {returnable.map((i) => (
            <li key={i.id} className="flex items-center gap-3 p-3">
              <div className="min-w-0 flex-1 text-sm">
                <div className="truncate font-medium">{i.product_name}</div>
                <div className="num text-xs text-muted-foreground">{i.quantity - i.returned_qty} returnable · {amount(i.unit_price)} each</div>
              </div>
              <Input
                inputMode="numeric"
                className="num w-20 text-center"
                placeholder="0"
                value={qty[i.id] ?? ""}
                onChange={(e) => setQty({ ...qty, [i.id]: Math.min(i.quantity - i.returned_qty, Math.max(0, parseInt(e.target.value) || 0)) })}
              />
            </li>
          ))}
        </ul>
        <ToggleRow label="Return to stock" hint="Turn off for damaged goods that cannot be resold" checked={restock} onChange={setRestock} />
        <RefundMethod value={refundMethod} onChange={setRefundMethod} methods={profile?.settings.sales.payment_methods.filter((m) => m.key !== "credit") ?? []} />
      </ConfirmDialog>
      <BarcodeScanner open={scanReturn} onOpenChange={setScanReturn} onDetected={onReturnScan} continuous title="Scan returned items" />
      <ConfirmDialog
        open={cancelling}
        onOpenChange={setCancelling}
        title="Cancel this sale?"
        description="All items return to stock, points and customer totals are reversed and any payment is refunded."
        confirmLabel="Cancel sale"
        destructive
        requireReason
        busy={cancel.isPending}
        onConfirm={(reason) => cancel.mutateAsync(reason)}
      >
        <RefundMethod value={refundMethod} onChange={setRefundMethod} methods={profile?.settings.sales.payment_methods.filter((m) => m.key !== "credit") ?? []} />
      </ConfirmDialog>
    </>
  );
}

function RefundMethod({ value, onChange, methods }: { value: string; onChange: (v: string) => void; methods: { key: string; label: string }[] }) {
  return (
    <Field label="Refund via" hint="Credit sales reduce the outstanding balance first">
      <Select value={value} onChange={onChange}>
        <option value="">{t("Same as original payment")}</option>
        {methods.map((m) => <option key={m.key} value={m.key}>{m.label}</option>)}
      </Select>
    </Field>
  );
}

/** Sale Owner (credited) vs Recorded By, the ownership history, and Change Sale Owner (sent for approval — roadmap 63). */
function OwnershipPanel({ data, onChanged }: { data: Detail; onChanged: () => void }) {
  const { can } = useSession();
  const s = data.sale;
  const [open, setOpen] = useState(false);
  const [picker, setPicker] = useState(false);
  const [next, setNext] = useState<Owner | null>(null);
  const [reason, setReason] = useState("");
  const history = data.owner_changes ?? [];
  const pending = history.find((h) => h.status === "pending");
  const request = useMutation({
    mutationFn: () => api<Outcome<unknown>>(`/sales/${s.id}/owner-change`, { body: { new_owner_id: next!.id, reason } }),
    onSuccess: (r) => {
      toast.success(r.pending_approval ? "Ownership change sent for approval" : "Sale owner changed");
      setOpen(false);
      setNext(null);
      setReason("");
      onChanged();
    },
    onError: (e) => toast.error(e),
  });
  const statusTone: Record<string, string> = { approved: "text-success", rejected: "text-destructive", pending: "text-warning", withdrawn: "text-muted-foreground" };
  return (
    <Section title="Sale ownership">
      <KV label="Sale owner">{s.user_name ?? "—"}</KV>
      <KV label="Recorded by">{s.recorded_by_name ?? "—"}</KV>
      {pending && <p className="mt-2 rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t("Change to")} {pending.to_owner} {t("awaiting approval")} — {pending.reason}</p>}
      {can("sales.request_owner_change") && !pending && s.status !== "cancelled" && !s.is_legacy && (
        <Button variant="outline" size="sm" className="mt-2 w-full" onClick={() => setOpen(true)}><UserRoundCog /> {t("Change Sale Owner")}</Button>
      )}
      {history.length > 0 && (
        <ul className="mt-3 space-y-2 border-t pt-3 text-xs">
          {history.map((h) => (
            <li key={h.id}>
              <span className="font-medium">{h.from_owner ?? "—"} → {h.to_owner ?? "—"}</span>{" "}
              <span className={statusTone[h.status] ?? ""}>· {t(h.status)}</span>
              <div className="text-muted-foreground">{h.reason} · {t("requested by")} {h.requested_by ?? "—"} · {dateTime(h.created_at)}{h.decided_by && ` · ${t(h.status)} ${t("by")} ${h.decided_by}`}</div>
            </li>
          ))}
        </ul>
      )}
      <ResponsiveDialog open={open} onOpenChange={setOpen} title="Change Sale Owner" description="The change is applied only after approval. The sale, payments, stock and receipt stay as they are."
        footer={<ActionButton online busy={request.isPending} busyLabel="Submitting…" blockedBy={[!next && "Choose the new owner", reason.trim().length < 3 && "Give the reason"]} onAction={() => request.mutateAsync()}>Submit for approval</ActionButton>}>
        <div className="space-y-3">
          <KV label="Receipt"><span className="num">{s.receipt_no}</span></KV>
          <KV label="Current sale owner">{s.user_name ?? "—"}</KV>
          <Field label="New sale owner">
            <Button type="button" variant="outline" className="w-full justify-start" onClick={() => setPicker(true)}>{next?.name ?? t("Choose…")}</Button>
          </Field>
          <Field label="Reason for change"><Textarea value={reason} onChange={(e) => setReason(e.target.value)} maxLength={500} /></Field>
        </div>
      </ResponsiveDialog>
      <OwnerPicker open={picker} onOpenChange={setPicker} branchId={s.branch_id} value={next?.id ?? null} exclude={s.owner_id} title="New sale owner"
        onPick={(o) => { setNext(o); setPicker(false); }} />
    </Section>
  );
}

/** Original vs adjusted position of a sale after returns / exchanges (roadmap 67). Only applicable lines are shown. */
function ReconciliationSummary({ data }: { data: Detail }) {
  const s = data.sale;
  const refunded = data.returns.reduce((a, r) => a + toNum(r.refund_amount), 0);
  const exchanged = data.returns.some((r) => r.refund_method === "exchange");
  const reversed = data.returns.reduce((a, r) => a + (r.points_reversed ?? 0), 0);
  const unrecovered = data.returns.reduce((a, r) => a + (r.points_unrecovered ?? 0), 0);
  const refundPaid = data.payments.filter((p) => toNum(p.amount) < 0).reduce((a, p) => a - toNum(p.amount), 0);
  return (
    <div className="mb-3 space-y-0.5 rounded-lg bg-muted/50 p-2.5 text-sm">
      <KV label="Original sale total" className="py-0.5"><span className="num">{money(s.total)}</span></KV>
      <KV label={exchanged ? "Returned value (exchange)" : "Returned value"} className="py-0.5"><span className="num">−{money(refunded)}</span></KV>
      {refundPaid > 0 && <KV label="Amount refunded" className="py-0.5"><span className="num">{money(refundPaid)}</span></KV>}
      <KV label="Net sale value" className="py-0.5 font-semibold"><span className="num">{money(Math.max(0, toNum(s.total) - refunded))}</span></KV>
      {s.points_earned > 0 && (
        <>
          <KV label="Original points" className="py-0.5"><span className="num">{s.points_earned}</span></KV>
          {reversed > 0 && <KV label="Points reversed" className="py-0.5"><span className="num">−{reversed}</span></KV>}
          {unrecovered > 0 && <KV label="Points not recoverable (already redeemed)" className="py-0.5"><span className="num text-warning">{unrecovered}</span></KV>}
          <KV label="Net loyalty points" className="py-0.5 font-semibold"><span className="num">{Math.max(0, s.points_earned - reversed)}</span></KV>
        </>
      )}
      <p className="pt-1 text-xs text-muted-foreground">{t("Amount paid stays as originally paid; refunds are listed separately.")}</p>
    </div>
  );
}
