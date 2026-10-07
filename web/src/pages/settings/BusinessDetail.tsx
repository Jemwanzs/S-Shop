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
} from "@/lib/billing";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Pill, StatusBadge } from "@/components/Badges";
import { ConfirmDialog, Field, Select, ToggleRow } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Loading } from "@/components/Page";
import { Card, Fact, SettingsPage } from "./shared";
import type { TenantRow } from "./Businesses";

interface Detail {
  tenant: TenantRow;
  users: { id: string; name: string; email: string; phone: string; role: string; is_admin: boolean; is_active: boolean; last_login_at: string | null; failed_attempts: number; locked_until: string | null; failed_last_7d: number }[];
  branches: { id: string; name: string; code: string; location: string; latitude: number | null; longitude: number | null; is_active: boolean }[];
  onboarding: { contact_name: string; email: string; phone: string; location: string; business_type: string; branches: number | null; message: string; created_at: string; decided_at: string | null; decided_by_name: string | null } | null;
  status_history: { created_at: string; by_name: string | null; action: string; after: { reason?: string } | null }[];
  billing: { plan: (BillingPlan & { notes: string }) | null; summary: BillingSummary; documents: BillingDocument[]; payments: BillingPayment[]; paystack: boolean };
  is_home: boolean;
}

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

  return (
    <SettingsPage title={b.name} description={`/${b.slug}`}>
      <div className="-mt-3 flex flex-wrap items-center gap-2">
        <Button size="sm" variant="ghost" asChild><Link to="/settings/businesses"><ArrowLeft className="rtl:rotate-180" /> {t("Businesses")}</Link></Button>
        <Pill tone={active ? "success" : "danger"}>{t(active ? "Active" : "Deactivated")}</Pill>
        <Pill tone={STATUS_TONE[s.status]}>{t(STATUS_LABEL[s.status])}</Pill>
        {b.is_demo && <Pill tone="info">{t("Demo")}</Pill>}
        <span className="flex-1" />
        <Button size="sm" variant="outline" disabled={open.isPending} onClick={() => open.mutate()}><ArrowRightLeft /> {t("Open business")}</Button>
        {!d.is_home && (
          <Button size="sm" variant={active ? "destructive" : "default"} onClick={() => setStatusOpen(true)}>
            <Power /> {t(active ? "Deactivate" : "Reactivate")}
          </Button>
        )}
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
            <Button size="sm" variant="outline" onClick={() => setPlanOpen(true)}><Pencil /> {t("Plan")}</Button>
            <Button size="sm" onClick={() => setIssueOpen(true)}><FilePlus2 /> {t("Issue")}</Button>
          </div>
        }
      >
        <div className="grid grid-cols-2 gap-x-4 gap-y-3 py-2 sm:grid-cols-3">
          <Fact label={t("Billing model")}>{s.model ? t(s.model === "one_off" ? "One-off" : "Subscription") : t("Not set")}</Fact>
          {s.model === "one_off" && <Fact label={t("One-off")}>{moneyDoc(s.one_off_amount, s.currency)} · {t(s.one_off_status === "paid" ? "Paid" : "Pending")}</Fact>}
          {s.amount != null && <Fact label={t(s.model === "one_off" ? "Maintenance" : "Subscription")}>{moneyDoc(s.amount, s.currency)} · {frequencyLabel(s.frequency, s.custom_months)}</Fact>}
          <Fact label={t("Next due")}>{s.next_due ? date(s.next_due) : "—"}</Fact>
          <Fact label={t("Outstanding")}>{moneyDoc(s.outstanding, s.currency)}</Fact>
          <Fact label={t("Last payment")}>{s.last_payment_at ? `${moneyDoc(s.last_payment_amount, s.currency)} · ${date(s.last_payment_at)}` : "—"}</Fact>
          <Fact label={t("Paid to date")}>{moneyDoc(s.paid_total, s.currency)}</Fact>
          {s.period_end && <Fact label={t("Period covered")}>{periodLabel(s)}</Fact>}
          {d.billing.plan && <Fact label={t("Auto-renew")}>{t(d.billing.plan.auto_renew ? "On" : "Off")} · {t("grace")} {d.billing.plan.grace_days}d</Fact>}
        </div>
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
        <Card title="Status history">
          {d.status_history.map((h, i) => (
            <div key={i} className="flex items-center gap-2 py-2 text-sm">
              {h.action === "deactivate_business" ? <Ban className="h-4 w-4 text-destructive" /> : <Power className="h-4 w-4 text-success" />}
              <span className="flex-1">{t(h.action === "deactivate_business" ? "Deactivated" : "Reactivated")}{h.after?.reason && ` — ${h.after.reason}`}</span>
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
      {planOpen && <PlanDialog tenantId={id} plan={d.billing.plan} onClose={() => setPlanOpen(false)} onSaved={refresh} />}
      {issueOpen && <IssueDialog tenantId={id} plan={d.billing.plan} onClose={() => setIssueOpen(false)} onSaved={refresh} />}
      {payDoc && <RecordPaymentDialog doc={payDoc} onClose={() => setPayDoc(null)} onSaved={refresh} />}
    </SettingsPage>
  );
}

function PlanDialog({ tenantId, plan, onClose, onSaved }: { tenantId: string; plan: (BillingPlan & { notes: string }) | null; onClose: () => void; onSaved: () => void }) {
  const [f, setF] = useState({
    model: plan?.model ?? "subscription",
    currency: plan?.currency ?? "KES",
    one_off_amount: plan ? String(plan.one_off_amount) : "",
    maintenance: plan ? plan.model === "one_off" && plan.recurring : false,
    amount: plan && plan.recurring ? String(plan.amount) : "",
    frequency: plan?.frequency ?? "monthly",
    custom_months: String(plan?.custom_months ?? 1),
    start_date: plan?.start_date ?? todayIso(),
    next_due_date: plan?.next_due_date ?? "",
    grace_days: String(plan?.grace_days ?? 7),
    auto_renew: plan?.auto_renew ?? true,
    notes: plan?.notes ?? "",
  });
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => setF((x) => ({ ...x, [k]: v }));
  const recurring = f.model === "subscription" || f.maintenance;
  const save = useMutation({
    mutationFn: () =>
      api(`/platform/tenants/${tenantId}/billing-plan`, {
        method: "PUT",
        body: {
          model: f.model, currency: f.currency, one_off_amount: Number(f.one_off_amount || 0), maintenance: f.maintenance,
          amount: Number(f.amount || 0), frequency: f.frequency, custom_months: Number(f.custom_months || 1),
          start_date: recurring ? f.start_date || null : null, next_due_date: recurring ? f.next_due_date || null : null,
          grace_days: Number(f.grace_days || 0), auto_renew: f.auto_renew, notes: f.notes,
        },
      }),
    onSuccess: () => {
      toast.success("Billing plan saved");
      onSaved();
      onClose();
    },
    onError: (e) => toast.error(e),
  });
  const what = f.model === "subscription" ? "Subscription" : "Maintenance";
  return (
    <ResponsiveDialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Billing plan"
      footer={<Button onClick={() => save.mutate()} disabled={save.isPending}>{save.isPending ? <Loader2 className="animate-spin" /> : t("Save plan")}</Button>}
    >
      <div className="space-y-3">
        <div className="grid grid-cols-2 gap-3">
          <Field label="Billing model">
            <Select value={f.model} onChange={(v) => set("model", v as typeof f.model)}>
              <option value="subscription">{t("Subscription")}</option>
              <option value="one_off">{t("One-off")}</option>
            </Select>
          </Field>
          <Field label="Currency"><Input value={f.currency} maxLength={3} onChange={(e) => set("currency", e.target.value.toUpperCase())} /></Field>
        </div>
        {f.model === "one_off" && (
          <>
            <Field label="One-off amount"><Input type="number" inputMode="decimal" min={0} value={f.one_off_amount} onChange={(e) => set("one_off_amount", e.target.value)} /></Field>
            <ToggleRow label="Maintenance fee required" checked={f.maintenance} onChange={(v) => set("maintenance", v)} />
          </>
        )}
        {recurring && (
          <>
            <div className="grid grid-cols-2 gap-3">
              <Field label={`${what} amount`}><Input type="number" inputMode="decimal" min={0} value={f.amount} onChange={(e) => set("amount", e.target.value)} /></Field>
              <Field label="Frequency">
                <Select value={f.frequency} onChange={(v) => set("frequency", v)}>
                  {FREQUENCIES.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}
                </Select>
              </Field>
            </div>
            {f.frequency === "custom" && (
              <Field label="Every (months)"><Input type="number" min={1} max={60} value={f.custom_months} onChange={(e) => set("custom_months", e.target.value)} /></Field>
            )}
            <div className="grid grid-cols-2 gap-3">
              <Field label="Start date"><Input type="date" value={f.start_date} onChange={(e) => set("start_date", e.target.value)} /></Field>
              <Field label="Next due date" hint="Defaults to the start date"><Input type="date" value={f.next_due_date} onChange={(e) => set("next_due_date", e.target.value)} /></Field>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Grace period (days)"><Input type="number" min={0} max={90} value={f.grace_days} onChange={(e) => set("grace_days", e.target.value)} /></Field>
            </div>
            <ToggleRow label="Auto-renew" hint="Issue the next period's invoice automatically 7 days before it starts" checked={f.auto_renew} onChange={(v) => set("auto_renew", v)} />
          </>
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
