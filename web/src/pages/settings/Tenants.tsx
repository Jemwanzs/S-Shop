/** Platform → Tenants (roadmap 70–73): every customer account with its businesses, people, billing, services,
 * activity and access — managed without entering any business. Entering one is a separate, audited step
 * (administrator sign-in or support access). */
import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ArrowRightLeft, Building2, ChevronRight, CircleOff, Globe, LifeBuoy, Plus, Power, Receipt, Users2 } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import type { Profile } from "@/lib/types";
import { toast } from "@/lib/toast";
import { ago, count, date, dateTime, money } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { ScrollRow } from "@/components/ScrollRow";
import { STATUS_LABEL, STATUS_TONE, type BillingStatus } from "@/lib/billing";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton } from "@/components/ActionButton";
import { Pill, type Tone } from "@/components/Badges";
import { Field, Select } from "@/components/Form";
import { Loading } from "@/components/Page";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { OpenBusinessDialog, type OpenTarget } from "@/components/OpenBusinessDialog";
import { Card, Fact, SettingsPage } from "./shared";
import { ActivityFeed } from "./PlatformActivity";
import type { TenantRow } from "./Businesses";

export interface AccountRow {
  id: string;
  name: string;
  notes: string;
  created_at: string;
  primary_user_id: string | null;
  admin_name: string | null;
  admin_email: string | null;
  admin_phone: string | null;
  businesses: number;
  branches: number;
  users: number;
  deactivated: number;
  platform_owned: boolean;
  is_demo: boolean;
  last_login_at: string | null;
  status: "active" | "trial" | "grace" | "suspended" | "deactivated" | "platform";
  billing_status: BillingStatus | "";
  next_due: string | null;
  outstanding: string;
  business_names: string[];
}

export const TENANT_STATUS: Record<AccountRow["status"], [string, Tone]> = {
  active: ["Active", "success"],
  trial: ["Trial", "info"],
  grace: ["Grace period", "warning"],
  suspended: ["Suspended", "danger"],
  deactivated: ["Deactivated", "danger"],
  platform: ["Platform owned", "primary"],
};

const SESSION_TONE: Record<string, Tone> = { requested: "warning", approved: "info", active: "success", ended: "neutral", denied: "danger", expired: "neutral" };
export const SESSION_LABEL: Record<string, string> = { requested: "Awaiting approval", approved: "Approved", active: "Active", ended: "Ended", denied: "Declined", expired: "Expired" };

export interface SessionRow {
  id: string;
  tenant_id: string;
  business: string;
  user_name: string;
  reason: string;
  scope: "view" | "full";
  minutes: number;
  status: string;
  requested_at: string;
  decided_at: string | null;
  decided_by_name: string | null;
  started_at: string | null;
  expires_at: string | null;
  ended_at: string | null;
  ended_by_name: string | null;
  end_note: string;
}

export function TenantsSettings() {
  const q = useQuery({ queryKey: ["platform-accounts"], queryFn: () => api<{ items: AccountRow[]; home_account_id: string }>("/platform/accounts") });
  const [filter, setFilter] = useState("");
  const items = (q.data?.items ?? []).filter((a) => !filter || `${a.name} ${a.admin_email ?? ""} ${a.business_names.join(" ")}`.toLowerCase().includes(filter.toLowerCase()));
  return (
    <SettingsPage title="Tenants" description="Every customer account: its businesses, branches, people, billing and services. Manage them here without entering their workspaces.">
      <Input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={t("Search tenants, businesses or emails")} />
      {q.isLoading ? <Loading /> : (
        <div className="grid gap-3 md:grid-cols-2">
          {items.map((a) => {
            const [label, tone] = TENANT_STATUS[a.status];
            return (
              <Link key={a.id} to={`/settings/tenants/${a.id}`} className="surface block space-y-2 p-3.5 transition-shadow hover:shadow-lift">
                <div className="flex items-start gap-2.5">
                  <span className="rounded-lg bg-primary/10 p-2 text-primary"><Users2 className="h-4 w-4" /></span>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5 font-semibold">
                      <span className="truncate">{a.name}</span>
                      {a.id === q.data?.home_account_id && <Pill>{t("Yours")}</Pill>}
                      {a.is_demo && <Pill tone="info">{t("Demo")}</Pill>}
                    </div>
                    <p className="truncate text-xs text-muted-foreground">{a.admin_name ?? t("No primary administrator")}{a.admin_email && ` · ${a.admin_email}`}{a.admin_phone && ` · ${a.admin_phone}`}</p>
                  </div>
                  <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground rtl:rotate-180" />
                </div>
                <div className="flex flex-wrap items-center gap-1.5">
                  <Pill tone={tone}>{t(label)}</Pill>
                  {a.billing_status && a.status !== "platform" && <Pill tone={STATUS_TONE[a.billing_status as BillingStatus]}>{t(STATUS_LABEL[a.billing_status as BillingStatus])}</Pill>}
                  {a.next_due && <span className="num text-xs text-muted-foreground">{t("Next due")} {date(a.next_due)}</span>}
                </div>
                <p className="num text-xs text-muted-foreground">
                  {count(a.businesses)} {t(a.businesses === 1 ? "business" : "businesses")} · {count(a.branches)} {t("branches")} · {count(a.users)} {t("users")}
                  {a.last_login_at && ` · ${t("last sign-in")} ${ago(a.last_login_at)}`}
                </p>
              </Link>
            );
          })}
          {!items.length && <p className="text-sm text-muted-foreground">{t("No tenants match.")}</p>}
        </div>
      )}
    </SettingsPage>
  );
}

