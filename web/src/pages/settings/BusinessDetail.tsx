import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ArrowRightLeft, Ban, Copy, Download, FilePlus2, KeyRound, Loader2, MapPin, Pencil, Power, RefreshCw, Wallet, XCircle } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { ago, count, date, dateTime, moneyDoc, todayIso } from "@/lib/format";
import { t } from "@/lib/i18n";
import type { Profile } from "@/lib/types";
import {
  billingPdf,
  CATEGORY_LABEL,
  FREQUENCIES,
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
  type ModuleDef,
  calculate,
  packageLabel,
} from "@/lib/billing";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Pill, StatusBadge } from "@/components/Badges";
import { ConfirmDialog, Field, Select, ToggleRow } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Loading } from "@/components/Page";
import { Card, Fact, PriceLines, SettingsPage } from "./shared";
import type { TenantRow } from "./Businesses";

interface Detail {
  tenant: TenantRow;
  users: { id: string; name: string; email: string; phone: string; role: string; is_admin: boolean; is_active: boolean; last_login_at: string | null; failed_attempts: number; locked_until: string | null; failed_last_7d: number }[];
  branches: { id: string; name: string; code: string; location: string; latitude: number | null; longitude: number | null; is_active: boolean }[];
  onboarding: { contact_name: string; email: string; phone: string; location: string; business_type: string; branches: number | null; message: string; created_at: string; decided_at: string | null; decided_by_name: string | null } | null;
  status_history: { created_at: string; by_name: string | null; action: string; after: { reason?: string; changes?: Record<string, unknown> } | null }[];
  billing: { plan: (BillingPlan & { notes: string }) | null; summary: BillingSummary; documents: BillingDocument[]; payments: BillingPayment[]; paystack: boolean; catalogue: ModuleDef[] };
  is_home: boolean;
}

const HISTORY_LABEL: Record<string, string> = {
  deactivate_business: "Deactivated",
  reactivate_business: "Reactivated",
  billing_suspended: "Billing suspended",
  billing_restored: "Billing access restored",
  trial_ended: "Trial ended — billing applies",
  plan_updated: "Billing plan changed",
};

