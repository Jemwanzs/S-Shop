import { useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { Loader2, Mail } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import type { Profile } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/PasswordInput";
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

  if (profile) return <Navigate to="/" replace />;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      const res = await api<{ token: string; profile: Profile }>("/auth/login", { body: { email: email.trim(), pin } });
      signIn(res.token, res.profile);
      const preset = res.profile.branches.some((b) => b.id === res.profile.user.default_branch_id);
      navigate(res.profile.branches.length > 1 && !preset ? "/select-branch" : "/", { replace: true });
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
