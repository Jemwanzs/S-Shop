/** Face ID & Biometric Login (roadmap 132): the person's own passkeys (User preferences, below Quick Login PIN). The
 * device checks the face, fingerprint or screen lock itself — S'Shop never receives or stores biometric data. */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fingerprint, ScanFace, ShieldCheck, Trash2 } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { ago, date } from "@/lib/format";
import { t } from "@/lib/i18n";
import { finishRegistration, forgetPasskey, onPasskeySite, PasskeyCancelled, passkeyDevice, PasskeyProblem, passkeysSupported, passkeyWording, platformAuthenticator, prepareRegistration, rememberPasskey, signalAccepted, type PreparedRegistration } from "@/lib/passkey";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/PasswordInput";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ConfirmDialog, Field, ToggleRow } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { Card } from "./shared";

interface Status {
  available: boolean;
  reason: string | null;
  enabled: boolean;
  passkeys: { id: string; credential_id: string; name: string; created_at: string; last_used_at: string | null }[];
  quick_session: boolean;
  rp_id: string | null;
  user_handle: string;
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
    onSuccess: () => {
      forgetPasskey();
      setHere(null);
      setConfirmOff(false);
      if (q.data) signalAccepted(q.data.rp_id, q.data.user_handle, []);
      toast.success("Face ID & biometric login switched off on every device");
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const remove = useMutation({
    mutationFn: (id: string) => api(`/auth/passkeys/${id}`, { method: "DELETE" }),
    onSuccess: (_r, id) => {
      if (q.data?.passkeys.some((p) => p.id === id && p.credential_id === here?.credential_id)) { forgetPasskey(); setHere(null); }
      if (q.data) signalAccepted(q.data.rp_id, q.data.user_handle, q.data.passkeys.filter((p) => p.id !== id).map((p) => p.credential_id));
      toast.success("Device removed");
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  if (q.isLoading || !q.data) return <Card title={TITLE}><Loading /></Card>;
  const s = q.data;
  const supported = passkeysSupported();
  // Passkeys only work on S'Shop's own address (not on a business's custom domain or an old link).
  const rightSite = onPasskeySite(s.rp_id);
  const thisDevice = !!here && s.passkeys.some((p) => p.credential_id === here.credential_id);
  // The browser remembers a passkey the server no longer has (removed elsewhere, PIN changed).
  const unavailable = !!here && !thisDevice;
  const canSetUp = s.available && !s.quick_session && supported && rightSite;
  const words = passkeyWording();
  // Roadmap 155: what the card says always follows the server's list of passkeys, never a saved preference.
  const [state, tone]: [string, Tone] = !supported ? ["Unsupported browser or device", "neutral"]
    : !s.available ? ["Not available", "neutral"]
    : thisDevice ? ["Enabled on this device", "success"]
    : unavailable ? ["Passkey unavailable", "warning"]
    : s.enabled ? ["Enabled on another device", "info"]
    : canSetUp ? ["Ready to set up", "info"]
    : ["Not configured", "neutral"];
  return (
    <Card title={TITLE} action={<Pill tone={tone}>{t(state)}</Pill>}>
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
        ) : unavailable ? (
          <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t("The passkey on this device is no longer active. Set this device up again to keep using it.")}</p>
        ) : null}
        <ToggleRow label={words.toggle} hint={builtIn === false ? "No built-in Face ID or fingerprint found — a phone nearby or a security key can still be used." : words.hint}
          checked={s.enabled} disabled={s.enabled ? s.quick_session : !canSetUp}
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
              <Button variant="ghost" size="icon-sm" aria-label={t("Remove device")} disabled={remove.isPending || s.quick_session} onClick={() => remove.mutate(p.id)}><Trash2 /></Button>
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

/** Roadmap 155: two steps on purpose. The PIN is checked first; the device's check then starts directly from its own tap,
 * because iPhones refuse to show the prompt after the page has waited on the network. The switch turns on only when the
 * server has verified the passkey. */
function SetupDialog({ onClose, onDone }: { onClose: () => void; onDone: (credentialId: string) => void }) {
  const [current, setCurrent] = useState("");
  const [name, setName] = useState("");
  const [prepared, setPrepared] = useState<PreparedRegistration | null>(null);
  const [waiting, setWaiting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const words = passkeyWording();
  const askDevice = async () => {
    if (!prepared || waiting) return;
    setProblem(null);
    setWaiting(true);
    try {
      const r = await finishRegistration(prepared, name);
      toast.success("Passkey set up for this device");
      onDone(r.credential_id);
    } catch (e) {
      if (e instanceof PasskeyCancelled) setProblem(t("Set-up was cancelled, timed out or not allowed by the device. Nothing was changed — try again."));
      else if (e instanceof PasskeyProblem) setProblem(t(e.message));
      else {
        // The server refused the answer, which uses up the request: start again from the PIN.
        setProblem(`${t("Registration failed")}: ${errorMessage(e)}`);
        setPrepared(null);
      }
    } finally {
      setWaiting(false);
    }
  };
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Set up Face ID & Biometric Login"
      description={prepared ? words.hint : "Confirm it is you first."}
      footer={prepared ? (
        <Button className="w-full md:w-auto" disabled={waiting} onClick={askDevice}><ScanFace /> {t(waiting ? "Waiting for your device…" : words.action)}</Button>
      ) : (
        <ActionButton online blockedBy={[!current && "Enter your current PIN"]}
          onAction={async () => {
            setProblem(null);
            try {
              setPrepared(await prepareRegistration(current));
              setCurrent("");
            } catch (e) {
              // ActionButton leaves reporting to its caller: without this a wrong PIN would simply look like nothing happened.
              setProblem(errorMessage(e));
            }
          }}><ScanFace /> {t("Continue")}</ActionButton>
      )}>
      <div className="space-y-3">
        {!prepared && (
          <>
            <Field label="Your current full PIN" hint="Confirms it is you"><PasswordInput value={current} onChange={(e) => setCurrent(e.target.value)} autoComplete="current-password" /></Field>
            <Field label="Device name" optional hint="e.g. My iPhone"><Input value={name} maxLength={60} onChange={(e) => setName(e.target.value)} /></Field>
          </>
        )}
        {prepared && <p className="text-sm">{t("Your PIN is confirmed. Tap the button below and follow your device's prompt.")}</p>}
        {problem && <p role="alert" className="rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">{problem}</p>}
        <p className="flex gap-2 text-xs text-muted-foreground"><ShieldCheck className="h-4 w-4 shrink-0 text-primary" />{t("Payments, roles, access and security settings still ask for your full sign-in.")}</p>
      </div>
    </ResponsiveDialog>
  );
}
