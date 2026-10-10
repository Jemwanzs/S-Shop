/** Settings → Support access (roadmap 71): the business decides how S'Shop support may enter it, sees every request
 * and session (who, why, scope, how long), approves or declines requests and can end access at any time. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { BellRing, ShieldCheck } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { Loading } from "@/components/Page";
import { Card, SettingsPage } from "./shared";
import { SupportSessions, type SessionRow } from "./Tenants";
import { TenantQuickPinCard } from "./QuickPin";

const POLICIES = [
  ["notify", BellRing, "Notify administrators", "S'Shop support may open the business for the time and scope it states; administrators are notified at once and can end it."],
  ["approval", ShieldCheck, "Ask for approval first", "Every request waits until an administrator approves it here; nothing is opened before that."],
] as const;

export function SupportAccessSettings() {
  const qc = useQueryClient();
  const { profile } = useSession();
  const q = useQuery({ queryKey: ["support-access"], queryFn: () => api<{ policy: string; items: SessionRow[] }>("/support-access") });
  const policy = useMutation({
    mutationFn: (p: string) => api("/support-access/policy", { method: "PUT", body: { policy: p } }),
    onSuccess: () => { toast.success("Support access policy saved"); qc.invalidateQueries({ queryKey: ["support-access"] }); },
    onError: (e) => toast.error(e),
  });
  const decide = async (id: string, d: "approve" | "deny" | "revoke") => {
    await api(`/support-access/${id}/${d}`, { method: "POST" });
    toast.success(d === "approve" ? "Support access approved" : d === "deny" ? "Support access declined" : "Support access ended");
    qc.invalidateQueries({ queryKey: ["support-access"] });
  };
  const admin = profile?.permissions.includes("*") && !profile?.acting && !profile?.quick;
  return (
    <SettingsPage title="Security" description="Sign-in rules for this business and how S'Shop support may enter it. S'Shop never asks for or knows your PINs; every support session is time-limited and recorded in your audit trail.">
      <TenantQuickPinCard />
      {q.isLoading || !q.data ? <Loading /> : (
        <>
          <Card title="Support access policy">
            <div className="grid gap-2 py-2 sm:grid-cols-2">
              {POLICIES.map(([k, Icon, label, hint]) => (
                <button key={k} type="button" disabled={!admin || policy.isPending} onClick={() => q.data.policy !== k && policy.mutate(k)}
                  className={cn("flex gap-2.5 rounded-xl border p-3 text-start disabled:opacity-70", q.data.policy === k ? "border-primary bg-primary/5" : "bg-card")}>
                  <Icon className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
                  <span>
                    <span className="block text-sm font-medium">{t(label)}</span>
                    <span className="block text-xs text-muted-foreground">{t(hint)}</span>
                  </span>
                </button>
              ))}
            </div>
            {!admin && <p className="pb-2 text-xs text-muted-foreground">{t("Only this business's administrators change the policy.")}</p>}
          </Card>
          <SupportSessions sessions={q.data.items} tenantSide onDecide={(id, d) => { void decide(id, d).catch((e) => toast.error(e)); }} />
        </>
      )}
    </SettingsPage>
  );
}
