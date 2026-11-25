/** Roadmap 182: the confidential Platform Security Audit Trail — every platform support session (who really worked
 * in which business, why, with what access, how they were authenticated, when, and how it ended) and every change
 * attempted in it. Platform administrators only; append-only on the server (nothing can change or remove a record). */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Lock, ShieldCheck } from "lucide-react";
import { api } from "@/lib/api";
import { count, dateTime } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Pill, type Tone } from "@/components/Badges";
import { Chip, PeriodFilter, type PeriodValue } from "@/components/Filters";
import { Select } from "@/components/Form";
import { Pager } from "@/components/DataList";
import { Loading } from "@/components/Page";
import { ScrollRow } from "@/components/ScrollRow";
import { Card, SettingsPage } from "./shared";
import type { TenantRow } from "./Businesses";

interface SecEvent {
  id: number; at: string; event: string; session_id: string | null; tenant_id: string | null; tenant_name: string;
  platform_user: string; platform_email: string; scope: string; reason: string; method: string; path: string; status: number | null;
  detail: Record<string, unknown>; ip: string; user_agent: string;
}
interface SecSession {
  id: string; tenant_id: string; business: string; platform_user: string; platform_email: string; reason: string; scope: "view" | "full";
  minutes: number; status: string; requested_at: string; started_at: string | null; expires_at: string | null; ended_at: string | null;
  end_note: string; changes: number; refused: number; reads: number;
}
interface SecResponse { items: SecEvent[]; total: number; sessions: SecSession[] }

const EVENT: Record<string, [string, Tone]> = {
  session_requested: ["Access requested", "warning"],
  request_approved: ["Approved by the business", "success"],
  request_declined: ["Declined by the business", "danger"],
  request_withdrawn: ["Request withdrawn", "neutral"],
  approval_lapsed: ["Approval lapsed", "neutral"],
  session_started: ["Session started", "warning"],
  session_ended: ["Session ended", "neutral"],
  session_revoked: ["Revoked by the business", "danger"],
  session_expired: ["Time limit reached", "neutral"],
  session_signed_out: ["Signed out", "neutral"],
  session_idle: ["Ended after inactivity", "neutral"],
  session_replaced: ["Replaced by a new request", "neutral"],
  change: ["Change", "primary"],
  change_refused: ["Change refused", "danger"],
};
const STATUS: Record<string, Tone> = { active: "warning", requested: "info", approved: "info", ended: "neutral", expired: "neutral", denied: "danger" };
const LIMIT = 50;

