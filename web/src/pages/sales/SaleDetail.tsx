import { useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, Download, MessageCircle, Printer, RefreshCcw, RotateCcw, ScanLine } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { amount, count, dateTime, methodLabel, money, phone, toNum } from "@/lib/format";
import { receiptPdf } from "@/lib/pdf";
import type { Outcome, SaleDetail as Detail } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { Pill, StatusBadge } from "@/components/Badges";
import { ConfirmDialog, Field, Select, ToggleRow } from "@/components/Form";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";
import { t } from "@/lib/i18n";

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
  const share = async () => {
    try {
      const r = await api<{ sent: boolean; link: string | null }>(`/sales/${id}/share`, { method: "POST" });
      if (r.sent) toast.success("Receipt sent on WhatsApp");
      else if (r.link) window.open(r.link, "_blank");
      else toast.info("This sale has no customer mobile");
    } catch (e) {
      toast.error(e);
    }
  };

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
            {can("sales.print") && (
              <>
                <Button variant="outline" onClick={() => window.print()}><Printer /> {t("Print")}</Button>
                <Button variant="outline" onClick={() => receiptPdf(data)}><Download /> {t("PDF")}</Button>
                <Button variant="outline" onClick={share}><MessageCircle /> {t("Share")}</Button>
              </>
            )}
          </div>
        }
      />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        {/* Receipt */}
        <div className="surface mx-auto w-full max-w-xl p-6 font-sans print:max-w-none print:border-0 print:shadow-none lg:mx-0">
          <div className="mb-4 text-center">
            {data.business.logo_url && <img src={data.business.logo_url} alt="" className="mx-auto mb-2 h-14 w-14 rounded-xl object-cover" />}
            <h2 className="text-lg font-semibold">{data.business.name}</h2>
            <p className="text-sm text-muted-foreground">{s.branch_name}{s.branch_location && ` · ${s.branch_location}`}</p>
            {(s.branch_phone || data.business.phone) && <p className="text-sm text-muted-foreground">{s.branch_phone || data.business.phone}</p>}
            <div className="mt-2 flex justify-center gap-2"><StatusBadge status={s.status} />{s.is_legacy && <Pill>{t("Imported")}</Pill>}</div>
          </div>
          {s.customer && (
            <p className="mb-3 text-center text-sm">
              <Link to={`/customers/${s.customer.id}`} className="font-medium hover:underline">{s.customer.name}</Link> · <span className="num">{phone(s.customer.mobile)}</span>
            </p>
          )}
          <table className="w-full text-sm">
            <thead className="border-b text-xs text-muted-foreground">
              <tr><th className="py-2 text-start font-medium">{t("Item")}</th><th className="text-end font-medium">{t("Qty")}</th><th className="text-end font-medium">{t("Price")}</th><th className="text-end font-medium">{t("Total")}</th></tr>
            </thead>
            <tbody className="divide-y">
              {data.items.map((i) => (
                <tr key={i.id}>
                  <td className="py-2 pe-2">
                    <div className="font-medium">{i.product_name}</div>
                    <div className="num text-xs text-muted-foreground">
                      {i.barcode && `${i.barcode} · `}marked {amount(i.marked_price)}
                      {toNum(i.discount) > 0 && <span className="text-destructive"> · −{amount(i.discount)}</span>}
                      {i.returned_qty > 0 && <span className="text-warning"> · {i.returned_qty} returned</span>}
                    </div>
                  </td>
                  <td className="num text-end align-top py-2">{count(i.quantity)}</td>
                  <td className="num text-end align-top py-2">{amount(i.unit_price)}</td>
                  <td className="num text-end align-top py-2 font-medium">{amount(i.line_total)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="mt-3 space-y-1 border-t pt-3 text-sm">
            <KV label="Marked total" className="py-0.5"><span className="num">{money(s.gross_total)}</span></KV>
            {toNum(s.discount_total) !== 0 && <KV label="Discount" className="py-0.5"><span className="num text-destructive">−{money(s.discount_total)}</span></KV>}
            {toNum(s.redeemed_value) > 0 && <KV label={`Points redeemed (${s.redeemed_points})`} className="py-0.5"><span className="num">−{money(s.redeemed_value)}</span></KV>}
            <div className="flex justify-between border-t pt-2 text-base font-semibold"><span>{t("Total")}</span><span className="num">{money(s.total)}</span></div>
            <KV label="Payment" className="py-0.5">{methodLabel(s.payment_method)}</KV>
            <KV label="Amount paid" className="py-0.5"><span className="num">{money(s.amount_paid)}</span></KV>
            {data.credit && <KV label="Balance" className="py-0.5"><span className="num text-destructive">{money(data.credit.balance)}</span></KV>}
            {data.payments.filter((p) => p.reference).map((p) => <KV key={p.id} label={toNum(p.amount) < 0 ? "Refund ref" : "Reference"} className="py-0.5"><span className="num">{p.reference}</span></KV>)}
            {s.points_earned > 0 && <KV label="Loyalty points earned" className="py-0.5"><span className="text-points">🌼 +{s.points_earned}</span></KV>}
            <KV label="Served by" className="py-0.5">{s.user_name ?? "—"}</KV>
          </div>
          {data.business.receipt_footer && <p className="mt-4 text-center text-sm text-muted-foreground">{data.business.receipt_footer}</p>}
        </div>

        {/* Side panel */}
        <div className="space-y-5 no-print">
          {data.pending_approval_id && <div className="rounded-xl bg-warning/10 p-4 text-sm text-warning">{t("A return or cancellation for this sale is awaiting approval.")}</div>}
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
        onConfirm={(reason) => ret.mutate(reason)}
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
        onConfirm={(reason) => cancel.mutate(reason)}
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