interface AccountDetail {
  account: AccountRow;
  businesses: TenantRow[];
  users: { id: string; tenant_id: string; business: string; name: string; email: string; phone: string; role: string; is_admin: boolean; is_active: boolean; linked: boolean; last_login_at: string | null }[];
  sessions: SessionRow[];
  websites: Record<string, { status: string; domain?: { domain: string; status: string } | null }>;
  policies: Record<string, string>;
  home_tenant_id: string;
  accounts: { id: string; name: string }[];
}

const TABS = [["overview", "Overview"], ["businesses", "Businesses & branches"], ["users", "Users"], ["billing", "Billing"], ["services", "Website services"],
  ["activity", "Activity & security"], ["access", "Access & status"]] as const;

export function TenantDetail() {
  const { id = "" } = useParams();
  const qc = useQueryClient();
  const key = ["platform-account", id];
  const q = useQuery({ queryKey: key, queryFn: () => api<AccountDetail>(`/platform/accounts/${id}`) });
  const [tab, setTab] = useState<(typeof TABS)[number][0]>("overview");
  const [openTarget, setOpenTarget] = useState<OpenTarget | null>(null);
  const [edit, setEdit] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [statusOpen, setStatusOpen] = useState(false);
  const refresh = () => {
    qc.invalidateQueries({ queryKey: key });
    qc.invalidateQueries({ queryKey: ["platform-accounts"] });
    qc.invalidateQueries({ queryKey: ["platform-tenants"] });
  };
  const endSession = useMutation({
    mutationFn: (sid: string) => api(`/platform/support/${sid}/end`, { method: "POST" }),
    onSuccess: () => { toast.success("Support session ended"); refresh(); },
    onError: (e) => toast.error(e),
  });
  if (q.isLoading || !q.data) return <Loading />;
  const d = q.data;
  const a = d.account;
  const [label, tone] = TENANT_STATUS[a.status];
  const openAs = (b: TenantRow) => setOpenTarget({ id: b.id, name: b.name, tenant: a.name, admin_email: b.admin_email, platform_owned: b.ownership === "platform" });

  return (
    <SettingsPage title={a.name} description={`${count(a.businesses)} ${t(a.businesses === 1 ? "business" : "businesses")} · ${t("since")} ${date(a.created_at)}`}>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="ghost" asChild><Link to="/settings/tenants"><ArrowLeft className="rtl:rotate-180" /> {t("Tenants")}</Link></Button>
        <Pill tone={tone}>{t(label)}</Pill>
        {a.billing_status && a.status !== "platform" && <Pill tone={STATUS_TONE[a.billing_status as BillingStatus]}>{t(STATUS_LABEL[a.billing_status as BillingStatus])}</Pill>}
      </div>
      <ScrollRow active={tab}>
        {TABS.map(([k, l]) => (
          <button key={k} type="button" onClick={() => setTab(k)} data-active={tab === k ? "true" : undefined} aria-current={tab === k ? "page" : undefined}
            className={cn("h-8 shrink-0 whitespace-nowrap rounded-full border px-3 text-xs", tab === k ? "border-primary bg-primary text-primary-foreground" : "bg-card")}>{t(l)}</button>
        ))}
      </ScrollRow>

      {tab === "overview" && (
        <>
          <Card title="Tenant profile" action={<Button size="sm" variant="outline" onClick={() => setEdit(true)}>{t("Edit")}</Button>}>
            <Fact label="Primary administrator">{a.admin_name ?? "—"}</Fact>
            <Fact label="Email">{a.admin_email ?? "—"}</Fact>
            <Fact label="Mobile">{a.admin_phone || "—"}</Fact>
            <Fact label="Registered">{date(a.created_at)}</Fact>
            <Fact label="Last sign-in">{a.last_login_at ? dateTime(a.last_login_at) : "—"}</Fact>
            {a.notes && <p className="whitespace-pre-wrap py-2 text-sm text-muted-foreground">{a.notes}</p>}
          </Card>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            {([["Businesses", a.businesses], ["Branches", a.branches], ["Users", a.users], ["Outstanding", null]] as const).map(([l, v]) => (
              <div key={l} className="surface p-3">
                <p className="text-xs text-muted-foreground">{t(l)}</p>
                <p className="num text-lg font-semibold">{v === null ? money(a.outstanding) : count(v)}</p>
              </div>
            ))}
          </div>
        </>
      )}

      {tab === "businesses" && (
        <Card title="Businesses" action={<Button size="sm" variant="outline" onClick={() => setAddOpen(true)}><Plus /> {t("Add business")}</Button>}>
          {d.businesses.map((b) => (
            <div key={b.id} className="flex items-center gap-3 py-3">
              <span className="rounded-lg bg-primary/10 p-2 text-primary"><Building2 className="h-4 w-4" /></span>
              <Link to={`/settings/businesses/${b.id}`} className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5 font-medium">
                  <span className="truncate">{b.name}</span>
                  {b.status !== "active" && <Pill tone="danger">{t("Deactivated")}</Pill>}
                </div>
                <p className="num truncate text-xs text-muted-foreground">{count(b.branches)} {t("branches")} · {count(b.users)} {t("users")} · {count(b.sales)} {t("sales")}{b.last_sale_at && ` · ${t("last sale")} ${ago(b.last_sale_at)}`}</p>
              </Link>
              {b.id !== d.home_tenant_id && <Button size="sm" variant="outline" aria-label={t("Open business")} onClick={() => openAs(b)}><ArrowRightLeft /> <span className="hidden sm:inline">{t("Open business")}</span></Button>}
            </div>
          ))}
          <p className="py-2 text-xs text-muted-foreground">{t("Each business keeps its own branches, products, stock, sales, orders, expenses and reports. Its administrators give people access to the tenant's other businesses in Settings → Users.")}</p>
        </Card>
      )}

      {tab === "users" && (
        <Card title="People">
          {d.users.map((u) => (
            <div key={u.id} className="flex items-center gap-3 py-2.5 text-sm">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5 font-medium">
                  <span className="truncate">{u.name}</span>
                  {u.is_admin && <Pill tone="primary">{t("Administrator")}</Pill>}
                  {u.linked && <Pill tone="info">{t("Linked")}</Pill>}
                  {!u.is_active && <Pill tone="danger">{t("Inactive")}</Pill>}
                </div>
                <p className="truncate text-xs text-muted-foreground">{u.business} · {u.role} · {u.email}</p>
              </div>
              <span className="shrink-0 text-xs text-muted-foreground">{u.last_login_at ? ago(u.last_login_at) : t("never")}</span>
            </div>
          ))}
        </Card>
      )}

      {tab === "billing" && (
        <Card title="Billing by business">
          {d.businesses.map((b) => (
            <div key={b.id} className="flex items-center gap-3 py-3 text-sm">
              <Receipt className="h-4 w-4 shrink-0 text-primary" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">{b.name}</p>
                <p className="num truncate text-xs text-muted-foreground">
                  {t(STATUS_LABEL[b.billing.status as BillingStatus] ?? b.billing.status)}
                  {b.billing.package && ` · ${b.billing.package}`}
                  {b.billing.next_due && ` · ${t("next due")} ${date(b.billing.next_due)}`}
                  {Number(b.billing.outstanding) > 0 && ` · ${t("outstanding")} ${money(b.billing.outstanding)}`}
                </p>
              </div>
              <Button size="sm" variant="outline" asChild><Link to={`/settings/businesses/${b.id}#billing`}>{t("Manage billing")}</Link></Button>
            </div>
          ))}
          <p className="py-2 text-xs text-muted-foreground">{t("Plans, pricing, discounts, tax, trials, invoices, quotations, payments and Paystack are set per business; the tenant's administrators see theirs in Settings → Billing.")}</p>
        </Card>
      )}

      {tab === "services" && (
        <Card title="Website services">
          {d.businesses.map((b) => {
            const w = d.websites[b.id];
            return (
              <div key={b.id} className="flex items-center gap-3 py-3 text-sm">
                <Globe className="h-4 w-4 shrink-0 text-primary" />
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">{b.name}</p>
                  <p className="truncate text-xs text-muted-foreground">{t(w?.status ?? "none")}{w?.domain && ` · ${w.domain.domain} (${t(w.domain.status)})`}</p>
                </div>
                <Button size="sm" variant="outline" asChild><Link to={`/settings/businesses/${b.id}#website`}>{t("Manage services")}</Link></Button>
              </div>
            );
          })}
        </Card>
      )}

      {tab === "activity" && (
        <>
          <SupportSessions sessions={d.sessions} onEnd={(sid) => endSession.mutate(sid)} />
          <ActivityFeed accountId={a.id} businesses={d.businesses.map((b) => ({ id: b.id, name: b.name }))} />
        </>
      )}

      {tab === "access" && (
        <>
          <Card title="Tenant status" action={a.status !== "platform" && (
            <Button size="sm" variant={a.status === "deactivated" ? "default" : "outline"} onClick={() => setStatusOpen(true)}>
              {a.status === "deactivated" ? <><Power /> {t("Reactivate")}</> : <><CircleOff /> {t("Deactivate")}</>}
            </Button>
          )}>
            <p className="py-2 text-sm text-muted-foreground">
              {a.status === "platform" ? t("Platform-owned tenants are never deactivated or billed.")
                : t("Deactivating ends every session, blocks sign-in, transactions, the ordering link and the website of all its businesses. All data, billing and audit history are kept.")}
            </p>
          </Card>
          <Card title="Support access policy">
            {d.businesses.map((b) => (
              <Fact key={b.id} label={b.name}>{t(d.policies[b.id] === "approval" ? "Approval required" : "Administrators notified")}</Fact>
            ))}
            <p className="py-2 text-xs text-muted-foreground">{t("Each business's administrators choose this in Settings → Support access. The platform cannot change it.")}</p>
          </Card>
        </>
      )}

      <OpenBusinessDialog target={openTarget} onClose={() => { setOpenTarget(null); refresh(); }} />
      {edit && <EditTenant d={d} onClose={() => setEdit(false)} onSaved={refresh} />}
      {addOpen && <AddBusiness id={a.id} onClose={() => setAddOpen(false)} onSaved={refresh} />}
      {statusOpen && <TenantStatus a={a} onClose={() => setStatusOpen(false)} onSaved={refresh} />}
    </SettingsPage>
  );
}

