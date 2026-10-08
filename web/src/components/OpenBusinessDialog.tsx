/** Opening another business from the platform (roadmap 71) — never a silent switch. Two separate routes:
 * A. the tenant's administrator signs in with their own PIN (typed by them; S'Shop never sees or keeps it), or
 * B. platform support access: your PIN again, a reason, a scope and a time limit, under the business's own policy. */
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { KeyRound, LifeBuoy, ShieldCheck } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import type { Profile } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { PasswordInput } from "@/components/PasswordInput";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";

export interface OpenTarget { id: string; name: string; tenant?: string | null; admin_email?: string | null; platform_owned?: boolean }

const DURATIONS: [number, string][] = [[15, "15 min"], [30, "30 min"], [60, "1 hour"], [120, "2 hours"], [240, "4 hours"], [480, "8 hours"]];

type Entered = { status: "active"; token: string; profile: Profile } | { status: "requested"; id: string };

export function OpenBusinessDialog({ target, onClose }: { target: OpenTarget | null; onClose: () => void }) {
  const { switchBusiness } = useSession();
  const navigate = useNavigate();
  const [route, setRoute] = useState<"admin" | "support">("support");
  const [email, setEmail] = useState("");
  const [adminPin, setAdminPin] = useState("");
  const [reason, setReason] = useState("");
  const [scope, setScope] = useState<"view" | "full">("view");
  const [minutes, setMinutes] = useState(60);
  const [pin, setPin] = useState("");
  useEffect(() => {
    if (!target) return;
    setRoute("support");
    setEmail(target.admin_email ?? "");
    setAdminPin("");
    setReason(target.platform_owned ? "Working in a platform-owned business" : "");
    setScope(target.platform_owned ? "full" : "view");
    setMinutes(target.platform_owned ? 480 : 60);
    setPin("");
  }, [target]);
  if (!target) return null;

  const go = (token: string, profile: Profile, msg: string) => {
    switchBusiness(token, profile);
    toast.success(msg);
    onClose();
    navigate(profile.branches.length > 1 ? "/select-branch" : "/", { replace: true });
  };
  const signInAsAdmin = async () => {
    const r = await api<{ token: string; profile: Profile }>("/auth/login", { body: { email: email.trim(), pin: adminPin }, token: null });
    if (r.profile.tenant.id !== target.id) throw new Error("These credentials belong to another business");
    go(r.token, r.profile, `${t("Signed in to")} ${r.profile.tenant.name}`);
  };
  const support = async () => {
    const r = await api<Entered>(`/platform/tenants/${target.id}/support`, { body: { pin, reason: reason.trim(), scope, minutes } });
    if (r.status === "requested") {
      toast.success("Request sent — the business's administrators approve it first. You will be notified.");
      onClose();
      return;
    }
    go(r.token, r.profile, `${t("Support access to")} ${r.profile.tenant.name}`);
  };

  return (
    <ResponsiveDialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={`${t("Open")} ${target.name}`}
      description={target.tenant && target.tenant !== target.name ? `${t("Tenant")}: ${target.tenant}` : "Choose how to enter this business. Every access is recorded in its audit trail."}
      footer={route === "admin" ? (
        <ActionButton online blockedBy={[!/^\S+@\S+\.\S+$/.test(email.trim()) && "Enter the administrator's email", !adminPin && "The administrator enters their PIN"]}
          onAction={signInAsAdmin}><KeyRound /> {t("Sign in as administrator")}</ActionButton>
      ) : (
        <ActionButton online blockedBy={[reason.trim().length < 10 && "Give the reason (at least 10 characters)", !pin && "Enter your PIN"]}
          onAction={support}><LifeBuoy /> {t("Start support access")}</ActionButton>
      )}
    >
      <div className="space-y-3">
        {!target.platform_owned && (
          <div className="grid grid-cols-2 gap-2">
            {([["support", LifeBuoy, "Platform support access"], ["admin", KeyRound, "Tenant administrator sign-in"]] as const).map(([k, Icon, label]) => (
              <button key={k} type="button" onClick={() => setRoute(k)}
                className={cn("flex items-center gap-2 rounded-xl border p-2.5 text-start text-sm", route === k ? "border-primary bg-primary/5 font-medium" : "bg-card")}>
                <Icon className="h-4 w-4 shrink-0 text-primary" /> {t(label)}
              </button>
            ))}
          </div>
        )}
        {route === "admin" ? (
          <>
            <p className="text-xs text-muted-foreground">{t("The administrator types their own PIN. S'Shop never sees, stores or shows it. You leave your platform session and continue as them.")}</p>
            <Field label="Administrator email"><Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="off" /></Field>
            <Field label="Administrator PIN"><PasswordInput value={adminPin} onChange={(e) => setAdminPin(e.target.value)} autoComplete="off" /></Field>
          </>
        ) : (
          <>
            <p className="flex gap-2 rounded-lg bg-muted/60 p-2.5 text-xs text-muted-foreground">
              <ShieldCheck className="h-4 w-4 shrink-0 text-primary" />
              {t("Time-limited, audited and visible to the business, which can end it at any time. Businesses that require approval are asked first.")}
            </p>
            <Field label="Reason"><Textarea value={reason} onChange={(e) => setReason(e.target.value)} maxLength={500} rows={2} placeholder={t("e.g. The owner asked us to check a stock count")} /></Field>
            <div className="grid grid-cols-2 gap-2">
              {([["view", "View only"], ["full", "Full access"]] as const).map(([k, label]) => (
                <button key={k} type="button" onClick={() => setScope(k)} className={cn("h-9 rounded-lg border text-sm", scope === k ? "border-primary bg-primary/5 font-medium" : "bg-card")}>{t(label)}</button>
              ))}
            </div>
            <div className="flex flex-wrap gap-1.5">
              {DURATIONS.map(([m, label]) => (
                <button key={m} type="button" onClick={() => setMinutes(m)} className={cn("h-8 rounded-full border px-3 text-xs", minutes === m ? "border-primary bg-primary text-primary-foreground" : "bg-card")}>{t(label)}</button>
              ))}
            </div>
            <Field label="Your PIN" hint="Confirms it is you"><PasswordInput value={pin} onChange={(e) => setPin(e.target.value)} autoComplete="current-password" /></Field>
          </>
        )}
      </div>
    </ResponsiveDialog>
  );
}