/** Platform owner: one business in full — contacts, users, branches, onboarding, activation and billing. */
export function BusinessDetail() {
  const { id = "" } = useParams();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const { switchBusiness } = useSession();
  const key = ["platform-tenant", id];
  const q = useQuery({ queryKey: key, queryFn: () => api<Detail>(`/platform/tenants/${id}`) });
  const refresh = () => {
    qc.invalidateQueries({ queryKey: key });
    qc.invalidateQueries({ queryKey: ["platform-tenants"] });
    qc.invalidateQueries({ queryKey: ["platform-billing"] });
  };
  const [statusOpen, setStatusOpen] = useState(false);
  const [resetUser, setResetUser] = useState<Detail["users"][number] | null>(null);
  const [tempPin, setTempPin] = useState<{ name: string; email: string; pin: string } | null>(null);
  const [planOpen, setPlanOpen] = useState(false);
  const [issueOpen, setIssueOpen] = useState(false);
  const [payDoc, setPayDoc] = useState<BillingDocument | null>(null);
  const [voidDoc, setVoidDoc] = useState<BillingDocument | null>(null);

  const open = useMutation({
    mutationFn: () => api<{ token: string; profile: Profile }>(`/platform/tenants/${id}/open`, { method: "POST" }),
    onSuccess: (r) => {
      switchBusiness(r.token, r.profile);
      toast.success(`${t("Now in")} ${r.profile.tenant.name}`);
      navigate(r.profile.branches.length > 1 ? "/select-branch" : "/", { replace: true });
    },
    onError: (e) => toast.error(e),
  });
  const setStatus = useMutation({
    mutationFn: (b: { status: string; reason: string }) => api(`/platform/tenants/${id}/status`, { body: b }),
    onSuccess: (_r, b) => {
      setStatusOpen(false);
      toast.success(b.status === "active" ? "Business reactivated" : "Business deactivated");
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const resetPin = useMutation({
    mutationFn: (uid: string) => api<{ name: string; email: string; temporary_pin: string }>(`/platform/tenants/${id}/users/${uid}/reset-pin`, { method: "POST" }),
    onSuccess: (r) => {
      setResetUser(null);
      setTempPin({ name: r.name, email: r.email, pin: r.temporary_pin });
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const docAction = useMutation({
    mutationFn: ({ doc, action, body }: { doc: BillingDocument; action: "void" | "invoice"; body?: unknown }) =>
      api(`/platform/billing/documents/${doc.id}/${action}`, { body: body ?? {} }),
    onSuccess: () => {
      setVoidDoc(null);
      toast.success("Saved");
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const verify = useMutation({
    mutationFn: (pid: string) => api<{ status: string }>(`/platform/billing/payments/${pid}/verify`, { method: "POST" }),
    onSuccess: (r) => {
      toast.info(`${t("Paystack")}: ${t(r.status)}`);
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const download = async (doc: BillingDocument, payment?: BillingPayment) => {
    try {
      await billingPdf(await api<DocumentView>(`/platform/billing/documents/${doc.id}`), payment);
    } catch (e) {
      toast.error(e);
    }
  };

  if (q.isLoading || !q.data) return <Loading />;
  const d = q.data;
  const b = d.tenant;
  const s = d.billing.summary;
  const active = b.status === "active";
  // The platform owner's own business: never deactivated, suspended or billed (also enforced by the server and database).
  const owned = b.ownership === "platform";

  return (
    <SettingsPage title={b.name} description={`/${b.slug}`}>
      <div className="-mt-3 flex flex-wrap items-center gap-2">
        <Button size="sm" variant="ghost" asChild><Link to="/settings/businesses"><ArrowLeft className="rtl:rotate-180" /> {t("Businesses")}</Link></Button>
        <Pill tone={active ? "success" : "danger"}>{t(active ? "Active" : "Deactivated")}</Pill>
        {owned && <Pill tone="primary">{t("Platform owned")}</Pill>}
        <Pill tone={STATUS_TONE[s.status]}>{t(STATUS_LABEL[s.status])}</Pill>
        {b.is_demo && <Pill tone="info">{t("Demo")}</Pill>}
        <span className="flex-1" />
        <Button size="sm" variant="outline" disabled={open.isPending} onClick={() => open.mutate()}><ArrowRightLeft /> {t("Open business")}</Button>
        <Button
          size="sm"
          variant={active ? "destructive" : "default"}
          disabled={owned || d.is_home}
          title={owned ? t("The platform owner's business cannot be deactivated") : undefined}
          onClick={() => setStatusOpen(true)}
        >
          <Power /> {t(active ? "Deactivate" : "Reactivate")}
        </Button>
      </div>
      {!active && (
        <p className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
          {t("Deactivated")} {b.status_changed_at && ago(b.status_changed_at)} — {b.status_reason}. {t("Sign-in, the ordering link and new transactions are blocked; all data is kept.")}
        </p>
      )}

      <Card title="Business">
        <div className="grid grid-cols-2 gap-x-4 gap-y-3 py-2 sm:grid-cols-3">
          <Fact label={t("Administrator")}>{b.admin_name ?? "—"}</Fact>
          <Fact label={t("Admin email")}><span className="break-all">{b.admin_email ?? "—"}</span></Fact>
          <Fact label={t("Admin phone")}>{b.admin_phone || b.phone || "—"}</Fact>
          <Fact label={t("Business email")}><span className="break-all">{b.email || "—"}</span></Fact>
          <Fact label={t("Location")}>{b.address || "—"}</Fact>
          <Fact label={t("Activated")}>{date(b.activated_at)}</Fact>
          <Fact label={t("Users")}>{count(b.users)}</Fact>
          <Fact label={t("Branches")}>{count(b.branches)}</Fact>
          <Fact label={t("Sales")}>{count(b.sales)}{b.last_sale_at && <span className="text-xs text-muted-foreground"> · {ago(b.last_sale_at)}</span>}</Fact>
          <Fact label={t("Last sign-in")}>{b.last_login_at ? ago(b.last_login_at) : "—"}</Fact>
        </div>
      </Card>

      {d.onboarding && (
        <Card title="Signup & onboarding">
          <div className="grid grid-cols-2 gap-x-4 gap-y-3 py-2 sm:grid-cols-3">
            <Fact label={t("Contact")}>{d.onboarding.contact_name}</Fact>
            <Fact label={t("Phone")}>{d.onboarding.phone}</Fact>
            <Fact label={t("Email")}><span className="break-all">{d.onboarding.email}</span></Fact>
            <Fact label={t("Business type")}>{d.onboarding.business_type || "—"}</Fact>
            <Fact label={t("Branches requested")}>{d.onboarding.branches ?? "—"}</Fact>
            <Fact label={t("Requested")}>{date(d.onboarding.created_at)}</Fact>
            <Fact label={t("Approved")}>{d.onboarding.decided_at ? `${date(d.onboarding.decided_at)}${d.onboarding.decided_by_name ? ` · ${d.onboarding.decided_by_name}` : ""}` : "—"}</Fact>
          </div>
          {d.onboarding.message && <p className="py-2 text-sm text-muted-foreground">“{d.onboarding.message}”</p>}
        </Card>
      )}

      <Card title="Users">
        {d.users.map((u) => (
          <div key={u.id} className="flex flex-wrap items-center gap-2 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-1.5 text-sm font-medium">
                <span className="truncate">{u.name}</span>
                {u.is_admin && <Pill tone="primary">{t("Admin")}</Pill>}
                {!u.is_active && <Pill tone="danger">{t("Inactive")}</Pill>}
                {u.locked_until && new Date(u.locked_until) > new Date() && <Pill tone="danger">{t("Locked")}</Pill>}
                {u.failed_last_7d > 0 && <Pill tone="warning">{u.failed_last_7d} {t("failed sign-ins")}</Pill>}
              </div>
              <p className="truncate text-xs text-muted-foreground">{u.email} · {u.role} · {u.last_login_at ? `${t("signed in")} ${ago(u.last_login_at)}` : t("never signed in")}</p>
            </div>
            <Button size="sm" variant="outline" onClick={() => setResetUser(u)}><KeyRound /> {t("Reset PIN")}</Button>
          </div>
        ))}
      </Card>

      <Card title="Branches & locations">
        {d.branches.map((br) => (
          <div key={br.id} className="flex items-center gap-3 py-2.5 text-sm">
            <MapPin className="h-4 w-4 shrink-0 text-muted-foreground" />
            <div className="min-w-0 flex-1">
              <p className="truncate font-medium">{br.name} <span className="text-xs text-muted-foreground">{br.code}</span>{!br.is_active && <Pill tone="danger" className="ms-1">{t("Inactive")}</Pill>}</p>
              <p className="truncate text-xs text-muted-foreground">{br.location || "—"}</p>
            </div>
            {br.latitude != null && br.longitude != null && (
              <a className="text-xs text-primary underline-offset-2 hover:underline" target="_blank" rel="noreferrer" href={`https://www.google.com/maps?q=${br.latitude},${br.longitude}`}>{t("Map")}</a>
            )}
          </div>
        ))}
      </Card>

      <Card
        title="Billing"
        action={
          <div className="flex gap-1.5">
            <Button size="sm" variant="outline" disabled={owned} onClick={() => setPlanOpen(true)}><Pencil /> {t("Plan")}</Button>
            <Button size="sm" disabled={owned} onClick={() => setIssueOpen(true)}><FilePlus2 /> {t("Issue")}</Button>
          </div>
        }
      >
        {owned && (
          <p className="py-2 text-sm text-muted-foreground">{t("Platform owned: the full platform is included, no subscription or maintenance invoices are issued and the business cannot be charged, suspended or deactivated.")}</p>
        )}
        <div className="grid grid-cols-2 gap-x-4 gap-y-3 py-2 sm:grid-cols-3">
          <Fact label={t("Billing model")}>{s.model ? t(s.model === "one_off" ? "One-off" : "Subscription") : t(owned ? "Platform owned" : "Not set")}</Fact>
          <Fact label={t("Package")}>{packageLabel(s.package, s.modules, d.billing.catalogue)}</Fact>
          {s.access_mode && s.access_mode !== "billed" && (
            <Fact label={t("Access")}>{s.access_mode === "free" ? t("Free") : `${t("Trial")} ${date(s.trial_start)} – ${date(s.trial_end)}`}</Fact>
          )}
          {s.grace_until && <Fact label={t("Grace extended to")}>{date(s.grace_until)}</Fact>}
          {s.model === "one_off" && <Fact label={t("One-off")}>{moneyDoc(s.one_off_amount, s.currency)} · {t(s.one_off_status === "paid" ? "Paid" : "Pending")}</Fact>}
          {s.amount != null && <Fact label={t(s.model === "one_off" ? "Maintenance" : "Subscription")}>{moneyDoc(s.amount, s.currency)} · {frequencyLabel(s.frequency, s.custom_months)}</Fact>}
          <Fact label={t("Next due")}>{s.next_due ? date(s.next_due) : "—"}</Fact>
          <Fact label={t("Outstanding")}>{moneyDoc(s.outstanding, s.currency)}</Fact>
          <Fact label={t("Last payment")}>{s.last_payment_at ? `${moneyDoc(s.last_payment_amount, s.currency)} · ${date(s.last_payment_at)}` : "—"}</Fact>
          <Fact label={t("Paid to date")}>{moneyDoc(s.paid_total, s.currency)}</Fact>
          {s.period_end && <Fact label={t("Period covered")}>{periodLabel(s)}</Fact>}
          {d.billing.plan && <Fact label={t("Auto-renew")}>{t(d.billing.plan.auto_renew ? "On" : "Off")} · {t("grace")} {d.billing.plan.grace_days}d</Fact>}
        </div>
        {(s.recurring_price || (s.one_off_price && s.one_off_status !== "paid")) && (
          <div className="grid gap-3 py-2 sm:grid-cols-2">
            {s.one_off_price && s.one_off_status !== "paid" && <PriceLines price={s.one_off_price} currency={s.currency} label="One-off payable" />}
            {s.recurring_price && <PriceLines price={s.recurring_price} currency={s.currency} label={s.model === "one_off" ? "Maintenance payable" : "Payable per period"} />}
          </div>
        )}
        {d.billing.plan?.notes && <p className="py-2 text-xs text-muted-foreground">{d.billing.plan.notes}</p>}
      </Card>

      <Card title="Quotations & invoices">
        {d.billing.documents.length === 0 && <p className="py-2 text-sm text-muted-foreground">{t("Nothing issued yet.")}</p>}
        {d.billing.documents.map((x) => (
          <div key={x.id} className="flex flex-wrap items-center gap-2 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-1.5 text-sm font-medium">
                <span className="num">{x.number}</span>
                <StatusBadge status={x.overdue ? "overdue" : x.status} />
                <Pill>{t(CATEGORY_LABEL[x.category])}</Pill>
              </div>
              <p className="truncate text-xs text-muted-foreground">
                {x.description} · {t(x.kind === "quotation" ? "valid until" : "due")} {date(x.due_date)}{x.void_reason && ` · ${x.void_reason}`}
              </p>
            </div>
            <span className="num text-sm font-semibold">{moneyDoc(x.amount, x.currency)}</span>
            <div className="flex gap-1">
              <Button size="icon-sm" variant="ghost" aria-label={t("Download")} onClick={() => download(x)}><Download /></Button>
              {x.status === "open" && x.kind === "invoice" && (
                <Button size="sm" variant="outline" onClick={() => setPayDoc(x)}><Wallet /> {t("Record payment")}</Button>
              )}
              {x.status === "open" && x.kind === "quotation" && (
                <Button size="sm" variant="outline" disabled={docAction.isPending} onClick={() => docAction.mutate({ doc: x, action: "invoice" })}>{t("Invoice it")}</Button>
              )}
              {x.status === "open" && (
                <Button size="icon-sm" variant="ghost" aria-label={t("Void")} onClick={() => setVoidDoc(x)}><XCircle /></Button>
              )}
            </div>
          </div>
        ))}
      </Card>

      <Card title="Payments">
        {d.billing.payments.length === 0 && <p className="py-2 text-sm text-muted-foreground">{t("No payments yet.")}</p>}
        {d.billing.payments.map((p) => {
          const doc = d.billing.documents.find((x) => x.id === p.invoice_id);
          return (
            <div key={p.id} className="flex flex-wrap items-center gap-2 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5 text-sm font-medium">
                  <span className="num">{p.receipt_no ?? p.reference}</span>
                  <StatusBadge status={p.status === "success" ? "paid" : p.status} />
                </div>
                <p className="truncate text-xs text-muted-foreground">
                  {p.invoice_number} · {t(METHOD_LABEL[p.method] ?? p.method)}{p.channel && p.channel !== p.method ? ` (${p.channel})` : ""} · {p.reference} · {dateTime(p.paid_at ?? p.created_at)}
                </p>
                {p.note && <p className="text-xs text-warning">{p.note}</p>}
              </div>
              <span className="num text-sm font-semibold">{moneyDoc(p.amount, p.currency)}</span>
              {p.status === "success" && doc && <Button size="icon-sm" variant="ghost" aria-label={t("Download receipt")} onClick={() => download(doc, p)}><Download /></Button>}
              {p.method === "paystack" && (p.status === "pending" || p.status === "abandoned") && (
                <Button size="sm" variant="outline" disabled={verify.isPending} onClick={() => verify.mutate(p.id)}><RefreshCw /> {t("Check")}</Button>
              )}
            </div>
          );
        })}
      </Card>

      {d.status_history.length > 0 && (
        <Card title="Status & billing history">
          {d.status_history.map((h, i) => (
            <div key={i} className="flex items-center gap-2 py-2 text-sm">
              {h.action === "deactivate_business" || h.action === "billing_suspended" ? <Ban className="h-4 w-4 text-destructive" /> : <Power className="h-4 w-4 text-success" />}
              <span className="flex-1">
                {t(HISTORY_LABEL[h.action] ?? h.action)}{h.after?.reason && ` — ${h.after.reason}`}
                {h.action === "plan_updated" && h.after?.changes && <span className="text-xs text-muted-foreground"> · {Object.keys(h.after.changes).join(", ")}</span>}
              </span>
              <span className="text-xs text-muted-foreground">{h.by_name} · {dateTime(h.created_at)}</span>
            </div>
          ))}
        </Card>
      )}

      <ConfirmDialog
        open={statusOpen}
        onOpenChange={setStatusOpen}
        title={active ? "Deactivate this business?" : "Reactivate this business?"}
        description={active
          ? "Everyone in this business is signed out and cannot sign in, the ordering link is switched off and no new transactions can be made. All data is kept."
          : "Sign-in, the ordering link and transactions are restored."}
        confirmLabel={active ? "Deactivate" : "Reactivate"}
        destructive={active}
        requireReason={active}
        busy={setStatus.isPending}
        onConfirm={(reason) => setStatus.mutate({ status: active ? "deactivated" : "active", reason })}
      />
      <ConfirmDialog
        open={!!resetUser}
        onOpenChange={(o) => !o && setResetUser(null)}
        title="Reset this user's PIN?"
        description={resetUser ? `${resetUser.name} (${resetUser.email}) gets a one-time PIN to pass on; their sign-in lock is cleared.` : ""}
        confirmLabel="Reset PIN"
        busy={resetPin.isPending}
        onConfirm={() => resetUser && resetPin.mutate(resetUser.id)}
      />
      <ResponsiveDialog
        open={!!tempPin}
        onOpenChange={(o) => !o && setTempPin(null)}
        title="One-time PIN"
        description="Pass it on privately. It is shown only once — ask them to change it after signing in (More → Change PIN)."
        footer={<Button onClick={() => setTempPin(null)}>{t("Done")}</Button>}
      >
        {tempPin && (
          <div className="space-y-2 text-sm">
            <p>{tempPin.name} · {tempPin.email}</p>
            <div className="flex items-center gap-2">
              <code className="num flex-1 rounded-lg bg-muted px-3 py-2 text-lg tracking-widest">{tempPin.pin}</code>
              <Button size="icon" variant="outline" aria-label={t("Copy")} onClick={() => navigator.clipboard?.writeText(tempPin.pin).then(() => toast.success("Copied"))}><Copy /></Button>
            </div>
          </div>
        )}
      </ResponsiveDialog>
      <ConfirmDialog
        open={!!voidDoc}
        onOpenChange={(o) => !o && setVoidDoc(null)}
        title="Void this document?"
        description={voidDoc ? `${voidDoc.number} — ${moneyDoc(voidDoc.amount, voidDoc.currency)}. It stays in the history, marked void.` : ""}
        confirmLabel="Void"
        destructive
        requireReason
        busy={docAction.isPending}
        onConfirm={(reason) => voidDoc && docAction.mutate({ doc: voidDoc, action: "void", body: { reason } })}
      />
      {planOpen && <PlanDialog tenantId={id} plan={d.billing.plan} catalogue={d.billing.catalogue} onClose={() => setPlanOpen(false)} onSaved={refresh} />}
      {issueOpen && <IssueDialog tenantId={id} plan={d.billing.plan} onClose={() => setIssueOpen(false)} onSaved={refresh} />}
      {payDoc && <RecordPaymentDialog doc={payDoc} onClose={() => setPayDoc(null)} onSaved={refresh} />}
    </SettingsPage>
  );
}

function PlanDialog({ tenantId, plan, catalogue, onClose, onSaved }: { tenantId: string; plan: (BillingPlan & { notes: string }) | null; catalogue: ModuleDef[]; onClose: () => void; onSaved: () => void }) {
  const str = (v: unknown) => (v === null || v === undefined ? "" : String(v));
  const [f, setF] = useState({
    model: plan?.model ?? "subscription",
    access_mode: plan?.access_mode ?? "billed",
    package: plan?.package ?? "full",
    modules: plan?.modules ?? [],
    module_prices: Object.fromEntries(Object.entries(plan?.module_prices ?? {}).map(([k, v]) => [k, str(v)])) as Record<string, string>,
    currency: plan?.currency ?? "KES",
    one_off_amount: plan && Number(plan.one_off_amount) > 0 ? str(plan.one_off_amount) : "",
    one_off_paid_on: plan?.one_off_paid_on ?? "",
    maintenance: plan ? plan.model === "one_off" && plan.recurring : false,
    amount: plan && plan.recurring ? str(plan.amount) : "",
    frequency: plan?.frequency ?? "monthly",
    custom_months: str(plan?.custom_months ?? 1),
    start_date: plan?.start_date ?? todayIso(),
    next_due_date: plan?.next_due_date ?? "",
    grace_days: str(plan?.grace_days ?? 7),
    grace_until: plan?.grace_until ?? "",
    auto_renew: plan?.auto_renew ?? true,
    auto_suspend: plan?.auto_suspend ?? false,
    discount_type: plan?.discount_type ?? "none",
    discount_value: plan && Number(plan.discount_value) > 0 ? str(plan.discount_value) : "",
    tax_enabled: plan?.tax_enabled ?? false,
    tax_rate: plan && Number(plan.tax_rate) > 0 ? str(plan.tax_rate) : "16",
    trial_start: plan?.trial_start ?? todayIso(),
    trial_end: plan?.trial_end ?? "",
    trial_modules: plan?.trial_modules ?? [],
    notes: plan?.notes ?? "",
  });
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF((x) => ({ ...x, [k]: v }));
  const toggle = (k: "modules" | "trial_modules", m: string) => set(k, f[k].includes(m) ? f[k].filter((x) => x !== m) : [...f[k], m]);
  const free = f.access_mode === "free";
  const recurring = f.model === "subscription" || f.maintenance;
  const perModule = f.model === "subscription" && f.package === "modules";
  const base = perModule && f.modules.some((m) => f.module_prices[m])
    ? f.modules.reduce((sum, m) => sum + Number(f.module_prices[m] || 0), 0)
    : Number(f.amount || 0);
  const calc = (b: number) => calculate(b, f.discount_type, Number(f.discount_value || 0), f.tax_enabled, Number(f.tax_rate || 0));
  const save = useMutation({
    mutationFn: () =>
      api<{ changes: Record<string, unknown> }>(`/platform/tenants/${tenantId}/billing-plan`, {
        method: "PUT",
        body: {
          model: f.model, access_mode: f.access_mode, package: f.package, modules: f.package === "modules" ? f.modules : [],
          module_prices: Object.fromEntries(Object.entries(f.module_prices).filter(([k, v]) => v !== "" && f.modules.includes(k)).map(([k, v]) => [k, Number(v)])),
          currency: f.currency, one_off_amount: Number(f.one_off_amount || 0), one_off_paid_on: f.model === "one_off" && f.one_off_paid_on ? f.one_off_paid_on : null,
          maintenance: f.maintenance, amount: Number(f.amount || 0), frequency: f.frequency, custom_months: Number(f.custom_months || 1),
          start_date: recurring ? f.start_date || null : null, next_due_date: recurring ? f.next_due_date || null : null,
          grace_days: Number(f.grace_days || 0), grace_until: f.grace_until || null, auto_renew: f.auto_renew, auto_suspend: f.auto_suspend,
          discount_type: f.discount_type, discount_value: Number(f.discount_value || 0), tax_enabled: f.tax_enabled, tax_rate: Number(f.tax_rate || 0),
          trial_start: f.access_mode === "trial" ? f.trial_start || null : null, trial_end: f.access_mode === "trial" ? f.trial_end || null : null,
          trial_modules: f.access_mode === "trial" ? f.trial_modules : [], notes: f.notes,
        },
      }),
    onSuccess: (r) => {
      const n = Object.keys(r.changes ?? {}).length;
      toast.success(n ? `${t("Billing plan saved")} · ${n} ${t("changes recorded")}` : t("No changes"));
      onSaved();
      onClose();
    },
    onError: (e) => toast.error(e),
  });
  const what = f.model === "subscription" ? "Subscription" : "Maintenance";
  const moduleChecks = (k: "modules" | "trial_modules", withPrices: boolean) => (
    <div className="grid gap-1.5 sm:grid-cols-2">
      {catalogue.map((m) => (
        <div key={m.key} className="flex items-center gap-2 rounded-lg border px-2.5 py-1.5 text-sm">
          <label className="flex min-w-0 flex-1 items-center gap-2">
            <input type="checkbox" className="h-4 w-4 accent-[hsl(var(--primary))]" checked={f[k].includes(m.key)} onChange={() => toggle(k, m.key)} />
            <span className="truncate">{t(m.label)}</span>
          </label>
          {withPrices && f[k].includes(m.key) && (
            <Input className="h-8 w-24" type="number" inputMode="decimal" min={0} placeholder={t("Price")} value={f.module_prices[m.key] ?? ""}
              onChange={(e) => set("module_prices", { ...f.module_prices, [m.key]: e.target.value })} />
          )}
        </div>
      ))}
    </div>
  );
  return (
    <ResponsiveDialog
      open
      wide
      onOpenChange={(o) => !o && onClose()}
      title="Billing plan"
      description="Tenant-specific package, price, discount, tax and access. Every change is recorded with its previous and new value."
      footer={<Button onClick={() => save.mutate()} disabled={save.isPending}>{save.isPending ? <Loader2 className="animate-spin" /> : t("Save plan")}</Button>}
    >
      <div className="space-y-4">
        <section className="space-y-3">
          <p className="label-caps">{t("Access")}</p>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Access">
              <Select value={f.access_mode} onChange={(v) => set("access_mode", v as typeof f.access_mode)}>
                <option value="billed">{t("Billed")}</option>
                <option value="trial">{t("Trial")}</option>
                <option value="free">{t("Free access")}</option>
              </Select>
            </Field>
            <Field label="Billing model">
              <Select value={f.model} onChange={(v) => set("model", v as typeof f.model)}>
                <option value="subscription">{t("Subscription")}</option>
                <option value="one_off">{t("One-off")}</option>
              </Select>
            </Field>
          </div>
          {free && <p className="rounded-lg bg-chart-2/10 p-2.5 text-xs">{t("Free access switches billing off for this business without deactivating it. No invoices are issued automatically.")}</p>}
          {f.access_mode === "trial" && (
            <div className="space-y-2 rounded-lg border p-2.5">
              <div className="grid grid-cols-2 gap-3">
                <Field label="Trial start"><Input type="date" value={f.trial_start} onChange={(e) => set("trial_start", e.target.value)} /></Field>
                <Field label="Trial end"><Input type="date" min={f.trial_start} value={f.trial_end} onChange={(e) => set("trial_end", e.target.value)} /></Field>
              </div>
              <p className="text-xs text-muted-foreground">{t("Modules during the trial (none ticked = the package's modules). After the trial ends the plan below applies and billing starts the next day.")}</p>
              {moduleChecks("trial_modules", false)}
            </div>
          )}
        </section>

        <section className="space-y-3">
          <p className="label-caps">{t("Package")}</p>
          <Field label="Package">
            <Select value={f.package} onChange={(v) => set("package", v as typeof f.package)}>
              <option value="full">{t("Full platform (all modules)")}</option>
              <option value="modules">{t("Selected modules")}</option>
            </Select>
          </Field>
          {f.package === "modules" && (
            <>
              <p className="text-xs text-muted-foreground">{t("Modules outside the package are refused by the server, not just hidden. Products, dashboard, settings, users and approvals are always included.")}</p>
              {moduleChecks("modules", f.model === "subscription")}
            </>
          )}
        </section>

        <section className="space-y-3">
          <p className="label-caps">{t("Pricing")}</p>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Currency"><Input value={f.currency} maxLength={3} onChange={(e) => set("currency", e.target.value.toUpperCase())} /></Field>
            {recurring && (
              <Field label="Frequency">
                <Select value={f.frequency} onChange={(v) => set("frequency", v)}>
                  {FREQUENCIES.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}
                </Select>
              </Field>
            )}
          </div>
          {recurring && f.frequency === "custom" && (
            <Field label="Every (months)"><Input type="number" min={1} max={60} value={f.custom_months} onChange={(e) => set("custom_months", e.target.value)} /></Field>
          )}
          {f.model === "one_off" && (
            <div className="grid grid-cols-2 gap-3">
              <Field label="One-off amount" optional={free}><Input type="number" inputMode="decimal" min={0} value={f.one_off_amount} onChange={(e) => set("one_off_amount", e.target.value)} /></Field>
              <Field label="One-off paid on" optional hint="Mark as paid when it was paid outside S'Shop"><Input type="date" max={todayIso()} value={f.one_off_paid_on} onChange={(e) => set("one_off_paid_on", e.target.value)} /></Field>
            </div>
          )}
          {f.model === "one_off" && <ToggleRow label="Maintenance required" checked={f.maintenance} onChange={(v) => set("maintenance", v)} />}
          {recurring && !perModule && (
            <Field label={`${what} base price`} optional={free}><Input type="number" inputMode="decimal" min={0} value={f.amount} onChange={(e) => set("amount", e.target.value)} /></Field>
          )}
          {perModule && (
            <Field label="Base price" hint="Sum of the included modules' prices, or a single price when none are set" optional={free}>
              <Input type="number" inputMode="decimal" min={0} value={f.modules.some((m) => f.module_prices[m]) ? String(base) : f.amount}
                disabled={f.modules.some((m) => f.module_prices[m])} onChange={(e) => set("amount", e.target.value)} />
            </Field>
          )}
          <div className="grid grid-cols-2 gap-3">
            <Field label="Discount">
              <Select value={f.discount_type} onChange={(v) => set("discount_type", v as typeof f.discount_type)}>
                <option value="none">{t("No discount")}</option>
                <option value="percent">{t("Percentage")}</option>
                <option value="fixed">{t("Fixed amount")}</option>
              </Select>
            </Field>
            {f.discount_type !== "none" && (
              <Field label={f.discount_type === "percent" ? "Discount (%)" : "Discount amount"}>
                <Input type="number" inputMode="decimal" min={0} max={f.discount_type === "percent" ? 100 : undefined} value={f.discount_value} onChange={(e) => set("discount_value", e.target.value)} />
              </Field>
            )}
          </div>
          <div className="grid grid-cols-2 items-end gap-3">
            <ToggleRow label="Tax applicable" checked={f.tax_enabled} onChange={(v) => set("tax_enabled", v)} />
            {f.tax_enabled && <Field label="Tax (%)"><Input type="number" inputMode="decimal" min={0} max={100} value={f.tax_rate} onChange={(e) => set("tax_rate", e.target.value)} /></Field>}
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            {f.model === "one_off" && Number(f.one_off_amount) > 0 && <PriceLines price={calc(Number(f.one_off_amount))} currency={f.currency} label="One-off payable" />}
            {recurring && base > 0 && <PriceLines price={calc(base)} currency={f.currency} label={f.model === "one_off" ? "Maintenance payable" : "Payable per period"} />}
          </div>
        </section>

        {recurring && (
          <section className="space-y-3">
            <p className="label-caps">{t("Dates & grace")}</p>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Start date"><Input type="date" value={f.start_date} onChange={(e) => set("start_date", e.target.value)} /></Field>
              <Field label="Next billing date" hint={f.access_mode === "trial" ? "Defaults to the day after the trial" : "Defaults to the start date"} optional>
                <Input type="date" value={f.next_due_date} onChange={(e) => set("next_due_date", e.target.value)} />
              </Field>
              <Field label="Grace period (days)"><Input type="number" min={0} max={90} value={f.grace_days} onChange={(e) => set("grace_days", e.target.value)} /></Field>
              <Field label="Grace extended to" optional><Input type="date" value={f.grace_until} onChange={(e) => set("grace_until", e.target.value)} /></Field>
            </div>
            <ToggleRow label="Auto-renew" hint="Issue the next period's invoice automatically 7 days before it starts" checked={f.auto_renew} onChange={(v) => set("auto_renew", v)} />
            <ToggleRow label="Suspend when overdue" hint="After the grace period, the business can only sign in and pay until the invoice is settled" checked={f.auto_suspend} onChange={(v) => set("auto_suspend", v)} />
          </section>
        )}
        <Field label="Internal notes" optional><Textarea rows={2} value={f.notes} onChange={(e) => set("notes", e.target.value)} /></Field>
      </div>
    </ResponsiveDialog>
  );
}

function IssueDialog({ tenantId, plan, onClose, onSaved }: { tenantId: string; plan: BillingPlan | null; onClose: () => void; onSaved: () => void }) {
  const [kind, setKind] = useState<"invoice" | "quotation">("invoice");
  const [category, setCategory] = useState(plan?.recurring ? "next_period" : plan?.model === "one_off" ? "one_off" : "other");
  const [amount, setAmount] = useState("");
  const [description, setDescription] = useState("");
  const [due, setDue] = useState("");
  const issue = useMutation({
    mutationFn: () =>
      api<{ number: string }>(`/platform/tenants/${tenantId}/billing-documents`, {
        body: { kind, category, description, amount: amount ? Number(amount) : null, due_date: due || null },
      }),
    onSuccess: (r) => {
      toast.success(`${t("Issued")} ${r.number}`);
      onSaved();
      onClose();
    },
    onError: (e) => toast.error(e),
  });
  return (
    <ResponsiveDialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Issue a quotation or invoice"
      footer={<Button onClick={() => issue.mutate()} disabled={issue.isPending}>{issue.isPending ? <Loader2 className="animate-spin" /> : t("Issue")}</Button>}
    >
      <div className="space-y-3">
        <div className="grid grid-cols-2 gap-3">
          <Field label="Document">
            <Select value={kind} onChange={(v) => setKind(v as typeof kind)}>
              <option value="invoice">{t("Invoice")}</option>
              <option value="quotation">{t("Quotation")}</option>
            </Select>
          </Field>
          <Field label="For">
            <Select value={category} onChange={setCategory}>
              {plan?.recurring && <option value="next_period">{t(plan.model === "subscription" ? "Next subscription period" : "Next maintenance period")}</option>}
              {plan?.model === "one_off" && <option value="one_off">{t("One-off fee")}</option>}
              <option value="other">{t("Other")}</option>
            </Select>
          </Field>
        </div>
        {category !== "next_period" && (
          <Field label="Amount" optional={category === "one_off"} hint={category === "one_off" && plan ? `${t("Default")}: ${moneyDoc(plan.one_off_amount, plan.currency)}` : undefined}>
            <Input type="number" inputMode="decimal" min={0} value={amount} onChange={(e) => setAmount(e.target.value)} />
          </Field>
        )}
        {category !== "next_period" && (
          <Field label="Description" optional={category === "one_off"}><Input value={description} onChange={(e) => setDescription(e.target.value)} /></Field>
        )}
        {category === "next_period" && plan && (
          <p className="rounded-lg bg-muted p-2.5 text-sm">
            {moneyDoc(plan.amount, plan.currency)} · {frequencyLabel(plan.frequency, plan.custom_months)} · {t("from")} {date(plan.next_due_date)}
          </p>
        )}
        {(category !== "next_period" || kind === "quotation") && (
          <Field label={kind === "quotation" ? "Valid until" : "Due date"} optional><Input type="date" min={todayIso()} value={due} onChange={(e) => setDue(e.target.value)} /></Field>
        )}
      </div>
    </ResponsiveDialog>
  );
}

function RecordPaymentDialog({ doc, onClose, onSaved }: { doc: BillingDocument; onClose: () => void; onSaved: () => void }) {
  const [method, setMethod] = useState("bank");
  const [reference, setReference] = useState("");
  const [paidOn, setPaidOn] = useState(todayIso());
  const [note, setNote] = useState("");
  const save = useMutation({
    mutationFn: () =>
      api<{ receipt_no: string }>(`/platform/billing/documents/${doc.id}/payments`, {
        body: { method, reference, note, paid_at: paidOn === todayIso() ? null : new Date(`${paidOn}T12:00:00`).toISOString() },
      }),
    onSuccess: (r) => {
      toast.success(`${t("Receipt")} ${r.receipt_no}`);
      onSaved();
      onClose();
    },
    onError: (e) => toast.error(e),
  });
  return (
    <ResponsiveDialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Record a payment"
      description={`${doc.number} — ${moneyDoc(doc.amount, doc.currency)}. ${t("For money received outside S'Shop (bank transfer, M-Pesa …).")}`}
      footer={<Button onClick={() => save.mutate()} disabled={save.isPending}>{save.isPending ? <Loader2 className="animate-spin" /> : t("Record payment")}</Button>}
    >
      <div className="space-y-3">
        <div className="grid grid-cols-2 gap-3">
          <Field label="Method">
            <Select value={method} onChange={setMethod}>
              {["bank", "mpesa", "cash", "other"].map((m) => <option key={m} value={m}>{t(METHOD_LABEL[m])}</option>)}
            </Select>
          </Field>
          <Field label="Date paid"><Input type="date" max={todayIso()} value={paidOn} onChange={(e) => setPaidOn(e.target.value)} /></Field>
        </div>
        <Field label="Reference" hint="Bank or M-Pesa transaction code"><Input value={reference} onChange={(e) => setReference(e.target.value)} /></Field>
        <Field label="Note" optional><Input value={note} onChange={(e) => setNote(e.target.value)} /></Field>
      </div>
    </ResponsiveDialog>
  );
}