export function SupportSessions({ sessions, onEnd, tenantSide, onDecide }: { sessions: SessionRow[]; onEnd?: (id: string) => void; tenantSide?: boolean; onDecide?: (id: string, d: "approve" | "deny" | "revoke") => void }) {
  return (
    <Card title="Support sessions">
      {!sessions.length && <p className="py-2 text-sm text-muted-foreground">{t("No support access so far.")}</p>}
      {sessions.map((s) => (
        <div key={s.id} className="space-y-1 py-2.5 text-sm">
          <div className="flex flex-wrap items-center gap-1.5">
            <LifeBuoy className="h-3.5 w-3.5 text-primary" />
            <span className="font-medium">{tenantSide ? s.user_name : s.business}</span>
            <Pill tone={SESSION_TONE[s.status] ?? "neutral"}>{t(SESSION_LABEL[s.status] ?? s.status)}</Pill>
            <Pill>{t(s.scope === "view" ? "View only" : "Full access")}</Pill>
            <span className="num ms-auto text-xs text-muted-foreground">{dateTime(s.requested_at)}</span>
          </div>
          <p className="text-xs text-muted-foreground">
            {s.reason} · {s.minutes} min
            {s.expires_at && s.status === "active" && ` · ${t("until")} ${dateTime(s.expires_at)}`}
            {s.decided_by_name && ` · ${t("decided by")} ${s.decided_by_name}`}
            {s.end_note && ` · ${s.end_note}`}
          </p>
          <div className="flex flex-wrap gap-2">
            {tenantSide && s.status === "requested" && onDecide && (
              <>
                <ActionButton size="sm" online onAction={async () => onDecide(s.id, "approve")}>{t("Approve")}</ActionButton>
                <ActionButton size="sm" variant="outline" online onAction={async () => onDecide(s.id, "deny")}>{t("Decline")}</ActionButton>
              </>
            )}
            {tenantSide && ["approved", "active"].includes(s.status) && onDecide && (
              <ActionButton size="sm" variant="destructive" online onAction={async () => onDecide(s.id, "revoke")}>{t("End access now")}</ActionButton>
            )}
            {!tenantSide && ["requested", "approved", "active"].includes(s.status) && onEnd && (
              <Button size="sm" variant="outline" onClick={() => onEnd(s.id)}>{t(s.status === "active" ? "End session" : "Withdraw")}</Button>
            )}
            {!tenantSide && s.status === "approved" && <StartApproved id={s.id} />}
          </div>
        </div>
      ))}
    </Card>
  );
}

