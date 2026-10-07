import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRightLeft, Building2, ChevronRight, FlaskConical, Loader2, RotateCcw } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { ago, count } from "@/lib/format";
import { t } from "@/lib/i18n";
import type { Profile } from "@/lib/types";
import { STATUS_LABEL, STATUS_TONE, type BillingSummary } from "@/lib/billing";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Pill } from "@/components/Badges";
import { ConfirmDialog } from "@/components/Form";
import { Card, SettingsPage } from "./shared";

export interface TenantRow {
  id: string;
  name: string;
  slug: string;
  is_demo: boolean;
  status: "active" | "deactivated";
  ownership: "customer" | "platform";
  billing_suspended: boolean;
  status_reason: string;
  status_changed_at: string | null;
  activated_at: string | null;
  created_at: string;
  phone: string;
  email: string;
  address: string;
  users: number;
  branches: number;
  sales: number;
  last_sale_at: string | null;
  last_login_at: string | null;
  admin_name: string | null;
  admin_email: string | null;
  admin_phone: string | null;
  billing: BillingSummary;
}

interface DemoState {
  status: { running: boolean; step: string; finished_at: string | null; error: string | null; report: { sales: number; products: number; customers: number; photos: number; photos_note: string; notes: string[] } | null };
  tenant_id: string | null;
  pexels: boolean;
}

/** Platform admins: every business on this installation, opening one, and the Pablo Niche demo business. */
export function BusinessesSettings() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const { profile, switchBusiness } = useSession();
  const tenants = useQuery({ queryKey: ["platform-tenants"], queryFn: () => api<{ items: TenantRow[]; home_tenant_id: string }>("/platform/tenants") });
  const demo = useQuery({
    queryKey: ["platform-demo"],
    queryFn: () => api<DemoState>("/platform/demo"),
    refetchInterval: (q) => (q.state.data?.status.running ? 2000 : false),
  });
  const running = !!demo.data?.status.running;
  useEffect(() => {
    if (!running) qc.invalidateQueries({ queryKey: ["platform-tenants"] });
  }, [running, qc]);

  const open = useMutation({
    mutationFn: (id: string) => api<{ token: string; profile: Profile }>(`/platform/tenants/${id}/open`, { method: "POST" }),
    onSuccess: (r) => {
      switchBusiness(r.token, r.profile);
      toast.success(`${t("Now in")} ${r.profile.tenant.name}`);
      navigate(r.profile.branches.length > 1 ? "/select-branch" : "/", { replace: true });
    },
    onError: (e) => toast.error(e),
  });
  const build = useMutation({
    mutationFn: (reset: boolean) => api("/platform/demo", { body: { reset } }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["platform-demo"] }),
    onError: (e) => toast.error(e),
  });

  const st = demo.data?.status;
  const [confirmReset, setConfirmReset] = useState(false);
  return (
    <SettingsPage title="Businesses" description="Every business on this S'Shop installation with its status and billing. Opening one gives you full access inside it as platform owner — it is recorded in that business's audit trail.">
      <Card>
        {tenants.data?.items.map((b) => {
          const current = b.id === profile?.tenant.id;
          return (
            <div key={b.id} className="flex items-center gap-3 py-3">
              <span className="rounded-lg bg-primary/10 p-2 text-primary"><Building2 className="h-4 w-4" /></span>
              <Link to={`/settings/businesses/${b.id}`} className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5 font-medium">
                  <span className="truncate">{b.name}</span>
                  {b.is_demo && <Pill tone="info">{t("Demo")}</Pill>}
                  {b.id === tenants.data.home_tenant_id && <Pill>{t("Yours")}</Pill>}
                  {b.status !== "active" && <Pill tone="danger">{t("Deactivated")}</Pill>}
                  {!b.is_demo && b.billing.status !== "not_set" && <Pill tone={STATUS_TONE[b.billing.status]}>{t(STATUS_LABEL[b.billing.status])}</Pill>}
                </div>
                <div className="num truncate text-xs text-muted-foreground">
                  {b.admin_name ? `${b.admin_name} · ` : ""}{count(b.branches)} {t("branches")} · {count(b.users)} {t("users")} · {count(b.sales)} {t("sales")}{b.last_sale_at && ` · ${t("last sale")} ${ago(b.last_sale_at)}`}
                </div>
              </Link>
              {current ? (
                <Pill tone="success">{t("Open now")}</Pill>
              ) : (
                <ActionButton size="sm" variant="outline" online busy={open.isPending && open.variables === b.id} disabled={open.isPending}
                  onAction={() => open.mutateAsync(b.id)}><ArrowRightLeft /> <span className="hidden sm:inline">{t("Open business")}</span></ActionButton>
              )}
              <Link to={`/settings/businesses/${b.id}`} aria-label={t("Details")} className="text-muted-foreground"><ChevronRight className="h-4 w-4 rtl:rotate-180" /></Link>
            </div>
          );
        })}
      </Card>

      <Card title="Pablo Niche demo business">
        <div className="space-y-3 py-2 text-sm">
          <p className="text-muted-foreground">
            {t("A separate, clearly marked demo business with five months of realistic trading — products, stock across branches, sales, customers, loyalty, credit, orders, transfers and expenses — created through the app's own rules. Your real business is never touched.")}
          </p>
          {!demo.data?.pexels && <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t("Product photos need PEXELS_API_KEY on the server (free Pexels API key). Without it the demo is built without photos.")}</p>}
          {running && (
            <p className="flex items-center gap-2 rounded-lg bg-muted p-2.5"><Loader2 className="h-4 w-4 animate-spin" /> {st?.step}</p>
          )}
          {!running && st?.error && <p className="rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">{st.error}</p>}
          {!running && st?.report && (
            <p className="rounded-lg bg-success/10 p-2.5 text-xs">
              {t("Built")}: {count(st.report.products)} {t("products")}, {count(st.report.customers)} {t("customers")}, {count(st.report.sales)} {t("sales")}, {count(st.report.photos)} {t("photos")}. {st.report.photos_note}
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            {demo.data?.tenant_id ? (
              <>
                <ActionButton size="sm" online busy={open.isPending && open.variables === demo.data.tenant_id} disabled={open.isPending}
                  blockedBy={[running && "Building…"]} onAction={() => open.mutateAsync(demo.data!.tenant_id!)}><FlaskConical /> {t("Open demo")}</ActionButton>
                <Button size="sm" variant="outline" disabled={running} onClick={() => setConfirmReset(true)}><RotateCcw /> {t("Reset demo")}</Button>
              </>
            ) : (
              <Button size="sm" disabled={running || build.isPending} onClick={() => build.mutate(false)}>{running ? <Loader2 className="animate-spin" /> : <FlaskConical />} {t("Build demo business")}</Button>
            )}
          </div>
        </div>
      </Card>
      <ConfirmDialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        title="Reset the demo business?"
        description="The demo business and everything in it are deleted and built again from scratch. Real businesses are not affected."
        confirmLabel="Reset demo"
        destructive
        onConfirm={() => { setConfirmReset(false); build.mutate(true); }}
      />
    </SettingsPage>
  );
}
