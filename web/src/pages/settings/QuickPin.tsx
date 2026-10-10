/** Quick Login PIN screens (roadmap 83): the person's own PIN and trusted devices (User preferences → Security), the
 * business's rule (Settings → Security) and the platform's rules (Platform → Security). Nobody can see a Quick PIN. */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, LogOut, MonitorSmartphone, ShieldCheck, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { ago, date } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { forgetQuickDevice, quickDevice, rememberQuickDevice } from "@/lib/quick";
import type { Role } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { PasswordInput } from "@/components/PasswordInput";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ConfirmDialog, Field, ToggleRow } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { Card, Fact, SettingsPage } from "./shared";

interface Status {
  available: boolean;
  reason: string | null;
  enabled: boolean;
  set_at: string | null;
  min_length: number;
  devices: { id: string; name: string; created_at: string; last_used_at: string | null; expires_at: string }[];
  quick_session: boolean;
}

const FULL_HINT = "Sign in with your email and full PIN to change Quick PIN settings.";

/** User preferences → Security. */
export function QuickPinCard() {
  const qc = useQueryClient();
  const { profile, signOut } = useSession();
  const q = useQuery({ queryKey: ["quick-pin"], queryFn: () => api<Status>("/auth/quick-pin") });
  const [mode, setMode] = useState<"set" | "trust" | null>(null);
  const [confirmOff, setConfirmOff] = useState(false);
  const [confirmAll, setConfirmAll] = useState(false);
  const here = quickDevice();
  const refresh = () => qc.invalidateQueries({ queryKey: ["quick-pin"] });
  const off = useMutation({
    mutationFn: () => api("/auth/quick-pin", { method: "DELETE" }),
    onSuccess: () => { forgetQuickDevice(); setConfirmOff(false); toast.success("Quick PIN switched off on every device"); refresh(); },
    onError: (e) => toast.error(e),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => api(`/auth/quick-pin/devices/${id}`, { method: "DELETE" }),
    onSuccess: (_r, id) => { if (here?.device_id === id) forgetQuickDevice(); toast.success("Device removed"); refresh(); },
    onError: (e) => toast.error(e),
  });
  const all = useMutation({
    mutationFn: () => api("/auth/quick-pin/devices/revoke-all", { method: "POST" }),
    onSuccess: () => { forgetQuickDevice(); toast.success("Signed out everywhere"); signOut(); },
    onError: (e) => toast.error(e),
  });
  if (q.isLoading || !q.data) return <Card title="Quick Login PIN"><Loading /></Card>;
  const s = q.data;
  const thisTrusted = !!here && s.devices.some((d) => d.id === here.device_id);
  return (
    <Card title="Quick Login PIN" action={s.enabled && <Pill tone="success">{t("On")}</Pill>}>
      <div className="space-y-3 py-2 text-sm">
        <p className="text-muted-foreground">
          {t("A 4–6 digit PIN for fast sign-in on your own trusted devices. New or shared devices always need your email and full PIN first.")}
        </p>
        {!s.available ? (
          <p className="rounded-lg bg-muted/60 p-2.5 text-xs text-muted-foreground">{t(s.reason ?? "Quick PIN sign-in is not available")}</p>
        ) : s.quick_session ? (
          <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t(FULL_HINT)}</p>
        ) : (
          <div className="flex flex-wrap gap-2">
            <Button size="sm" onClick={() => setMode("set")}><KeyRound /> {t(s.enabled ? "Change Quick PIN" : "Create Quick PIN")}</Button>
            {s.enabled && !thisTrusted && <Button size="sm" variant="outline" onClick={() => setMode("trust")}><MonitorSmartphone /> {t("Trust this device")}</Button>}
            {s.enabled && <Button size="sm" variant="ghost" onClick={() => setConfirmOff(true)}>{t("Switch off")}</Button>}
          </div>
        )}
        {s.enabled && s.set_at && <p className="text-xs text-muted-foreground">{t("Set")} {date(s.set_at)}</p>}
      </div>
      {s.devices.length > 0 && (
        <div className="border-t py-2">
          <p className="label-caps py-1">{t("Trusted devices")}</p>
          {s.devices.map((d) => (
            <div key={d.id} className="flex items-center gap-3 py-2 text-sm">
              <MonitorSmartphone className="h-4 w-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">{d.name}{here?.device_id === d.id && <span className="ms-1.5 text-xs font-normal text-primary">({t("this device")})</span>}</p>
                <p className="truncate text-xs text-muted-foreground">{d.last_used_at ? `${t("used")} ${ago(d.last_used_at)}` : t("not used yet")} · {t("until")} {date(d.expires_at)}</p>
              </div>
              <Button variant="ghost" size="icon-sm" aria-label={t("Remove device")} disabled={revoke.isPending} onClick={() => revoke.mutate(d.id)}><Trash2 /></Button>
            </div>
          ))}
        </div>
      )}
      <div className="border-t py-2">
        <Button size="sm" variant="ghost" className="text-destructive" onClick={() => setConfirmAll(true)}><LogOut /> {t("Sign out everywhere")}</Button>
      </div>
      {mode && profile && (
        <SetupDialog mode={mode} min={s.min_length} onClose={() => setMode(null)} onDone={(device_id, token) => {
          rememberQuickDevice({ token, device_id, email: profile.user.email, name: profile.user.name, business: profile.tenant.name });
          setMode(null);
          refresh();
        }} />
      )}
      <ConfirmDialog open={confirmOff} onOpenChange={setConfirmOff} title="Switch off Quick PIN?" description="Your Quick PIN is deleted and every trusted device needs your full sign-in again."
        confirmLabel="Switch off" destructive busy={off.isPending} onConfirm={() => off.mutate()} />
      <ConfirmDialog open={confirmAll} onOpenChange={setConfirmAll} title="Sign out everywhere?" description="Every session ends, including this one, and every trusted device needs your full sign-in again."
        confirmLabel="Sign out everywhere" destructive busy={all.isPending} onConfirm={() => all.mutate()} />
    </Card>
  );
}