export function PlatformSecurityAudit() {
  const [tenant, setTenant] = useState("");
  const [kind, setKind] = useState("all");
  const [session, setSession] = useState("");
  const [period, setPeriod] = useState<PeriodValue>({ period: "week" });
  const [offset, setOffset] = useState(0);
  const tenants = useQuery({ queryKey: ["platform-tenants"], queryFn: () => api<{ items: TenantRow[] }>("/platform/tenants") });
  const query = { ...period, tenant_id: tenant || undefined, session_id: session || undefined, kind, limit: LIMIT, offset };
  const q = useQuery({ queryKey: ["platform-security-audit", query], queryFn: () => api<SecResponse>("/platform/security-audit", { query }), placeholderData: (p) => p });
  const reset = (fn: () => void) => { fn(); setOffset(0); };
  const picked = q.data?.sessions.find((s) => s.id === session);

  return (
    <SettingsPage title="Security audit" description="Every platform support session and everything attempted in it — the real platform administrator, the business, the reason, the access, the time and the outcome.">
      <p className="mb-3 flex gap-2 rounded-lg bg-muted/60 p-2.5 text-xs text-muted-foreground">
        <Lock className="h-4 w-4 shrink-0 text-primary" />
        {t("Confidential: platform administrators only. Kept apart from the businesses' own audit trails, and append-only — no record can be changed or deleted. Businesses still see their support sessions in Settings → Security and are notified.")}
      </p>
      <div className="mb-4 space-y-2.5">
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          <Select value={tenant} label={t("Business")} onChange={(v) => reset(() => { setTenant(v); setSession(""); })}>
            <option value="">{t("All businesses")}</option>
            {tenants.data?.items.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
          </Select>
          {session && (
            <button className="h-10 rounded-lg border px-3 text-start text-sm" onClick={() => reset(() => setSession(""))}>
              {t("Showing one session")} · <span className="text-primary">{t("Show all")}</span>
            </button>
          )}
        </div>
        {!session && <PeriodFilter value={period} onChange={(v) => reset(() => setPeriod(v))} />}
        <ScrollRow active={kind}>
          {([["all", "All"], ["sessions", "Sessions"], ["changes", "Changes"], ["refused", "Refused"]] as const).map(([k, label]) => (
            <Chip key={k} active={kind === k} onClick={() => reset(() => setKind(k))}>{t(label)}</Chip>
          ))}
        </ScrollRow>
      </div>

      {q.isLoading ? <Loading /> : (
        <div className="space-y-4">
          <Card>
            <p className="mb-1 font-medium">{t("Support sessions")}</p>
            {q.data?.sessions.length === 0 && <p className="py-2 text-sm text-muted-foreground">{t("No support sessions for these filters.")}</p>}
            {(picked ? [picked] : q.data?.sessions ?? []).map((s) => (
              <button key={s.id} onClick={() => reset(() => setSession(s.id === session ? "" : s.id))}
                className={cn("flex w-full items-start gap-3 py-2.5 text-start text-sm", s.id === session && "rounded-lg bg-primary/5 px-2")}>
                <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
                <span className="min-w-0 flex-1">
                  <span className="flex flex-wrap items-center gap-1.5 font-medium">
                    {s.business}
                    <Pill tone={s.scope === "full" ? "warning" : "info"}>{t(s.scope === "full" ? "Full access" : "View only")}</Pill>
                    <Pill tone={STATUS[s.status] ?? "neutral"}>{t(s.status[0].toUpperCase() + s.status.slice(1))}</Pill>
                  </span>
                  <span className="block truncate text-xs text-muted-foreground">{s.platform_user} ({s.platform_email}) · {s.reason}</span>
                  <span className="num block text-xs text-muted-foreground">
                    {dateTime(s.started_at ?? s.requested_at)}{s.ended_at && ` → ${dateTime(s.ended_at)}`} · {s.minutes} min · {count(s.changes)} {t("changes")} · {count(s.refused)} {t("refused")} · {count(s.reads)} {t("pages read")}{s.end_note && ` · ${s.end_note}`}
                  </span>
                </span>
              </button>
            ))}
          </Card>
          <Card>
            <p className="mb-1 font-medium">{t("Events")}</p>
            {q.data?.items.length === 0 && <p className="py-2 text-sm text-muted-foreground">{t("No events for these filters.")}</p>}
            {q.data?.items.map((e) => {
              const [label, tone] = EVENT[e.event] ?? [e.event, "neutral" as Tone];
              return (
                <div key={e.id} className="flex items-start gap-3 py-2.5 text-sm">
                  <Pill tone={tone} className="mt-0.5 shrink-0">{t(label)}</Pill>
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium">{e.tenant_name}{e.scope && <span className="text-muted-foreground"> · {t(e.scope === "full" ? "Full access" : "View only")}</span>}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {e.platform_user}{e.platform_email && ` (${e.platform_email})`}
                      {e.method && <> · <span className="font-mono">{e.method} {e.path.replace(/^\/api/, "")}</span> → {e.status}</>}
                      {e.ip && ` · ${e.ip}`}
                      {typeof e.detail.authentication === "string" && ` · ${e.detail.authentication}`}
                      {typeof e.detail.by === "string" && ` · ${t("by")} ${e.detail.by}`}
                    </p>
                  </div>
                  <span className="num shrink-0 text-xs text-muted-foreground">{dateTime(e.at)}</span>
                </div>
              );
            })}
          </Card>
          {q.data && q.data.total > LIMIT && <Pager total={q.data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
        </div>
      )}
    </SettingsPage>
  );
}
