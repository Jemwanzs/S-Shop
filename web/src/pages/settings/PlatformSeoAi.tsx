/** Platform → AI content (roadmap 159): the platform owner's switch, limits and costs for AI-assisted article drafts. Totals per
 * business only: titles, topics and text stay with each business. */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { count } from "@/lib/format";
import { t } from "@/lib/i18n";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Field, ToggleRow } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { StatCard as Stat } from "@/components/Stat";
import { Card, SettingsPage } from "./shared";

interface Rules { enabled: boolean; monthly_quota: number; monthly_budget_usd: string | number }
interface TenantRow { id: string; name: string; website: string | null; ai_enabled: boolean | null; monthly_quota: number | null; automation: boolean; drafts: number; failed: number; cost_usd: string; tokens: number; articles_published: number }
interface Data { settings: Rules; key_configured: boolean; model: string; spent_usd: string; drafts: number; failed: number; tenants: TenantRow[] }

export function PlatformSeoAi() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["platform-seo-ai"], queryFn: () => api<Data>("/platform/seo-ai") });
  const [d, setD] = useState<Rules | null>(null);
  useEffect(() => { if (q.data) setD(q.data.settings); }, [q.data]);
  const save = useMutation({
    mutationFn: (b: Rules) => api("/platform/seo-ai", { method: "PUT", body: { ...b, monthly_quota: Number(b.monthly_quota), monthly_budget_usd: Number(b.monthly_budget_usd) } }),
    onSuccess: () => { toast.success("Saved"); qc.invalidateQueries({ queryKey: ["platform-seo-ai"] }); },
    onError: (e) => toast.error(e),
  });
  const limit = useMutation({
    mutationFn: (b: { id: string; ai_enabled: boolean | null; monthly_quota: number | null }) => api(`/platform/tenants/${b.id}/seo-ai`, { method: "PUT", body: { ai_enabled: b.ai_enabled, monthly_quota: b.monthly_quota } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["platform-seo-ai"] }),
    onError: (e) => toast.error(e),
  });
  if (q.isLoading || !q.data || !d) return <Loading />;
  const data = q.data;
  return (
    <SettingsPage title="AI content" description="AI-assisted article drafts for businesses' websites. Off until you switch it on, and each business must also opt in."
      dirty={JSON.stringify(d) !== JSON.stringify(data.settings)} saving={save.isPending} onSave={() => save.mutate(d)} onReset={() => setD(data.settings)}>
      {!data.key_configured && <p className="rounded-lg bg-warning/10 p-3 text-sm text-warning">{t("The service key is not set: add ANTHROPIC_API_KEY on Railway. Drafts cannot start until then.")}</p>}
      <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
        <Stat label="Drafts this month" value={count(data.drafts)} />
        <Stat label="Failed or declined" value={count(data.failed)} />
        <Stat label="Estimated cost" value={`$${Number(data.spent_usd).toFixed(2)}`} />
      </div>
      <Card title="Rules">
        <ToggleRow label="Allow AI-assisted drafts" hint={`${t("Model")}: ${data.model}`} checked={d.enabled} onChange={(v) => setD({ ...d, enabled: v })} />
        <Field group label="Drafts per business per month" hint="Unless a business below has its own limit"><Input inputMode="numeric" className="num w-28" value={d.monthly_quota} onChange={(e) => setD({ ...d, monthly_quota: Number(e.target.value.replace(/\D/g, "")) })} /></Field>
        <Field group label="Monthly budget, all businesses (US$)" hint="New drafts pause when this estimate is reached"><Input inputMode="decimal" className="num w-28" value={d.monthly_budget_usd} onChange={(e) => setD({ ...d, monthly_budget_usd: e.target.value.replace(/[^\d.]/g, "") })} /></Field>
      </Card>
      <Card title="Businesses">
        {data.tenants.length === 0 ? <p className="py-2 text-sm text-muted-foreground">{t("No business has a website yet.")}</p> : (
          <ul className="divide-y">
            {data.tenants.map((b) => (
              <li key={b.id} className="space-y-1.5 py-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium">{b.name}</span>
                  {b.automation && <Pill tone="info">{t("Automated drafts on")}</Pill>}
                  {b.ai_enabled === false && <Pill tone="warning">{t("Switched off")}</Pill>}
                </div>
                <p className="num text-xs text-muted-foreground">{count(b.drafts)} {t("drafts")} · {count(b.failed)} {t("failed")} · ${Number(b.cost_usd).toFixed(2)} · {count(b.tokens)} {t("tokens")} · {count(b.articles_published)} {t("articles published")}</p>
                <div className="flex flex-wrap items-center gap-2">
                  <Input aria-label={t("Own monthly limit")} inputMode="numeric" className="num h-9 w-24" placeholder={String(d.monthly_quota)} defaultValue={b.monthly_quota ?? ""}
                    onBlur={(e) => { const v = e.target.value.trim(); const q2 = v === "" ? null : Number(v); if (q2 !== b.monthly_quota) limit.mutate({ id: b.id, ai_enabled: b.ai_enabled, monthly_quota: q2 }); }} />
                  <Button size="sm" variant="outline" onClick={() => limit.mutate({ id: b.id, ai_enabled: b.ai_enabled === false ? null : false, monthly_quota: b.monthly_quota })}>{t(b.ai_enabled === false ? "Allow" : "Switch off")}</Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </SettingsPage>
  );
}