function SetupDialog({ mode, min, onClose, onDone }: { mode: "set" | "trust"; min: number; onClose: () => void; onDone: (device: string, token: string) => void }) {
  const [current, setCurrent] = useState("");
  const [pin, setPin] = useState("");
  const [again, setAgain] = useState("");
  const [name, setName] = useState("");
  const digits = (v: string) => v.replace(/\D/g, "").slice(0, 6);
  const set = mode === "set";
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title={set ? "Quick PIN" : "Trust this device"}
      description={set ? `${min}–6 ${t("digits. It works only on devices you trust; changing it means trusting your other devices again.")}` : "Use your Quick PIN on this device too."}
      footer={<ActionButton online blockedBy={[!current && "Enter your current PIN", set && (pin.length < min ? `${min}–6 digits` : pin !== again && "The Quick PINs do not match")]}
        onAction={async () => {
          const r = set
            ? await api<{ device_id: string; device_token: string }>("/auth/quick-pin", { method: "PUT", body: { current_pin: current, quick_pin: pin, device_name: name } })
            : await api<{ device_id: string; device_token: string }>("/auth/quick-pin/devices", { body: { current_pin: current, device_name: name } });
          toast.success(set ? "Quick PIN saved — this device is trusted" : "This device is trusted");
          onDone(r.device_id, r.device_token);
        }}>{t("Save")}</ActionButton>}>
      <div className="space-y-3">
        <Field label="Your current full PIN" hint="Confirms it is you"><PasswordInput value={current} onChange={(e) => setCurrent(e.target.value)} autoComplete="current-password" /></Field>
        {set && (
          <div className="grid grid-cols-2 gap-3">
            <Field label="New Quick PIN"><Input type="password" inputMode="numeric" autoComplete="new-password" value={pin} onChange={(e) => setPin(digits(e.target.value))} /></Field>
            <Field label="Repeat"><Input type="password" inputMode="numeric" autoComplete="new-password" value={again} onChange={(e) => setAgain(digits(e.target.value))} /></Field>
          </div>
        )}
        <Field label="Device name" optional hint="e.g. My phone"><Input value={name} maxLength={60} onChange={(e) => setName(e.target.value)} /></Field>
        <p className="flex gap-2 text-xs text-muted-foreground"><ShieldCheck className="h-4 w-4 shrink-0 text-primary" />{t("Payments, roles, access and security settings still ask for your full sign-in.")}</p>
      </div>
    </ResponsiveDialog>
  );
}

