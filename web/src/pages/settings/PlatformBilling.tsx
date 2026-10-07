import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Ban, CalendarClock, CheckCircle2, ChevronRight, CircleDollarSign, Clock, Loader2, Wrench } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { date, dateTime, moneyDoc } from "@/lib/format";
import { t } from "@/lib/i18n";
import type { Money } from "@/lib/types";
import { STATUS_LABEL, STATUS_TONE, type BillingStatus } from "@/lib/billing";
import { StatCard } from "@/components/Stat";
import { Pill } from "@/components/Badges";
import { Chip } from "@/components/Filters";
import { Field } from "@/components/Form";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Loading } from "@/components/Page";
import { Card, SettingsPage } from "./shared";
import type { TenantRow } from "./Businesses";

type Rev = { month: Money; year: Money; all: Money };
interface Dashboard {
  counts: Record<"businesses" | "active" | "deactivated" | "paid" | "due_soon" | "grace" | "overdue" | "pending" | "not_set", number>;
  revenue: { subscription: Rev; one_off: Rev; maintenance: Rev; other: Rev; monthly_recurring: Money };
  outstanding: Money;
  maintenance_due: { count: number; amount: Money };
  attention: { id: string; business: string; invoice_number: string; amount: Money; reference: string; receipt_no: string | null; note: string; paid_at: string | null }[];
  items: TenantRow[];
  paystack: boolean;
}

interface Vendor { bank_name: string; account_name: string; account_number: string; branch: string; instructions: string }

type Filter = "all" | "active" | "deactivated" | BillingStatus;

/** Platform owner billing dashboard (roadmap 40) with drill-down per business, and the vendor bank details. */
export function PlatformBilling() {
  const q = useQuery({ queryKey: ["platform-billing"], queryFn: () => api<Dashboard>("/platform/billing") });
  const [filter, setFilter] = useState<Filter>("all");
  if (q.isLoading || !q.data) return <Loading />;
  const d = q.data;
  const c = d.counts;
  const k = (v: Money) => moneyDoc(v, "KES");
  const rows = d.items.filter((r) => !r.is_demo).filter((r) =>
    filter === "all" ? true : filter === "active" || filter === "deactivated" ? (filter === "active") === (r.status === "active") : r.billing.status === filter,
  );
  const chips: [Filter, string, number][] = [
    ["all", "All", c.businesses], ["active", "Active", c.active], ["deactivated", "Deactivated", c.deactivated], ["paid", "Paid up", c.paid],
    ["due_soon", "Due soon", c.due_soon], ["grace", "In grace period", c.grace], ["overdue", "Overdue", c.overdue], ["pending", "Payment pending", c.pending], ["not_set", "No plan", c.not_set],
  ];
  return (
    <SettingsPage title="Platform billing" description="Every business's billing position, revenue and what is due. Payments show as paid only after the server has confirmed them.">
      {!d.paystack && <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t("Paystack is not configured (PAYSTACK_SECRET_KEY). Businesses can still pay by bank transfer, recorded here.")}</p>}
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard label="Active" value={c.active} icon={CheckCircle2} tone="success" hint={`${c.deactivated} ${t("deactivated")}`} />
        <StatCard label="Paid up" value={c.paid} icon={CircleDollarSign} tone="primary" />
        <StatCard label="Due soon" value={c.due_soon + c.grace} icon={Clock} tone="warning" hint={c.grace ? `${c.grace} ${t("in grace period")}` : undefined} />
        <StatCard label="Overdue" value={c.overdue} icon={AlertTriangle} tone="danger" hint={k(d.outstanding) + " " + t("outstanding")} />
        <StatCard label="Subscription revenue" value={k(d.revenue.subscription.month)} icon={CalendarClock} hint={`${t("This year")} ${k(d.revenue.subscription.year)} · MRR ${k(d.revenue.monthly_recurring)}`} />
        <StatCard label="One-off revenue" value={k(d.revenue.one_off.year)} icon={CircleDollarSign} hint={`${t("All time")} ${k(d.revenue.one_off.all)}`} />
        <StatCard label="Maintenance due" value={d.maintenance_due.count} icon={Wrench} hint={`${k(d.maintenance_due.amount)} · ${t("next 30 days")}`} />
        <StatCard label="Deactivated" value={c.deactivated} icon={Ban} />
      </div>

      {d.attention.length > 0 && (
        <Card title="Needs attention">
          {d.attention.map((a) => (
            <div key={a.id} className="py-2.5 text-sm">
              <p className="font-medium">{a.business} · <span className="num">{a.invoice_number}</span> · {k(a.amount)}</p>
              <p className="text-xs text-warning">{a.note} · {a.receipt_no} · {a.reference} · {dateTime(a.paid_at)}</p>
            </div>
          ))}
        </Card>
      )}

      <div className="scrollbar-none -mx-3.5 flex gap-1.5 overflow-x-auto px-3.5 md:mx-0 md:flex-wrap md:px-0">
        {chips.map(([key, label, n]) => (
          <Chip key={key} active={filter === key} onClick={() => setFilter(key)}>{t(label)} <span className="num opacity-70">{n}</span></Chip>
        ))}
      </div>
      <Card>
        {rows.length === 0 && <p className="py-3 text-sm text-muted-foreground">{t("No businesses in this view.")}</p>}
        {rows.map((r) => (
          <Link key={r.id} to={`/settings/businesses/${r.id}`} className="flex items-center gap-3 py-2.5 text-sm hover:bg-accent/40">
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-1.5 font-medium">
                <span className="truncate">{r.name}</span>
                {r.status !== "active" && <Pill tone="danger">{t("Deactivated")}</Pill>}
                <Pill tone={STATUS_TONE[r.billing.status]}>{t(STATUS_LABEL[r.billing.status])}</Pill>
              </div>
              <p className="truncate text-xs text-muted-foreground">
                {r.billing.model ? t(r.billing.model === "one_off" ? "One-off" : "Subscription") : t("No plan")}
                {r.billing.next_due && ` · ${t("next due")} ${date(r.billing.next_due)}`}
                {r.billing.last_payment_at && ` · ${t("last paid")} ${date(r.billing.last_payment_at)}`}
              </p>
            </div>
            <span className={`num font-semibold ${Number(r.billing.outstanding) > 0 ? "text-destructive" : "text-muted-foreground"}`}>{k(r.billing.outstanding)}</span>
            <ChevronRight className="h-4 w-4 text-muted-foreground rtl:rotate-180" />
          </Link>
        ))}
      </Card>
      <VendorCard />
    </SettingsPage>
  );
}

