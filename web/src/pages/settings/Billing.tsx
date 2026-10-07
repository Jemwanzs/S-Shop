import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, CreditCard, Download, FileText, Landmark, Loader2, Receipt } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { date, dateTime, moneyDoc } from "@/lib/format";
import { t } from "@/lib/i18n";
import {
  billingPdf,
  CATEGORY_LABEL,
  frequencyLabel,
  METHOD_LABEL,
  periodLabel,
  STATUS_LABEL,
  STATUS_TONE,
  type BillingDocument,
  type BillingPayment,
  type BillingPlan,
  type BillingSummary,
  type DocumentView,
  type VendorPublic,
} from "@/lib/billing";
import { Button } from "@/components/ui/button";
import { Pill, StatusBadge } from "@/components/Badges";
import { Card, Fact, SettingsPage } from "./shared";

interface MyBilling {
  summary: BillingSummary;
  plan: BillingPlan | null;
  documents: BillingDocument[];
  payments: BillingPayment[];
  vendor: VendorPublic;
  paystack: boolean;
}

/** Settings → Billing: the business's own plan, invoices, receipts and *Pay now* (roadmap 39). */
export function BillingSettings() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["billing"], queryFn: () => api<MyBilling>("/billing") });
  const [params, setParams] = useSearchParams();
  const verifying = useRef(false);
  const [paying, setPaying] = useState<string | null>(null);

  // Back from Paystack (?reference=…): the server confirms the payment with Paystack before anything is shown as paid.
  const reference = params.get("reference") ?? params.get("trxref");
  const verify = useMutation({
    mutationFn: (ref: string) => api<{ status: string; receipt_no: string | null }>("/billing/paystack/verify", { body: { reference: ref } }),
    onSuccess: (r) => {
      if (r.status === "success") toast.success(`${t("Payment confirmed")} · ${r.receipt_no ?? ""}`);
      else if (r.status === "pending") toast.info(t("Payment not confirmed yet — it will update automatically once Paystack confirms it"));
      else toast.error(t("The payment did not go through"));
      qc.invalidateQueries({ queryKey: ["billing"] });
    },
    onError: (e) => toast.error(e),
    onSettled: () => setParams({}, { replace: true }),
  });
  useEffect(() => {
    if (reference && !verifying.current) {
      verifying.current = true;
      verify.mutate(reference);
    }
  }, [reference, verify]);

  const pay = useMutation({
    mutationFn: (id: string) => api<{ authorization_url: string }>(`/billing/invoices/${id}/pay`, { method: "POST" }),
    onMutate: (id) => setPaying(id),
    onSuccess: (r) => window.location.assign(r.authorization_url),
    onError: (e) => {
      setPaying(null);
      toast.error(e);
      qc.invalidateQueries({ queryKey: ["billing"] });
    },
  });
  const accept = useMutation({
    mutationFn: (id: string) => api<{ number: string }>(`/billing/quotations/${id}/accept`, { method: "POST" }),
    onSuccess: (r) => {
      toast.success(`${t("Invoice")} ${r.number}`);
      qc.invalidateQueries({ queryKey: ["billing"] });
    },
    onError: (e) => toast.error(e),
  });
  const download = async (doc: BillingDocument, payment?: BillingPayment) => {
    try {
      const v = await api<DocumentView>(`/billing/documents/${doc.id}`);
      await billingPdf(v, payment);
    } catch (e) {
      toast.error(e);
    }
  };

  const d = q.data;
  const s = d?.summary;
  const open = d?.documents.filter((x) => x.kind === "invoice" && x.status === "open") ?? [];
  const quotes = d?.documents.filter((x) => x.kind === "quotation" && x.status === "open") ?? [];
  const history = d?.documents.filter((x) => !(x.kind === "invoice" && x.status === "open") && !(x.kind === "quotation" && x.status === "open")) ?? [];
  const receipts = d?.payments ?? [];
  const docOf = (id: string) => d?.documents.find((x) => x.id === id);

  return (
    <SettingsPage title="Billing" description="Your S'Shop plan, invoices and receipts. Payments are confirmed with Paystack before they show as paid." loading={q.isLoading}>
      {verify.isPending && (
        <p className="flex items-center gap-2 rounded-lg bg-muted p-3 text-sm"><Loader2 className="h-4 w-4 animate-spin" /> {t("Confirming your payment with Paystack…")}</p>
      )}
      {s && (
        <Card title="Plan" action={<Pill tone={STATUS_TONE[s.status]}>{t(STATUS_LABEL[s.status])}</Pill>}>
          {s.status === "not_set" ? (
            <p className="py-2 text-sm text-muted-foreground">{t("No billing plan has been set up for this business yet.")}</p>
          ) : (
            <div className="grid grid-cols-2 gap-x-4 gap-y-2.5 py-2 sm:grid-cols-3">
              <Fact label={t("Billing model")}>{t(s.model === "one_off" ? "One-off" : "Subscription")}</Fact>
              {s.model === "one_off" && (
                <Fact label={t("One-off payment")}>
                  {moneyDoc(s.one_off_amount, s.currency)} · <span className={s.one_off_status === "paid" ? "text-success" : "text-warning"}>{t(s.one_off_status === "paid" ? "Paid" : "Pending")}</span>
                </Fact>
              )}
              {s.amount != null && (
                <Fact label={t(s.model === "one_off" ? "Maintenance fee" : "Amount")}>
                  {moneyDoc(s.amount, s.currency)} · {frequencyLabel(s.frequency, s.custom_months)}
                </Fact>
              )}
              {s.next_due && <Fact label={t(s.model === "one_off" ? "Next maintenance due" : "Next payment due")}>{date(s.next_due)}</Fact>}
              <Fact label={t("Outstanding")}><span className={Number(s.outstanding) > 0 ? "font-semibold text-destructive" : ""}>{moneyDoc(s.outstanding, s.currency)}</span></Fact>
              <Fact label={t("Last payment")}>{s.last_payment_at ? `${moneyDoc(s.last_payment_amount, s.currency)} · ${date(s.last_payment_at)}` : "—"}</Fact>
              {s.period_end && <Fact label={t("Period covered")}>{periodLabel(s)}</Fact>}
              {s.grace_days > 0 && s.amount != null && <Fact label={t("Grace period")}>{s.grace_days} {t("days")}</Fact>}
            </div>
          )}
        </Card>
      )}

      {open.length > 0 && (
        <Card title="To pay">
          {open.map((x) => (
            <div key={x.id} className="flex flex-wrap items-center gap-3 py-3">
              <span className="rounded-lg bg-primary/10 p-2 text-primary"><FileText className="h-4 w-4" /></span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 font-medium">
                  <span className="num">{x.number}</span>
                  {x.overdue ? <Pill tone="danger">{t("Overdue")}</Pill> : <Pill tone="warning">{t("Due")} {date(x.due_date)}</Pill>}
                </div>
                <p className="truncate text-xs text-muted-foreground">{x.description}</p>
              </div>
              <span className="num font-semibold">{moneyDoc(x.amount, x.currency)}</span>
              <div className="flex w-full gap-2 sm:w-auto">
                <Button size="sm" variant="outline" className="flex-1 sm:flex-none" onClick={() => download(x)}><Download /> PDF</Button>
                <Button size="sm" className="flex-1 sm:flex-none" disabled={!d?.paystack || pay.isPending} onClick={() => pay.mutate(x.id)}>
                  {paying === x.id ? <Loader2 className="animate-spin" /> : <CreditCard />} {t("Pay now")}
                </Button>
              </div>
            </div>
          ))}
          {!d?.paystack && <p className="py-2 text-xs text-muted-foreground">{t("Online payment is not available yet — pay by bank transfer using the details below.")}</p>}
        </Card>
      )}

      {quotes.length > 0 && (
        <Card title="Quotations">
          {quotes.map((x) => (
            <div key={x.id} className="flex flex-wrap items-center gap-3 py-3">
              <div className="min-w-0 flex-1">
                <p className="num font-medium">{x.number}</p>
                <p className="truncate text-xs text-muted-foreground">{x.description} · {t("Valid until")} {date(x.due_date)}</p>
              </div>
              <span className="num font-semibold">{moneyDoc(x.amount, x.currency)}</span>
              <div className="flex w-full gap-2 sm:w-auto">
                <Button size="sm" variant="outline" className="flex-1 sm:flex-none" onClick={() => download(x)}><Download /> PDF</Button>
                <Button size="sm" className="flex-1 sm:flex-none" disabled={accept.isPending} onClick={() => accept.mutate(x.id)}><CheckCircle2 /> {t("Accept")}</Button>
              </div>
            </div>
          ))}
        </Card>
      )}

      {d && (
        <Card title="Pay by bank">
          <div className="flex items-start gap-3 py-2 text-sm">
            <span className="rounded-lg bg-muted p-2"><Landmark className="h-4 w-4" /></span>
            <div className="space-y-0.5">
              <p><span className="text-muted-foreground">{t("Bank")}:</span> {d.vendor.bank_name || "—"}</p>
              <p><span className="text-muted-foreground">{t("Account")}:</span> <span className="num">{d.vendor.account_masked || "—"}</span>{d.vendor.account_name && ` · ${d.vendor.account_name}`}</p>
              {d.vendor.branch && <p><span className="text-muted-foreground">{t("Branch")}:</span> {d.vendor.branch}</p>}
              <p className="text-xs text-muted-foreground">{d.vendor.instructions || t("Use the invoice number as the payment reference.")}</p>
            </div>
          </div>
        </Card>
      )}

      <Card title="Receipts & payments">
        {receipts.length === 0 && <p className="py-2 text-sm text-muted-foreground">{t("No payments yet.")}</p>}
        {receipts.map((p) => {
          const doc = docOf(p.invoice_id);
          return (
            <div key={p.id} className="flex items-center gap-3 py-3">
              <span className="rounded-lg bg-success/10 p-2 text-success"><Receipt className="h-4 w-4" /></span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 font-medium">
                  <span className="num">{p.receipt_no ?? p.reference}</span>
                  {p.status !== "success" && <StatusBadge status={p.status} />}
                </div>
                <p className="truncate text-xs text-muted-foreground">
                  {p.invoice_number} · {t(METHOD_LABEL[p.method] ?? p.method)} · {dateTime(p.paid_at ?? p.created_at)}
                </p>
              </div>
              <span className="num font-semibold">{moneyDoc(p.amount, p.currency)}</span>
              {p.status === "success" && doc && (
                <Button size="icon" variant="ghost" aria-label={t("Download receipt")} onClick={() => download(doc, p)}><Download /></Button>
              )}
            </div>
          );
        })}
      </Card>

      <Card title="Billing history">
        {history.length === 0 && <p className="py-2 text-sm text-muted-foreground">{t("No invoices yet.")}</p>}
        {history.map((x) => (
          <div key={x.id} className="flex items-center gap-3 py-3">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5 font-medium">
                <span className="num">{x.number}</span>
                <StatusBadge status={x.status} />
              </div>
              <p className="truncate text-xs text-muted-foreground">
                {t(CATEGORY_LABEL[x.category])} · {x.period_start ? periodLabel(x) : date(x.issue_date)}{x.void_reason && ` · ${x.void_reason}`}
              </p>
            </div>
            <span className="num font-semibold">{moneyDoc(x.amount, x.currency)}</span>
            <Button size="icon" variant="ghost" aria-label={t("Download")} onClick={() => download(x)}><Download /></Button>
          </div>
        ))}
      </Card>
    </SettingsPage>
  );
}
