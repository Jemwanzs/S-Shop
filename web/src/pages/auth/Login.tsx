import { useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { Loader2, Mail } from "lucide-react";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import type { Profile } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/PasswordInput";
import { PinPad } from "@/components/PinPad";
import { forgetQuickDevice, preferredMethod, quickDevice, rememberMethod } from "@/lib/quick";
import { AuthLabel, AuthLayout } from "./AuthLayout";
import { t } from "@/lib/i18n";

export default function LoginPage() {
  const { profile, signIn } = useSession();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [email, setEmail] = useState(params.get("email") ?? "");
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  // Roadmap 83: a trusted device opens on the Quick PIN keypad (unless this device last chose the full sign-in).
  const [device, setDevice] = useState(quickDevice);
  const [mode, setMode] = useState<"quick" | "full">(() => (quickDevice() && preferredMethod() === "quick" && !params.get("email") ? "quick" : "full"));
  const [quick, setQuick] = useState("");
  const [shake, setShake] = useState(0);

  if (profile) return <Navigate to="/" replace />;

  const enter = (res: { token: string; profile: Profile }) => {
    signIn(res.token, res.profile);
    const preset = res.profile.branches.some((b) => b.id === res.profile.user.default_branch_id);
    navigate(res.profile.branches.length > 1 && !preset ? "/select-branch" : "/", { replace: true });
  };
  const quickLogin = async (pin: string) => {
    if (!device || busy) return;
    setError("");
    setBusy(true);
    try {
      enter(await api<{ token: string; profile: Profile }>("/auth/quick-login", { body: { device_token: device.token, pin } }));
      rememberMethod("quick");
    } catch (err) {
      setQuick("");
      setShake((n) => n + 1);
      const title = err instanceof ApiError ? err.title : undefined;
      // The device is no longer trusted (revoked, expired, Quick PIN off): back to the full sign-in for good.
      if (title === "Full sign-in needed") {
        forgetQuickDevice();
        setDevice(null);
        setEmail(device.email);
        setMode("full");
      }
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  if (mode === "quick" && device) {
    return (
      <AuthLayout title="S'Shop" subtitle={`${t("Welcome back")}, ${device.name.split(" ")[0]}`}>
        <div className="space-y-5">
          <p className="text-center text-sm text-muted-foreground">{t("Enter your Quick PIN")} · <span className="text-foreground">{device.business}</span></p>
          <PinPad key={shake} value={quick} onChange={(v) => { setQuick(v); setError(""); }} onSubmit={quickLogin} disabled={busy} error={shake > 0 && !quick} />
          {busy && <Loader2 className="mx-auto h-5 w-5 animate-spin text-primary" />}
          {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-center text-xs text-destructive">{error}</p>}
          <div className="flex items-center justify-between text-xs font-medium">
            <button type="button" className="text-primary hover:underline" onClick={() => { rememberMethod("full"); setEmail(device.email); setMode("full"); setError(""); }}>{t("Use password instead")}</button>
            <Link to={`/forgot?email=${encodeURIComponent(device.email)}`} className="text-muted-foreground hover:underline" title={t("Sign in with your full PIN, then set a new Quick PIN in your preferences")}>{t("Forgot PIN?")}</Link>
          </div>
        </div>
      </AuthLayout>
    );
  }

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      const res = await api<{ token: string; profile: Profile }>("/auth/login", { body: { email: email.trim(), pin } });
      enter(res);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthLayout title="S'Shop" subtitle="Sign in to continue">
      <form onSubmit={submit} className="space-y-4">
        <label className="block">
          <AuthLabel>{t("Email")}</AuthLabel>
          <div className="relative">
            <Mail className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input type="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="your@email.com" className="ps-9" required />
          </div>
        </label>
        <label className="block">
          <AuthLabel>{t("PIN")}</AuthLabel>
          <PasswordInput withIcon autoComplete="current-password" value={pin} onChange={(e) => setPin(e.target.value)} placeholder={t("Enter your PIN")} maxLength={12} required />
        </label>
        {error && (
          <div className="space-y-1 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            <p>{error}</p>
            {/* Never says whether the email exists; applicants get their status by email (proof they own it). */}
            <p className="text-muted-foreground">
              {t("Applied for access?")} <Link to={`/forgot?email=${encodeURIComponent(email.trim())}`} className="font-medium text-primary underline-offset-2 hover:underline">{t("Check your request status")}</Link>
            </p>
          </div>
        )}
        <Button type="submit" className="w-full" disabled={busy}>
          {busy ? <Loader2 className="animate-spin" /> : t("Sign in")}
        </Button>
        <Link to={`/forgot${email.trim() ? `?email=${encodeURIComponent(email.trim())}` : ""}`} className="block text-center text-xs font-medium text-primary hover:underline">{t("Forgot PIN / Password?")}</Link>
        {device && (
          <button type="button" className="block w-full text-center text-sm font-semibold text-primary underline underline-offset-4" onClick={() => { rememberMethod("quick"); setMode("quick"); setError(""); }}>
            {t("Login with Quick PIN")}
          </button>
        )}
      </form>
      <div className="mt-6 border-t pt-5 text-center">
        <p className="text-xs text-muted-foreground">{t("Interested in accessing S'Shop?")}</p>
        <Button variant="outline" size="sm" className="mt-2 w-full" asChild>
          <Link to="/request-access">{t("Request Access")}</Link>
        </Button>
      </div>
    </AuthLayout>
  );
}
