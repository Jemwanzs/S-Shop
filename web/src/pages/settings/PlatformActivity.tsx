import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { count, dateTime } from "@/lib/format";
import { t } from "@/lib/i18n";
import { Pill, type Tone } from "@/components/Badges";
import { Chip, PeriodFilter, type PeriodValue } from "@/components/Filters";
import { Select } from "@/components/Form";
import { Pager } from "@/components/DataList";
import { Loading } from "@/components/Page";
import { Card, SettingsPage } from "./shared";
import type { TenantRow } from "./Businesses";

interface ActivityRow {
  id: string;
  created_at: string;
  tenant_id: string;
  business: string;
  user_id: string | null;
  user_name: string | null;
  user_email: string | null;
  branch_name: string | null;
  module: string;
  action: string;
  entity_type: string;
  after: Record<string, unknown> | null;
  ip: string;
}

interface ActivityResponse {
  items: ActivityRow[];
  total: number;
  totals: Record<string, number>;
  activities: string[];
}

const LABEL: Record<string, string> = {
  login: "Sign-ins",
  login_failed: "Failed sign-ins",
  sale: "Sales",
  stock_count: "Stock counts & adjustments",
  stock_receive: "Stock received",
  transfer: "Transfers",
  pin_reset: "PIN resets",
  platform: "Platform actions",
  support: "Support access",
  billing: "Billing",
};

const ACTION_LABEL: Record<string, [string, Tone]> = {
  "auth.login": ["Signed in", "success"],
  "auth.login_failed": ["Failed sign-in", "danger"],
  "sales.create": ["Sale", "primary"],
  "sales.offline_sync": ["Offline sale synced", "primary"],
  "sales.exchange": ["Exchange", "primary"],
  "stock.adjust": ["Stock count / adjustment", "info"],
  "stock.receive": ["Stock received", "info"],
  "transfers.create": ["Transfer created", "info"],
  "transfers.submit": ["Transfer submitted", "info"],
  "transfers.dispatch": ["Transfer dispatched", "info"],
  "transfers.receive": ["Transfer received", "info"],
  "transfers.cancel": ["Transfer cancelled", "warning"],
  "platform.reset_pin": ["PIN reset by platform", "warning"],
  "platform.open_business": ["Opened by platform", "neutral"],
  "platform.deactivate_business": ["Business deactivated", "danger"],
  "platform.reactivate_business": ["Business reactivated", "success"],
  "platform.approve_access": ["Access approved", "success"],
  "platform.add_business": ["Business added to tenant", "success"],
  "platform.move_business": ["Business moved to tenant", "neutral"],
  "platform.update_tenant": ["Tenant updated", "neutral"],
  "platform.support_requested": ["Support access requested", "warning"],
  "platform.support_approved": ["Support access approved", "success"],
  "platform.support_denied": ["Support access declined", "danger"],
  "platform.support_started": ["Support session started", "warning"],
  "platform.support_ended": ["Support session ended", "neutral"],
  "platform.support_revoked": ["Support session revoked", "danger"],
  "billing.payment_received": ["Payment received", "success"],
  "billing.payment_started": ["Payment started", "neutral"],
  "billing.invoice_issued": ["Invoice issued", "neutral"],
  "billing.quotation_issued": ["Quotation issued", "neutral"],
  "billing.quotation_accepted": ["Quotation accepted", "success"],
  "billing.document_void": ["Document voided", "warning"],
  "billing.plan_updated": ["Billing plan updated", "neutral"],
};

const LIMIT = 50;

/** Platform owner: activity across businesses from the audit trail (roadmap 35). */
export function PlatformActivity() {
  return (
    <SettingsPage title="Activity" description="Sign-ins, failed sign-ins, sales, stock counts, transfers, support access and platform actions across every business, from the audit trail.">
      <ActivityFeed />
    </SettingsPage>
  );
}

