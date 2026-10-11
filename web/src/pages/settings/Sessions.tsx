/** Settings → Security → Session management (roadmap 87): automatic logout after inactivity, within S'Shop's limits,
 * with stricter timeouts for sensitive roles. The server enforces it; the effective policy is shown. */
import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { useUnsaved } from "@/lib/unsaved";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import type { Role } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/Select";
import { ActionButton } from "@/components/ActionButton";
import { Field } from "@/components/Form";
import { Loading } from "@/components/Page";
import { Card, Fact } from "./shared";

interface Policy { idle_minutes: number; warning_seconds: number; role_minutes: { role_id: string; minutes: number }[] }
interface Data {
  policy: Policy;
  platform: { default_minutes: number; min_minutes: number; max_minutes: number; max_session_hours: number };
  mine: { idle_seconds: number; warning_seconds: number };
}

const PRESETS = [1, 2, 3, 5, 10, 15, 20, 30, 60];
export const minutesLabel = (m: number) => (m >= 60 && m % 60 === 0 ? `${m / 60} ${t(m === 60 ? "hour" : "hours")}` : `${m} ${t(m === 1 ? "minute" : "minutes")}`);

export function TenantSessionCard() {
  const qc = useQueryClient();
  const { profile } = useSession();
  const q = useQuery({ queryKey: ["session-policy"], queryFn: () => api<Data>("/security/sessions") });
  const roles = useQuery({ queryKey: ["roles"], queryFn: () => api<Role[]>("/roles") });
  const [d, setD] = useState<Policy | null>(null);
  const [custom, setCustom] = useState(false);
  useEffect(() => {
    if (!q.data) return;
    setD(q.data.policy);
    setCustom(!PRESETS.includes(q.data.policy.idle_minutes));
  }, [q.data]);
  const dirty = !!d && !!q.data && JSON.stringify(d) !== JSON.stringify(q.data.policy);
  useUnsaved("session-policy", dirty);
  if (!q.data || !d) return <Card title="Session management"><Loading /></Card>;
  const { platform, mine } = q.data;
  const admin = profile?.permissions.includes("*") && !profile?.acting && !profile?.quick;
  const inRange = (m: number) => m >= platform.min_minutes && m <= platform.max_minutes;
  const presets = PRESETS.filter(inRange);
  const roleName = (id: string) => roles.data?.find((r) => r.id === id)?.name ?? t("Role");
  const unused = roles.data?.filter((r) => r.is_active && !d.role_minutes.some((x) => x.role_id === r.id)) ?? [];
  const problem = !inRange(d.idle_minutes) ? `${t("Choose between")} ${platform.min_minutes} ${t("and")} ${platform.max_minutes} ${t("minutes")}`
    : d.warning_seconds < 10 || d.warning_seconds > 120 ? t("The warning shows 10–120 seconds before logout")
    : d.role_minutes.some((r) => r.minutes < platform.min_minutes || r.minutes > d.idle_minutes) ? t("A role's timeout must be shorter than the business's")
    : null;

  return (
    <Card title="Session management">
      <div className="space-y-3 py-2 text-sm">
        <p className="text-xs text-muted-foreground">{t("People are logged out automatically after a period without activity (taps, clicks, typing, scrolling). Background updates never keep a session open.")}</p>
        <Field group label="Log out after inactivity" hint={`${t("S'Shop allows")} ${minutesLabel(platform.min_minutes)} – ${minutesLabel(platform.max_minutes)}`}>
          <div className="flex flex-wrap gap-1.5">
            {presets.map((m) => (
              <button key={m} type="button" disabled={!admin} onClick={() => { setCustom(false); setD({ ...d, idle_minutes: m }); }}
                className={cn("h-8 rounded-lg border px-3 text-xs", !custom && d.idle_minutes === m ? "border-primary bg-primary/5 font-medium" : "bg-card", !admin && "opacity-60")}>{minutesLabel(m)}</button>
            ))}
            <button type="button" disabled={!admin} onClick={() => setCustom(true)}
              className={cn("h-8 rounded-lg border px-3 text-xs", custom ? "border-primary bg-primary/5 font-medium" : "bg-card", !admin && "opacity-60")}>{t("Custom")}</button>
          </div>
          {custom && (
            <div className="mt-2 flex items-center gap-2">
              <Input type="number" className="w-24" min={platform.min_minutes} max={platform.max_minutes} disabled={!admin} value={d.idle_minutes}
                onChange={(e) => setD({ ...d, idle_minutes: Number(e.target.value) || 0 })} />
              <span className="text-xs text-muted-foreground">{t("minutes")}</span>
            </div>
          )}
        </Field>
        <Field label="Warning before logout (seconds)" hint="10–120">
          <Input type="number" className="w-24" min={10} max={120} disabled={!admin} value={d.warning_seconds} onChange={(e) => setD({ ...d, warning_seconds: Number(e.target.value) || 0 })} />
        </Field>
        <Field group label="Stricter timeouts for sensitive roles" optional>
          <div className="space-y-2">
            {d.role_minutes.map((r, i) => (
              <div key={r.role_id} className="flex items-center gap-2 rounded-lg bg-muted/50 p-2">
                <span className="min-w-0 flex-1 truncate font-medium">{roleName(r.role_id)}</span>
                <Input type="number" className="w-20" min={platform.min_minutes} max={d.idle_minutes} disabled={!admin} value={r.minutes} aria-label={t("Minutes")}
                  onChange={(e) => setD({ ...d, role_minutes: d.role_minutes.map((x, j) => (j === i ? { ...x, minutes: Number(e.target.value) || 0 } : x)) })} />
                <span className="text-xs text-muted-foreground">{t("min")}</span>
                {admin && <Button variant="ghost" size="icon" aria-label={t("Remove")} onClick={() => setD({ ...d, role_minutes: d.role_minutes.filter((_, j) => j !== i) })}><Trash2 /></Button>}
              </div>
            ))}
            {admin && unused.length > 0 && (
              <Select value="" placeholder={t("Add a role…")} onChange={(v) => v && setD({ ...d, role_minutes: [...d.role_minutes, { role_id: v, minutes: Math.max(platform.min_minutes, Math.min(d.idle_minutes, 2)) }] })}>
                {unused.map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
              </Select>
            )}
            {!d.role_minutes.length && !admin && <p className="text-xs text-muted-foreground">{t("Every role uses the business's timeout.")}</p>}
          </div>
        </Field>
        <div className="grid grid-cols-2 gap-3 rounded-xl bg-muted/50 p-3 sm:grid-cols-3">
          <Fact label={t("Business timeout")}>{minutesLabel(q.data.policy.idle_minutes)}</Fact>
          <Fact label={t("Your timeout")}>{minutesLabel(Math.round(mine.idle_seconds / 60))} · {mine.warning_seconds}s {t("warning")}</Fact>
          <Fact label={t("Longest session")}>{platform.max_session_hours} {t("hours")}</Fact>
        </div>
        {problem && <p className="text-xs text-destructive">{problem}</p>}
        {admin ? (
          <div className="flex justify-end">
            <ActionButton online blockedBy={[!dirty && "Nothing to save", problem]} onAction={async () => {
              await api("/security/sessions", { method: "PUT", body: d });
              toast.success("Session rules saved");
              qc.invalidateQueries({ queryKey: ["session-policy"] });
              qc.invalidateQueries({ queryKey: ["me"] });
            }}>{t("Save")}</ActionButton>
          </div>
        ) : <p className="text-xs text-muted-foreground">{t("Only this business's administrators, fully signed in, change this.")}</p>}
      </div>
    </Card>
  );
}