function VendorCard() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["platform-vendor"], queryFn: () => api<{ vendor: Vendor; public: { account_masked: string } }>("/platform/billing/vendor") });
  const [f, setF] = useState<Vendor | null>(null);
  useEffect(() => {
    if (q.data) setF(q.data.vendor);
  }, [q.data]);
  const save = useMutation({
    mutationFn: (v: Vendor) => api("/platform/billing/vendor", { method: "PUT", body: v }),
    onSuccess: () => {
      toast.success("Saved");
      qc.invalidateQueries({ queryKey: ["platform-vendor"] });
    },
    onError: (e) => toast.error(e),
  });
  if (!f) return null;
  const set = (k: keyof Vendor, v: string) => setF({ ...f, [k]: v });
  return (
    <Card title="Vendor payment details">
      <div className="space-y-3 py-2">
        <p className="text-xs text-muted-foreground">{t("Shown to businesses on their Billing page, with the account number masked")}: {q.data?.public.account_masked || "—"}</p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field label="Bank"><Input value={f.bank_name} onChange={(e) => set("bank_name", e.target.value)} /></Field>
          <Field label="Account number"><Input value={f.account_number} onChange={(e) => set("account_number", e.target.value)} /></Field>
          <Field label="Account name" optional><Input value={f.account_name} onChange={(e) => set("account_name", e.target.value)} /></Field>
          <Field label="Branch" optional><Input value={f.branch} onChange={(e) => set("branch", e.target.value)} /></Field>
        </div>
        <Field label="Payment instructions" optional><Textarea rows={2} value={f.instructions} onChange={(e) => set("instructions", e.target.value)} /></Field>
        <div className="flex justify-end">
          <Button size="sm" disabled={save.isPending} onClick={() => save.mutate(f)}>{save.isPending ? <Loader2 className="animate-spin" /> : t("Save")}</Button>
        </div>
      </div>
    </Card>
  );
}