function StartApproved({ id }: { id: string }) {
  const [pin, setPin] = useState("");
  const [open, setOpen] = useState(false);
  const { switchBusiness } = useSession();
  return (
    <>
      <Button size="sm" onClick={() => setOpen(true)}>{t("Start session")}</Button>
      <ResponsiveDialog open={open} onOpenChange={setOpen} title="Start approved support session" description="Confirm with your PIN."
        footer={<ActionButton online blockedBy={[!pin && "Enter your PIN"]} onAction={async () => {
          const r = await api<{ token: string; profile: Profile }>(`/platform/support/${id}/start`, { body: { pin } });
          switchBusiness(r.token, r.profile);
          window.location.assign("/");
        }}>{t("Start session")}</ActionButton>}>
        <Field label="Your PIN"><Input type="password" value={pin} onChange={(e) => setPin(e.target.value)} autoComplete="current-password" /></Field>
      </ResponsiveDialog>
    </>
  );
}


function EditTenant({ d, onClose, onSaved }: { d: AccountDetail; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState(d.account.name);
  const [notes, setNotes] = useState(d.account.notes);
  const [primary, setPrimary] = useState(d.account.primary_user_id ?? "");
  const admins = d.users.filter((u) => u.is_admin && u.is_active && !u.linked);
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Edit tenant"
      footer={<ActionButton online blockedBy={[!name.trim() && "Enter the tenant's name"]} onAction={async () => {
        await api(`/platform/accounts/${d.account.id}`, { method: "PUT", body: { name: name.trim(), notes, primary_user_id: primary || null } });
        toast.success("Tenant saved");
        onSaved();
        onClose();
      }}>{t("Save")}</ActionButton>}>
      <Field label="Tenant name"><Input value={name} onChange={(e) => setName(e.target.value)} maxLength={120} /></Field>
      <Field label="Primary administrator">
        <Select value={primary} onChange={setPrimary}>
          {admins.map((u) => <option key={u.id} value={u.id}>{u.name} · {u.business}</option>)}
        </Select>
      </Field>
      <Field label="Notes" optional><Textarea value={notes} onChange={(e) => setNotes(e.target.value)} rows={3} maxLength={2000} /></Field>
    </ResponsiveDialog>
  );
}

