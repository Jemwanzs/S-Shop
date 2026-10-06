import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, Banknote, MessageCircle, RotateCcw } from "lucide-react";
import { RecallDialog, type SoldItem } from "./RecallDialog";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { date, dateTime, methodLabel, money, phone, titleCase, toNum } from "@/lib/format";
import type { CreditRow, Money, Outcome } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { StatusBadge } from "@/components/Badges";
import { ConfirmDialog, Field } from "@/components/Form";
import { Chip } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { t } from "@/lib/i18n";

interface Detail {
  credit: CreditRow;
  payments: { id: string; method: string; amount: Money; reference: string; created_at: string; user_name: string | null }[];
  history: { action: string; created_at: string; user_name: string | null; comments: string }[];
  items: SoldItem[];
  can_recall: boolean;
  recalls: {
    return_no: string; reason: string; amount: Money; balance_before: Money | null; balance_after: Money | null; customer_credit: Money;
    refund_method: string; created_at: string; user_name: string | null; items: { product: string; quantity: number; barcode: string | null }[];
  }[];
}

export default function CreditDetail() {
  const { id } = useParams();
  const { can, profile } = useSession();
  const qc = useQueryClient();
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["credit", id], queryFn: () => api<Detail>(`/credit/${id}`) });
  const [paying, setPaying] = useState(false);
  const [writing, setWriting] = useState(false);
  const [recalling, setRecalling] = useState(false);
  const [amountStr, setAmount] = useState("");
  const [method, setMethod] = useState("cash");
  const [reference, setReference] = useState("");

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["credit"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
    qc.invalidateQueries({ queryKey: ["stock"] });
  };
  const pay = useMutation({
    mutationFn: () => api<{ balance: number; status: string }>(`/credit/${id}/payments`, { body: { amount: toNum(amountStr), method, reference } }),
    onSuccess: (r) => {
      toast.success(r.status === "paid" ? "Fully paid 🎉" : `Payment recorded · balance ${money(r.balance)}`);
      setPaying(false);
      setAmount("");
      setReference("");
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const writeOff = useMutation({
    mutationFn: (reason: string) => api<Outcome<unknown>>(`/credit/${id}/write-off`, { body: { reason } }),
    onSuccess: (r) => {
      toast.success(r.pending_approval ? "Write-off sent for approval" : "Credit written off");
      setWriting(false);
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const remind = async () => {
    try {
      const r = await api<{ sent: boolean; link: string }>(`/credit/${id}/remind`, { method: "POST" });
      if (r.sent) toast.success("Reminder sent on WhatsApp");
      else window.open(r.link, "_blank");
    } catch (e) {
      toast.error(e);
    }
  };

  if (error) return <ErrorState error={error} retry={refetch} />;
  if (isLoading || !data) return <Loading />;
  const c = data.credit;
  const open = ["outstanding", "partially_paid", "overdue"].includes(c.status);
  const methods = profile?.settings.sales.payment_methods.filter((m) => m.enabled && m.key !== "credit") ?? [];
  const paidPct = Math.min(100, (toNum(c.amount_paid) / Math.max(toNum(c.original_amount) - toNum(c.adjustments), 1)) * 100);

  return (
    <>
      <PageHeader
        back="/credit"
        eyebrow="Credit sale"
        title={c.customer_name}
        description={<span className="num">{phone(c.customer_mobile)}</span>}
        actions={
          open && (
            <>
              <Button variant="outline" onClick={remind}><MessageCircle /> {t("Remind")}</Button>
              {can("credit.collect") && <Button onClick={() => setPaying(true)}><Banknote /> {t("Record payment")}</Button>}
            </>
          )
        }
      />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_360px]">
        <div className="space-y-5">
          <div className="surface card-body">
            <div className="flex items-end justify-between gap-4">
              <div>
                <p className="label-caps">{t("Outstanding balance")}</p>
                <p className="num mt-1 text-3xl font-bold">{money(c.balance)}</p>
              </div>
              <div className="flex flex-col items-end gap-1">
                <StatusBadge status={c.status} />
                {c.recall_state && <StatusBadge status={c.recall_state} />}
              </div>
            </div>
            <div className="mt-4 h-2.5 overflow-hidden rounded-full bg-muted">
              <div className="h-full rounded-full bg-success transition-all" style={{ width: `${paidPct}%` }} />
            </div>
            <p className="num mt-1.5 text-xs text-muted-foreground">{money(c.amount_paid)} paid of {money(toNum(c.original_amount) - toNum(c.adjustments))}</p>
          </div>
          <Section title="Payment history">
            {data.payments.length === 0 ? (
              <p className="py-4 text-sm text-muted-foreground">{t("No repayments yet.")}</p>
            ) : (
              <ul className="divide-y">
                {data.payments.map((p) => (
                  <li key={p.id} className="flex items-center justify-between gap-3 py-3 text-sm">
                    <div>
                      <div className="font-medium">{methodLabel(p.method)}{p.reference && <span className="num text-muted-foreground"> · {p.reference}</span>}</div>
                      <div className="text-xs text-muted-foreground">{dateTime(p.created_at)} · {p.user_name}</div>
                    </div>
                    <span className="num font-semibold text-success">{money(p.amount)}</span>
                  </li>
                ))}
              </ul>
            )}
          </Section>
          {data.recalls.length > 0 && (
            <Section title="Recalls">
              <ul className="divide-y">
                {data.recalls.map((r) => (
                  <li key={r.return_no} className="space-y-1 py-3 text-sm">
                    <div className="flex items-center justify-between gap-3">
                      <span className="num font-medium">{r.return_no}</span>
                      <span className="num font-semibold">{money(r.amount)}</span>
                    </div>
                    <div className="text-xs text-muted-foreground">{dateTime(r.created_at)} · {r.user_name} — {r.reason}</div>
                    <div className="text-xs">
                      {r.items.map((it) => `${it.quantity} × ${it.product}${it.barcode ? ` (${it.barcode})` : ""}`).join(", ")}
                    </div>
                    {r.balance_before !== null && (
                      <div className="num text-xs text-muted-foreground">
                        {t("Balance")} {money(r.balance_before)} → {money(r.balance_after ?? 0)}
                        {toNum(r.customer_credit) > 0 && <span className="font-medium text-warning"> · customer credit {money(r.customer_credit)} (follow up)</span>}
                        {toNum(r.customer_credit) === 0 && r.refund_method && r.refund_method !== "customer_credit" && ` · refunded by ${methodLabel(r.refund_method)}`}
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </Section>
          )}
          <Section title="Audit trail">
            <ul className="space-y-2 text-sm">
              {data.history.map((h, i) => (
                <li key={i} className="flex gap-2"><span className="text-muted-foreground">{dateTime(h.created_at)}</span> {titleCase(h.action)} · {h.user_name}{h.comments && ` — ${h.comments}`}</li>
              ))}
              {data.history.length === 0 && <li className="text-muted-foreground">{t("Created with the sale.")}</li>}
            </ul>
          </Section>
        </div>
        <div className="space-y-5">
          <Section title="Details">
            <KV label="Receipt"><Link to={`/sales/${c.sale_id}`} className="num text-primary">{c.receipt_no}</Link></KV>
            <KV label="Original amount"><span className="num">{money(c.original_amount)}</span></KV>
            {toNum(c.adjustments) > 0 && <KV label="Returns"><span className="num">−{money(c.adjustments)}</span></KV>}
            <KV label="Due date">{date(c.due_date)}</KV>
            <KV label="Days outstanding"><span className="num">{c.days_outstanding}</span></KV>
            <KV label="Branch">{c.branch_name}</KV>
            <KV label="Salesperson">{c.salesperson ?? "—"}</KV>
            <KV label="Customer"><Link to={`/customers/${c.customer_id}`} className="text-primary">{t("View profile")}</Link></KV>
          </Section>
          {data.can_recall && (
            <Button variant="outline" className="w-full" onClick={() => setRecalling(true)}><RotateCcw /> {t("Recall sale")}</Button>
          )}
          {open && can("credit.write_off") && (
            <Button variant="outline" className="w-full text-destructive" onClick={() => setWriting(true)}><Ban /> {t("Write off balance")}</Button>
          )}
        </div>
      </div>

      <RecallDialog open={recalling} onOpenChange={setRecalling} credit={c} items={data.items} onDone={refresh} />
      <ResponsiveDialog
        open={paying}
        onOpenChange={setPaying}
        title="Record repayment"
        description={`Outstanding ${money(c.balance)}`}
        footer={<Button className="w-full md:w-auto" disabled={pay.isPending || toNum(amountStr) <= 0 || (method === "mpesa" && reference.length < 8)} onClick={() => pay.mutate()}>{t("Save payment")}</Button>}
      >
        <div className="space-y-4">
          <Field label="Amount received">
            <div className="flex gap-2">
              <Input inputMode="decimal" className="num h-12 text-lg" value={amountStr} onChange={(e) => setAmount(e.target.value.replace(/[^\d.]/g, ""))} autoFocus />
              <Button variant="outline" className="h-12" onClick={() => setAmount(String(toNum(c.balance)))}>{t("Full")}</Button>
            </div>
          </Field>
          <div className="flex flex-wrap gap-2">
            {methods.map((m) => <Chip key={m.key} active={method === m.key} onClick={() => setMethod(m.key)}>{m.label}</Chip>)}
          </div>
          {method === "mpesa" && (
            <Field label="M-Pesa confirmation code"><Input className="num uppercase" value={reference} onChange={(e) => setReference(e.target.value.toUpperCase())} placeholder="e.g. QFT1ABC2DE" /></Field>
          )}
          {method !== "mpesa" && method !== "cash" && (
            <Field label="Reference" optional><Input value={reference} onChange={(e) => setReference(e.target.value)} /></Field>
          )}
        </div>
      </ResponsiveDialog>
      <ConfirmDialog
        open={writing}
        onOpenChange={setWriting}
        title="Write off this balance?"
        description={`${money(c.balance)} will no longer be collected. This is recorded in the audit trail.`}
        confirmLabel="Write off"
        destructive
        requireReason
        busy={writeOff.isPending}
        onConfirm={(reason) => writeOff.mutate(reason)}
      />
    </>
  );
}
