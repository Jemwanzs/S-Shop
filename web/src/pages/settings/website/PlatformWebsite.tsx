/** Platform owner → business → Website: approve, decline, activate or disable the service, bill it, and connect domains. */
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { FilePlus2, Globe, Pencil } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { date, dateTime, moneyDoc } from "@/lib/format";
import type { BillingPlan, BillingSummary, ModuleDef } from "@/lib/billing";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";
import { Card, Fact } from "../shared";
import { t } from "@/lib/i18n";

export interface PlatformWebsiteInfo {
  status: "none" | "requested" | "declined" | "active" | "disabled";
  status_reason?: string;
  request_message?: string;
  requested_at?: string | null;
  activated_at?: string | null;
  billing_suspended?: boolean;
  version?: number;
  published_at?: string | null;
  domain?: { domain: string; status: string; verified_at: string | null; routing_target: string | null; automatic: boolean } | null;
}

const STATUS: Record<string, string> = { none: "Not requested", requested: "Requested", declined: "Declined", active: "Active", disabled: "Disabled" };

export function PlatformWebsiteCard({ tenantId, info, billing, catalogue, owned, onChanged, PlanDialog, IssueDialog }: {
  tenantId: string;
  info: PlatformWebsiteInfo;
  billing: { plan: (BillingPlan & { notes: string }) | null; summary: BillingSummary } | undefined;
  catalogue: ModuleDef[];
  owned: boolean;
  onChanged: () => void;
  PlanDialog: React.ComponentType<{ tenantId: string; plan: (BillingPlan & { notes: string }) | null; catalogue: ModuleDef[]; onClose: () => void; onSaved: () => void; service?: "platform" | "website" }>;
  IssueDialog: React.ComponentType<{ tenantId: string; plan: BillingPlan | null; onClose: () => void; onSaved: () => void; service?: "platform" | "website" }>;
}) {
  const [action, setAction] = useState<"activate" | "decline" | "disable" | null>(null);
  const [reason, setReason] = useState("");
  const [plan, setPlan] = useState(false);
  const [issue, setIssue] = useState(false);
  const [target, setTarget] = useState("");
  const decide = useMutation({
    mutationFn: () => api(`/platform/tenants/${tenantId}/website`, { body: { action, reason } }),
    onSuccess: () => {
      toast.success(action === "activate" ? "Website service activated" : action === "decline" ? "Request declined" : "Website service disabled");
      setAction(null);
      setReason("");
      onChanged();
    },
    onError: (e) => toast.error(e),
  });
  const routing = useMutation({
    mutationFn: () => api(`/platform/tenants/${tenantId}/website/domain`, { method: "PUT", body: { routing_target: target } }),
    onSuccess: () => { toast.success("Routing target saved — the business has been told"); setTarget(""); onChanged(); },
    onError: (e) => toast.error(e),
  });
  const s = billing?.summary;
  const d = info.domain;
  return (
    <Card title="Website Add-On" action={
      <div className="flex flex-wrap gap-1.5">
        {info.status !== "active" && <Button size="sm" onClick={() => setAction("activate")}>{t("Activate")}</Button>}
        {info.status === "requested" && <Button size="sm" variant="outline" onClick={() => setAction("decline")}>{t("Decline")}</Button>}
        {info.status === "active" && <Button size="sm" variant="outline" onClick={() => setAction("disable")}>{t("Disable")}</Button>}
      </div>
    }>
      <div className="grid grid-cols-2 gap-x-4 gap-y-3 py-2 sm:grid-cols-3">
        <Fact label={t("Service")}>{t(STATUS[info.status] ?? info.status)}{info.billing_suspended && ` · ${t("billing suspended")}`}</Fact>
        {info.requested_at && <Fact label={t("Requested")}>{dateTime(info.requested_at)}</Fact>}
        {info.activated_at && <Fact label={t("Activated")}>{date(info.activated_at)}</Fact>}
        {info.status === "active" && <Fact label={t("Published")}>{info.published_at ? `v${info.version} · ${date(info.published_at)}` : t("Not yet")}</Fact>}
        {s && <Fact label={t("Website billing")}>{t(String((s as { status_label?: string }).status_label ?? s.status))}</Fact>}
        {s && <Fact label={t("Outstanding")}>{moneyDoc(s.outstanding, s.currency)}</Fact>}
      </div>
      {info.request_message && <p className="py-2 text-sm"><span className="text-muted-foreground">{t("Request:")}</span> {info.request_message}</p>}
      {info.status_reason && <p className="py-2 text-sm text-muted-foreground">{info.status_reason}</p>}
      {(info.status === "active" || info.status === "disabled") && !owned && (
        <div className="flex flex-wrap gap-1.5 py-2">
          <Button size="sm" variant="outline" onClick={() => setPlan(true)}><Pencil /> {t("Website plan")}</Button>
          <Button size="sm" variant="outline" onClick={() => setIssue(true)}><FilePlus2 /> {t("Issue website invoice")}</Button>
        </div>
      )}
      {d && (
        <div className="space-y-2 py-2">
          <p className="flex items-center gap-2 text-sm"><Globe className="h-4 w-4 text-muted-foreground" /> <span className="font-medium">{d.domain}</span> · {t(d.status.replace(/_/g, " "))}</p>
          {!d.automatic && d.verified_at && d.status !== "active" && (
            <div className="space-y-1.5 rounded-lg border p-2.5">
              <p className="text-xs text-muted-foreground">{t("Ownership verified. Add the domain to the S'Shop service in Railway (Settings → Networking → Custom Domain), then paste the CNAME value Railway shows:")}</p>
              {d.routing_target && <p className="text-xs">{t("Current:")} <code>{d.routing_target}</code></p>}
              <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); routing.mutate(); }}>
                <Input value={target} onChange={(e) => setTarget(e.target.value)} placeholder="abc123.up.railway.app" className="h-8" />
                <ActionButton size="sm" type="submit" online busy={routing.isPending} blockedBy={[!target.trim() && "Enter the target"]}>Save</ActionButton>
              </form>
            </div>
          )}
        </div>
      )}
      <ResponsiveDialog open={!!action} onOpenChange={(o) => !o && setAction(null)}
        title={action === "activate" ? "Activate the website service" : action === "decline" ? "Decline the request" : "Disable the website"}
        description={action === "activate" ? "The business gets a ready-made draft website to edit and publish. Set up website billing separately." : action === "disable" ? "The public website shows “temporarily unavailable”. All content, media, products, domain and analytics are kept; POS is not affected." : undefined}
        footer={<ActionButton online busy={decide.isPending} blockedBy={[action !== "activate" && reason.trim().length < 3 && "Give the reason"]} onAction={() => decide.mutateAsync()}>{action === "activate" ? "Activate" : action === "decline" ? "Decline" : "Disable"}</ActionButton>}>
        <Field label="Reason" optional={action === "activate"}><Textarea value={reason} onChange={(e) => setReason(e.target.value)} maxLength={500} /></Field>
      </ResponsiveDialog>
      {plan && <PlanDialog service="website" tenantId={tenantId} plan={billing?.plan ?? null} catalogue={catalogue} onClose={() => setPlan(false)} onSaved={onChanged} />}
      {issue && <IssueDialog service="website" tenantId={tenantId} plan={billing?.plan ?? null} onClose={() => setIssue(false)} onSaved={onChanged} />}
    </Card>
  );
}