/** The activity feed; with `accountId`, one tenant's businesses only (roadmap 73). */
export function ActivityFeed({ accountId, businesses }: { accountId?: string; businesses?: { id: string; name: string }[] }) {
  const [tenant, setTenant] = useState("");
  const [branch, setBranch] = useState("");
  const [user, setUser] = useState("");
  const [activity, setActivity] = useState("all");
  const [period, setPeriod] = useState<PeriodValue>({ period: "week" });
  const [offset, setOffset] = useState(0);

  const tenants = useQuery({ queryKey: ["platform-tenants"], queryFn: () => api<{ items: TenantRow[] }>("/platform/tenants"), enabled: !businesses });
  const choices = businesses ?? tenants.data?.items ?? [];
  const detail = useQuery({
    queryKey: ["platform-tenant", tenant],
    queryFn: () => api<{ users: { id: string; name: string }[]; branches: { id: string; name: string }[] }>(`/platform/tenants/${tenant}`),
    enabled: !!tenant,
  });
  const params = new URLSearchParams();
  if (accountId) params.set("account_id", accountId);
  if (tenant) params.set("tenant_id", tenant);
  if (branch) params.set("branch_id", branch);
  if (user) params.set("user_id", user);
  if (activity !== "all") params.set("activity", activity);
  if (period.period) params.set("period", period.period);
  if (period.from) params.set("from", period.from);
  if (period.to) params.set("to", period.to);
  params.set("limit", String(LIMIT));
  params.set("offset", String(offset));
  const q = useQuery({ queryKey: ["platform-activity", params.toString()], queryFn: () => api<ActivityResponse>(`/platform/activity?${params}`) });
  const reset = (fn: () => void) => {
    fn();
    setOffset(0);
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2.5">
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          <Select value={tenant} placeholder={t("All businesses")} label={t("Business")} onChange={(v) => reset(() => { setTenant(v); setBranch(""); setUser(""); })}>
            <option value="">{t("All businesses")}</option>
            {choices.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
          </Select>
          <Select value={branch} placeholder={t("All branches")} label={t("Branch")} disabled={!tenant} onChange={(v) => reset(() => setBranch(v))}>
            <option value="">{t("All branches")}</option>
            {detail.data?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
          </Select>
          <Select value={user} placeholder={t("All users")} label={t("User")} disabled={!tenant} onChange={(v) => reset(() => setUser(v))}>
            <option value="">{t("All users")}</option>
            {detail.data?.users.map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
          </Select>
        </div>
        <PeriodFilter value={period} onChange={(v) => reset(() => setPeriod(v))} />
        <div className="scrollbar-none -mx-3.5 flex gap-1.5 overflow-x-auto px-3.5 md:mx-0 md:flex-wrap md:px-0">
          <Chip active={activity === "all"} onClick={() => reset(() => setActivity("all"))}>{t("All")}</Chip>
          {(q.data?.activities ?? Object.keys(LABEL)).map((a) => (
            <Chip key={a} active={activity === a} onClick={() => reset(() => setActivity(a))}>
              {t(LABEL[a] ?? a)}{q.data && activity === "all" ? <span className="num opacity-70">{count(q.data.totals[a] ?? 0)}</span> : null}
            </Chip>
          ))}
        </div>
      </div>

      {q.isLoading ? (
        <Loading />
      ) : (
        <Card>
          {q.data?.items.length === 0 && <p className="py-3 text-sm text-muted-foreground">{t("No activity for these filters.")}</p>}
          {q.data?.items.map((r) => {
            const [label, tone] = ACTION_LABEL[`${r.module}.${r.action}`] ?? [`${r.module} · ${r.action}`, "neutral" as Tone];
            const extra = r.after && typeof r.after === "object" ? (r.after.reason ?? r.after.receipt_no ?? r.after.number ?? r.after.invoice ?? "") : "";
            return (
              <div key={r.id} className="flex items-start gap-3 py-2.5 text-sm">
                <Pill tone={tone} className="mt-0.5 shrink-0">{t(label)}</Pill>
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">{r.business}{r.branch_name && <span className="text-muted-foreground"> · {r.branch_name}</span>}</p>
                  <p className="truncate text-xs text-muted-foreground">
                    {r.user_name ?? t("System")}{r.user_email && ` (${r.user_email})`}{r.ip && ` · ${r.ip}`}{extra ? ` · ${String(extra)}` : ""}
                  </p>
                </div>
                <span className="num shrink-0 text-xs text-muted-foreground">{dateTime(r.created_at)}</span>
              </div>
            );
          })}
        </Card>
      )}
      {q.data && q.data.total > LIMIT && <Pager total={q.data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
    </div>
  );
}
