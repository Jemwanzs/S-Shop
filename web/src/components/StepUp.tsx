/**
 * Roadmap 105 — signed in with a Quick PIN, a sensitive action answers "Full sign-in needed". Instead of failing,
 * the person confirms their full PIN / password here; the session becomes a full one and the same request is sent
 * again, so whatever they entered on screen stays as it is. Cancelling shows the original refusal.
 *
 * Also the once-only Quick PIN offer after a person set up their account.
 */
import { useEffect, useState } from "react";
import { useNavigate } from "@/lib/router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Fingerprint, ShieldCheck } from "lucide-react";
import { api, errorMessage, onStepUp } from "@/lib/api";
import { useSession } from "@/lib/session";
import { t } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { PasswordInput } from "@/components/PasswordInput";
import { SShopLoader } from "@/components/SShopLoader";

export function StepUpHost() {
  const { profile, renewToken } = useSession();
  const [ask, setAsk] = useState<((ok: boolean) => void) | null>(null);
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    // Several requests refused at once share one confirmation.
    let pending: Promise<boolean> | null = null;
    onStepUp(() => {
      pending ??= new Promise<boolean>((resolve) => {
        setPin("");
        setError("");
        setAsk(() => (ok: boolean) => {
          pending = null;
          setAsk(null);
          resolve(ok);
        });
      });
      return pending;
    });
    return () => onStepUp(null);
  }, []);

  const confirm = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (!ask || pin.length < 4) return;
    setBusy(true);
    setError("");
    try {
      const r = await api<{ token: string }>("/auth/step-up", { body: { pin } });
      await renewToken(r.token);
      ask(true);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ResponsiveDialog open={!!ask} onOpenChange={(o) => !o && ask?.(false)} confirmDiscard={false} title="Confirm it's you"
      description="This needs your full sign-in. What you entered on screen stays as it is."
      footer={
        <div className="flex w-full flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <Button variant="outline" onClick={() => ask?.(false)}>{t("Cancel")}</Button>
          <Button onClick={() => void confirm()} disabled={busy || pin.length < 4}>{busy ? <SShopLoader size="sm" /> : <ShieldCheck />} {t("Continue")}</Button>
        </div>
      }>
      <form onSubmit={confirm} className="space-y-3 py-1">
        <p className="text-sm">{t("Enter the PIN / password you sign in with for")} <b>{profile?.user.email}</b>. {t("Your Quick PIN is not accepted here.")}</p>
        <input type="email" name="username" autoComplete="username" value={profile?.user.email ?? ""} readOnly hidden />
        <PasswordInput autoComplete="current-password" autoFocus maxLength={12} value={pin} onChange={(e) => setPin(e.target.value)} aria-label={t("PIN / password")} />
        {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
      </form>
    </ResponsiveDialog>
  );
}

/** Once, after a person set up their account: offer Quick PIN on this device (if the business allows it). */
export function QuickPinOffer() {
  const { profile } = useSession();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const offer = !!profile?.user.quick_pin_offer && !profile.quick;
  const q = useQuery({
    queryKey: ["quick-pin"],
    queryFn: () => api<{ available: boolean; enabled: boolean }>("/auth/quick-pin"),
    enabled: offer,
  });
  const [closed, setClosed] = useState(false);
  const show = offer && !closed && !!q.data?.available && !q.data.enabled;
  const done = async (setUp: boolean) => {
    setClosed(true);
    await api("/auth/quick-pin/offer/dismiss", { method: "POST" }).catch(() => undefined);
    void qc.invalidateQueries({ queryKey: ["me"] });
    if (setUp) navigate("/settings/preferences");
  };
  return (
    <ResponsiveDialog open={show} onOpenChange={(o) => !o && void done(false)} confirmDiscard={false} title="Enable Quick PIN login?"
      footer={
        <div className="flex w-full flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <Button variant="outline" onClick={() => void done(false)}>{t("Maybe later")}</Button>
          <Button onClick={() => void done(true)}><Fingerprint /> {t("Set up Quick PIN")}</Button>
        </div>
      }>
      <p className="py-1 text-sm">{t("Sign in faster on this trusted device using a secure 4–6 digit PIN. You can turn it on or off any time in User preferences → Security.")}</p>
    </ResponsiveDialog>
  );
}
