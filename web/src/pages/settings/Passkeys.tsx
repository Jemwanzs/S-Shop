/** Face ID & Biometric Login (roadmap 132): the person's own passkeys (User preferences, below Quick Login PIN). The
 * device checks the face, fingerprint or screen lock itself — S'Shop never receives or stores biometric data. */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fingerprint, ScanFace, ShieldCheck, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { ago, date } from "@/lib/format";
import { t } from "@/lib/i18n";
import { biometricLabel, forgetPasskey, PasskeyCancelled, passkeyDevice, passkeysSupported, platformAuthenticator, registerPasskey, rememberPasskey } from "@/lib/passkey";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/PasswordInput";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ConfirmDialog, Field, ToggleRow } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { Card } from "./shared";

interface Status {
  available: boolean;
  reason: string | null;
  enabled: boolean;
  passkeys: { id: string; credential_id: string; name: string; created_at: string; last_used_at: string | null }[];
  quick_session: boolean;
  rp_id: string | null;
}

const TITLE = "Face ID & Biometric Login";

export function PasskeyCard() {
  const qc = useQueryClient();
  const { profile } = useSession();
  const q = useQuery({ queryKey: ["passkeys"], queryFn: () => api<Status>("/auth/passkeys") });
  const [setup, setSetup] = useState(false);
  const [confirmOff, setConfirmOff] = useState(false);
  const [here, setHere] = useState(passkeyDevice);
  const [builtIn, setBuiltIn] = useState<boolean | null>(null);
  useEffect(() => { void platformAuthenticator().then(setBuiltIn); }, []);
  const refresh = () => qc.invalidateQueries({ queryKey: ["passkeys"] });
  const off = useMutation({
    mutationFn: () => api("/auth/passkeys/disable", { method: "POST" }),
    onSuccess: () => { forgetPasskey(); setHere(null); setConfirmOff(false); toast.success("Face ID & biometric login switched off on every device"); refresh(); },
    onError: (e) => toast.error(e),
  });
  const remove = useMutation({
    mutationFn: (id: string) => api(`/auth/passkeys/${id}`, { method: "DELETE" }),
    onSuccess: (_r, id) => {
      if (q.data?.passkeys.some((p) => p.id === id && p.credential_id === here?.credential_id)) { forgetPasskey(); setHere(null); }
      toast.success("Device removed");
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  if (q.isLoading || !q.data) return <Card title={TITLE}><Loading /></Card>;
  const s = q.data;
  const supported = passkeysSupported();
  // Passkeys only work on S'Shop's own address (not on a business's custom domain or an old link).
  const rightSite = !s.rp_id || location.hostname === s.rp_id || location.hostname.endsWith(`.${s.rp_id}`);
  const thisDevice = !!here && s.passkeys.some((p) => p.credential_id === here.credential_id);
  const canSetUp = s.available && !s.quick_session && supported && rightSite;
  return (
    <Card title={TITLE} action={s.enabled && <Pill tone="success">{t("On")}</Pill>}>
      <div className="space-y-3 py-2 text-sm">
        <p className="text-muted-foreground">
          {t("Sign in with your device's Face ID, fingerprint or screen lock. Your face and fingerprint never leave your device — S'Shop never sees or stores them.")}
        </p>
        {!s.available ? (
          <p className="rounded-lg bg-muted/60 p-2.5 text-xs text-muted-foreground">{t(s.reason ?? "Face ID & biometric sign-in is not available")}</p>
        ) : !supported ? (
          <p className="rounded-lg bg-muted/60 p-2.5 text-xs text-muted-foreground">{t("This browser does not support Face ID & biometric sign-in. Use Safari or Chrome, up to date.")}</p>
        ) : !rightSite ? (
          <p className="rounded-lg bg-muted/60 p-2.5 text-xs text-muted-foreground">{t("Set this up from S'Shop's own address")}: {s.rp_id}</p>
        ) : s.quick_session ? (
          <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t("Sign in with your email and full PIN to change Face ID & biometric settings.")}</p>
        ) : null}
        <ToggleRow label="Enable Face ID & Biometric Login" hint={builtIn === false ? "No built-in Face ID or fingerprint found — a phone nearby or a security key can still be used." : `${t("Uses")} ${t(biometricLabel())}`}
          checked={s.enabled} disabled={!canSetUp && !s.enabled}
          onChange={(v) => (v ? setSetup(true) : setConfirmOff(true))} />
        {s.enabled && canSetUp && !thisDevice && (
          <Button size="sm" variant="outline" onClick={() => setSetup(true)}><ScanFace /> {t("Set up this device")}</Button>
        )}
      </div>
      {s.passkeys.length > 0 && (
        <div className="border-t py-2">
          <p className="label-caps py-1">{t("Registered devices")}</p>
          {s.passkeys.map((p) => (
            <div key={p.id} className="flex items-center gap-3 py-2 text-sm">
              <Fingerprint className="h-4 w-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">{p.name}{here?.credential_id === p.credential_id && <span className="ms-1.5 text-xs font-normal text-primary">({t("this device")})</span>}</p>
                <p className="truncate text-xs text-muted-foreground">{t("Added")} {date(p.created_at)} · {p.last_used_at ? `${t("used")} ${ago(p.last_used_at)}` : t("not used yet")}</p>
              </div>
              <Button variant="ghost" size="icon-sm" aria-label={t("Remove device")} disabled={remove.isPending} onClick={() => remove.mutate(p.id)}><Trash2 /></Button>
            </div>
          ))}
        </div>
      )}
      {setup && profile && (
        <SetupDialog onClose={() => setSetup(false)} onDone={(credential_id) => {
          const d = { credential_id, email: profile.user.email, name: profile.user.name, business: profile.tenant.name, slug: profile.tenant.slug };
          rememberPasskey(d);
          setHere(d);
          setSetup(false);
          refresh();
        }} />
      )}
      <ConfirmDialog open={confirmOff} onOpenChange={setConfirmOff} title="Switch off Face ID & Biometric Login?" description="Every registered device is removed. You can set it up again at any time."
        confirmLabel="Switch off" destructive busy={off.isPending} onConfirm={() => off.mutate()} />
    </Card>
  );
}

function SetupDialog({ onClose, onDone }: { onClose: () => void; onDone: (credentialId: string) => void }) {
  const [current, setCurrent] = useState("");
  const [name, setName] = useState("");
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Set up Face ID & Biometric Login"
      description={`${t("Confirm it is you, then your device asks for")} ${t(biometricLabel())}.`}
      footer={<ActionButton online blockedBy={[!current && "Enter your current PIN"]}
        onAction={async () => {
          try {
            const r = await registerPasskey(current, name);
            toast.success("Face ID & biometric login is on for this device");
            onDone(r.credential_id);
          } catch (e) {
            if (e instanceof PasskeyCancelled) {
              toast.error("Set-up cancelled on the device");
              return;
            }
            throw e;
          }
        }}><ScanFace /> {t("Continue")}</ActionButton>}>
      <div className="space-y-3">
        <Field label="Your current full PIN" hint="Confirms it is you"><PasswordInput value={current} onChange={(e) => setCurrent(e.target.value)} autoComplete="current-password" /></Field>
        <Field label="Device name" optional hint="e.g. My iPhone"><Input value={name} maxLength={60} onChange={(e) => setName(e.target.value)} /></Field>
        <p className="flex gap-2 text-xs text-muted-foreground"><ShieldCheck className="h-4 w-4 shrink-0 text-primary" />{t("Payments, roles, access and security settings still ask for your full sign-in.")}</p>
      </div>
    </ResponsiveDialog>
  );
}