function AddBusiness({ id, onClose, onSaved }: { id: string; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState("");
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Add business" description="A separate business for this tenant: its own branches, products, stock, sales and reports. The primary administrator manages it with their existing sign-in."
      footer={<ActionButton online blockedBy={[name.trim().length < 2 && "Enter the business name"]} onAction={async () => {
        await api(`/platform/accounts/${id}/businesses`, { body: { name: name.trim() } });
        toast.success("Business added");
        onSaved();
        onClose();
      }}>{t("Add business")}</ActionButton>}>
      <Field label="Business name"><Input value={name} onChange={(e) => setName(e.target.value)} maxLength={120} autoFocus /></Field>
    </ResponsiveDialog>
  );
}

function TenantStatus({ a, onClose, onSaved }: { a: AccountRow; onClose: () => void; onSaved: () => void }) {
  const reactivate = a.status === "deactivated";
  const [reason, setReason] = useState("");
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title={reactivate ? "Reactivate tenant" : "Deactivate tenant"}
      description={reactivate ? "Sign-in and transactions return for every business, according to its billing and services." : "Every business of this tenant: sessions end, sign-in, transactions, ordering and websites stop. Data is kept."}
      footer={<ActionButton online variant={reactivate ? "default" : "destructive"} blockedBy={[!reactivate && reason.trim().length < 5 && "Give the reason"]} onAction={async () => {
        await api(`/platform/accounts/${a.id}/status`, { body: { status: reactivate ? "active" : "deactivated", reason: reason.trim() } });
        toast.success(reactivate ? "Tenant reactivated" : "Tenant deactivated");
        onSaved();
        onClose();
      }}>{t(reactivate ? "Reactivate" : "Deactivate")}</ActionButton>}>
      <Field label="Reason" optional={reactivate}><Textarea value={reason} onChange={(e) => setReason(e.target.value)} rows={2} maxLength={500} /></Field>
    </ResponsiveDialog>
  );
}