/** Settings → Security: the business's Quick PIN rule (within the platform's). */
export function TenantQuickPinCard() {
  const qc = useQueryClient();
  const { profile } = useSession();
  const q = useQuery({ queryKey: ["quick-pin-policy"], queryFn: () => api<{ policy: { enabled: boolean; role_ids: string[] }; platform: { enabled: boolean; min_length: number; session_hours: number }; users_with_quick_pin: number }>("/security/quick-pin") });
  const roles = useQuery({ queryKey: ["roles"], queryFn: () => api<Role[]>("/roles") });
  const [draft, setDraft] = useState<{ enabled: boolean; role_ids: string[] } | null>(null);
  useEffect(() => { if (q.data) setDraft(q.data.policy); }, [q.data]);
  if (!q.data || !draft) return <Card title="Quick Login PIN"><Loading /></Card>;
  const admin = profile?.permissions.includes("*") && !profile?.acting && !profile?.quick;
  const dirty = JSON.stringify(draft) !== JSON.stringify(q.data.policy);
  return (
    <Card title="Quick Login PIN">
      <div className="space-y-2 py-2 text-sm">
        {!q.data.platform.enabled && <p className="rounded-lg bg-muted/60 p-2.5 text-xs text-muted-foreground">{t("Quick PIN sign-in is switched off on S'Shop")}</p>}
        <ToggleRow label="Allow Quick PIN sign-in" hint={`${q.data.platform.min_length}–6 ${t("digits, on devices people trust after a full sign-in")} · ${q.data.users_with_quick_pin} ${t("people use it")}`}
          checked={draft.enabled} onChange={(v) => setDraft({ ...draft, enabled: v })} disabled={!admin || !q.data.platform.enabled} />
        {draft.enabled && (
          <Field label="Roles" hint="None selected = every role">
            <div className="grid gap-2 rounded-xl bg-muted/50 p-3 sm:grid-cols-2">
              {roles.data?.filter((r) => r.is_active).map((r) => (
                <label key={r.id} className={cn("flex items-center gap-2", !admin && "opacity-60")}>
                  <Checkbox disabled={!admin} checked={draft.role_ids.includes(r.id)} onCheckedChange={(c) => setDraft({ ...draft, role_ids: c ? [...draft.role_ids, r.id] : draft.role_ids.filter((x) => x !== r.id) })} />
                  {r.name}
                </label>
              ))}
            </div>
          </Field>
        )}
        <p className="text-xs text-muted-foreground">{t("Payments, roles, access and security settings always ask for the full sign-in. Switching off revokes every trusted device at once.")}</p>
        {admin ? (
          <div className="flex justify-end">
            <ActionButton online blockedBy={[!dirty && "Nothing to save"]} onAction={async () => {
              await api("/security/quick-pin", { method: "PUT", body: draft });
              toast.success("Quick PIN rule saved");
              qc.invalidateQueries({ queryKey: ["quick-pin-policy"] });
            }}>{t("Save")}</ActionButton>
          </div>
        ) : <p className="text-xs text-muted-foreground">{t("Only this business's administrators, fully signed in, change this.")}</p>}
      </div>
    </Card>
  );
}

interface PlatformSecurity { quick_pin_enabled: boolean; quick_pin_min_length: number; quick_session_hours: number; max_attempts: number; device_days: number }

/** Platform → Security. */
export function PlatformSecuritySettings() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["platform-security"], queryFn: () => api<PlatformSecurity>("/platform/security") });
  const [d, setD] = useState<PlatformSecurity | null>(null);
  useEffect(() => { if (q.data) setD(q.data); }, [q.data]);
  const save = useMutation({
    mutationFn: (b: PlatformSecurity) => api<{ devices_revoked: number }>("/platform/security", { method: "PUT", body: b }),
    onSuccess: (r) => { toast.success(r.devices_revoked ? `Saved — ${r.devices_revoked} trusted devices revoked` : "Security settings saved"); qc.invalidateQueries({ queryKey: ["platform-security"] }); },
    onError: (e) => toast.error(e),
  });
  return (
    <SettingsPage title="Security" description="Platform-wide sign-in rules. Businesses can only narrow them." loading={q.isLoading}
      dirty={!!d && JSON.stringify(d) !== JSON.stringify(q.data)} saving={save.isPending} onSave={() => d && save.mutate(d)} onReset={() => q.data && setD(q.data)}>
      {d && (
        <Card title="Quick Login PIN">
          <ToggleRow label="Allow Quick PIN sign-in" hint="Off revokes every trusted device on S'Shop." checked={d.quick_pin_enabled} onChange={(v) => setD({ ...d, quick_pin_enabled: v })} />
          <Field label="Shortest Quick PIN">
            <div className="flex gap-2">
              {[4, 5, 6].map((n) => (
                <button key={n} type="button" onClick={() => setD({ ...d, quick_pin_min_length: n })}
                  className={cn("h-9 rounded-lg border px-4 text-sm", d.quick_pin_min_length === n ? "border-primary bg-primary/5 font-medium" : "bg-card")}>{n} {t("digits")}</button>
              ))}
            </div>
          </Field>
          <div className="grid gap-3 py-2 sm:grid-cols-3">
            <Field label="Quick sign-in lasts (hours)" hint="1–24"><Input type="number" min={1} max={24} value={d.quick_session_hours} onChange={(e) => setD({ ...d, quick_session_hours: Number(e.target.value) || 1 })} /></Field>
            <Field label="Wrong PINs before a lock" hint="3–10; twice this needs a full sign-in"><Input type="number" min={3} max={10} value={d.max_attempts} onChange={(e) => setD({ ...d, max_attempts: Number(e.target.value) || 3 })} /></Field>
            <Field label="Trusted device lifetime (days)" hint="7–365"><Input type="number" min={7} max={365} value={d.device_days} onChange={(e) => setD({ ...d, device_days: Number(e.target.value) || 7 })} /></Field>
          </div>
          <Fact label="Full sign-in still required for">{t("payments, roles & access, security settings, platform administration")}</Fact>
        </Card>
      )}
    </SettingsPage>
  );
}
