/** Self-service account recovery (roadmap 60–61): Forgot PIN, set a PIN from an emailed link, applicant status,
 * and the first sign-in with a one-time PIN. Links carry their token in the URL fragment (#t=…), never sent to servers. */
import { useEffect, useState, type ReactNode } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { CheckCircle2, Clock, Loader2, Mail, MessageCircle, Phone, ShieldCheck, XCircle } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/PasswordInput";
import { AuthLabel, AuthLayout } from "./AuthLayout";
import { t } from "@/lib/i18n";

export const SUPPORT_PHONES = ["0798 993 404", "0732 968 898"];
const intl = (p: string) => "254" + p.replace(/\D/g, "").replace(/^0/, "");

/** Support contacts with Call and WhatsApp. */
export function SupportContacts({ title = "Need assistance? Contact S'Shop Support" }: { title?: string }) {
  return (
    <div className="rounded-xl bg-muted/60 p-3 text-center">
      <p className="text-xs text-muted-foreground">{t(title)}</p>
      <div className="mt-2 space-y-1.5">
        {SUPPORT_PHONES.map((p) => (
          <div key={p} className="flex items-center justify-center gap-2">
            <span className="num text-sm font-semibold">{p}</span>
            <a href={`tel:${p.replace(/\s/g, "")}`} className="inline-flex items-center gap-1 rounded-full border bg-card px-2 py-0.5 text-xs text-primary" aria-label={`${t("Call")} ${p}`}><Phone className="h-3 w-3" /> {t("Call")}</a>
            <a href={`https://wa.me/${intl(p)}`} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 rounded-full border bg-card px-2 py-0.5 text-xs text-success" aria-label={`WhatsApp ${p}`}><MessageCircle className="h-3 w-3" /> WhatsApp</a>
          </div>
        ))}
      </div>
    </div>
  );
}

function linkToken(): string {
  const m = /[#&]t=([A-Za-z0-9_-]+)/.exec(window.location.hash);
  return m ? m[1] : "";
}

const pinProblem = (pin: string, confirm: string) =>
  pin.length < 4 || pin.length > 12 ? "Your PIN needs 4–12 characters" : confirm !== pin ? "The two PINs do not match" : null;

// ── Forgot PIN / Password ─────────────────────────────────────────────

export function ForgotPage() {
  const [params] = useSearchParams();
  const [email, setEmail] = useState(params.get("email") ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [sent, setSent] = useState(false);
  const [cooldown, setCooldown] = useState(0);
  useEffect(() => {
    if (cooldown <= 0) return;
    const id = setTimeout(() => setCooldown((c) => c - 1), 1000);
    return () => clearTimeout(id);
  }, [cooldown]);
  const submit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    setError("");
    setBusy(true);
    try {
      await api("/auth/forgot", { body: { email: email.trim() }, token: null });
      setSent(true);
      setCooldown(60);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <AuthLayout title="Forgot PIN / Password?" subtitle={sent ? undefined : "Enter the email you sign in with"}>
      {sent ? (
        <div className="space-y-4 text-center">
          <Mail className="mx-auto h-10 w-10 text-primary" />
          <p className="text-sm">{t("If this email is registered, we'll send you a secure link to reset your PIN or password.")}</p>
          <p className="text-xs text-muted-foreground">{t("Applied for access? You'll receive a link to your request's status instead. Links expire after 30 minutes.")}</p>
          <div className="grid grid-cols-2 gap-2">
            <Button variant="outline" asChild><Link to="/login">{t("Back to Sign In")}</Link></Button>
            <Button variant="outline" onClick={() => submit()} disabled={busy || cooldown > 0}>{busy ? <Loader2 className="animate-spin" /> : cooldown > 0 ? `${t("Resend Link")} (${cooldown})` : t("Resend Link")}</Button>
          </div>
          <SupportContacts title="Can't reach that email? Contact S'Shop Support" />
        </div>
      ) : (
        <form onSubmit={submit} className="space-y-4">
          <label className="block">
            <AuthLabel>{t("Email")}</AuthLabel>
            <div className="relative">
              <Mail className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
              <Input type="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="your@email.com" className="ps-9" required autoFocus />
            </div>
          </label>
          {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
          <Button type="submit" className="w-full" disabled={busy || !email.includes("@")}>{busy ? <Loader2 className="animate-spin" /> : t("Send Reset Link")}</Button>
          <Link to="/login" className="block text-center text-xs text-muted-foreground hover:text-foreground">{t("Back to Sign In")}</Link>
        </form>
      )}
    </AuthLayout>
  );
}

// ── Set a PIN from a welcome (set-up) or reset link ───────────────────

interface LinkInfo { kind: "setup" | "reset"; name: string; email: string; business: string; expires_at: string }

export function SetPinPage() {
  const [token] = useState(linkToken);
  const [info, setInfo] = useState<LinkInfo | null>(null);
  const [linkError, setLinkError] = useState("");
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [done, setDone] = useState<string | null>(null);
  useEffect(() => {
    // Keep the token out of the address bar (history, screenshots) once read.
    if (window.location.hash) history.replaceState(null, "", window.location.pathname);
    if (!token) {
      setLinkError("This link is incomplete. Open the link from your email again, or request a new one.");
      return;
    }
    api<LinkInfo>("/auth/link", { body: { token }, token: null }).then(setInfo).catch((e) => setLinkError(errorMessage(e)));
  }, [token]);
  const problem = pinProblem(pin, confirm);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (problem) return;
    setBusy(true);
    setError("");
    try {
      const r = await api<{ email: string }>("/auth/set-pin", { body: { token, pin }, token: null });
      setDone(r.email);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };
  if (done) {
    return (
      <AuthLayout title="Your PIN is set">
        <div className="space-y-4 text-center">
          <CheckCircle2 className="mx-auto h-10 w-10 text-success" />
          <p className="text-sm">{t("Sign in with your email and your new PIN. Any other signed-in devices have been signed out.")}</p>
          <Button className="w-full" asChild><Link to={`/login?email=${encodeURIComponent(done)}`}>{t("Sign in")}</Link></Button>
        </div>
      </AuthLayout>
    );
  }
  if (linkError) {
    return (
      <AuthLayout title="Link expired">
        <div className="space-y-4 text-center">
          <XCircle className="mx-auto h-10 w-10 text-destructive" />
          <p className="text-sm">{t(linkError)}</p>
          <Button className="w-full" asChild><Link to="/forgot">{t("Request a new link")}</Link></Button>
          <SupportContacts />
        </div>
      </AuthLayout>
    );
  }
  if (!info) return <AuthLayout title="Checking your link…"><Loader2 className="mx-auto animate-spin" /></AuthLayout>;
  return (
    <AuthLayout title={info.kind === "setup" ? "Set up your account" : "Choose a new PIN"} subtitle={info.business}>
      <form onSubmit={submit} className="space-y-4">
        <p className="text-sm">{t("Hello")} {info.name} — <span className="text-muted-foreground">{info.email}</span></p>
        <input type="email" name="username" autoComplete="username" value="" readOnly hidden />
        <label className="block"><AuthLabel>{t("New PIN")}</AuthLabel><PasswordInput withIcon autoComplete="new-password" value={pin} onChange={(e) => setPin(e.target.value)} maxLength={12} autoFocus /></label>
        <label className="block"><AuthLabel>{t("Confirm new PIN")}</AuthLabel><PasswordInput withIcon autoComplete="new-password" value={confirm} onChange={(e) => setConfirm(e.target.value)} maxLength={12} /></label>
        <p className="text-xs text-muted-foreground">{t("4–12 characters — letters, numbers and symbols allowed")}</p>
        {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
        <Button type="submit" className="w-full" disabled={busy || !!problem}>{busy ? <Loader2 className="animate-spin" /> : problem && (pin || confirm) ? t(problem) : t("Save PIN")}</Button>
      </form>
    </AuthLayout>
  );
}

// ── Access request status (reachable only through the emailed link) ───

interface Status { status: "pending" | "approved_setup" | "approved_active" | "rejected"; business: string; name: string; sign_in_url: string }

export function AccessStatusPage() {
  const [token] = useState(linkToken);
  const [s, setS] = useState<Status | null>(null);
  const [err, setErr] = useState("");
  const [resent, setResent] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!token) {
      setErr("This link is incomplete. Open the link from your email again, or request a new one.");
      return;
    }
    api<Status>("/auth/request-status", { body: { token }, token: null }).then(setS).catch((e) => setErr(errorMessage(e)));
  }, [token]);
  const resend = async () => {
    setBusy(true);
    try {
      const r = await api<{ sent: boolean }>("/auth/request-status/resend-setup", { body: { token }, token: null });
      setResent(r.sent);
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  if (err) {
    return (
      <AuthLayout title="Link expired">
        <div className="space-y-4 text-center">
          <XCircle className="mx-auto h-10 w-10 text-destructive" />
          <p className="text-sm">{t(err)}</p>
          <Button className="w-full" asChild><Link to="/forgot">{t("Request a new link")}</Link></Button>
          <SupportContacts />
        </div>
      </AuthLayout>
    );
  }
  if (!s) return <AuthLayout title="Checking your link…"><Loader2 className="mx-auto animate-spin" /></AuthLayout>;
  const view: Record<Status["status"], { icon: ReactNode; title: string; text: string }> = {
    pending: { icon: <Clock className="mx-auto h-10 w-10 text-warning" />, title: "Under review", text: "Your access request is currently under review. Please allow us a little time to complete the approval process." },
    approved_setup: { icon: <ShieldCheck className="mx-auto h-10 w-10 text-success" />, title: "Approved", text: "Your S'Shop access has been approved. Please check your email for your account setup instructions." },
    approved_active: { icon: <CheckCircle2 className="mx-auto h-10 w-10 text-success" />, title: "Approved", text: "Your S'Shop account is ready. Sign in with your email and PIN." },
    rejected: { icon: <XCircle className="mx-auto h-10 w-10 text-destructive" />, title: "Not approved", text: "Your access request could not be approved at this time. Please contact S'Shop Support for assistance or further clarification." },
  };
  const v = view[s.status];
  return (
    <AuthLayout title={v.title} subtitle={s.business}>
      <div className="space-y-4 text-center">
        {v.icon}
        <p className="text-sm font-medium">{t("Hello")} {s.name},</p>
        <p className="text-sm">{t(v.text)}</p>
        {s.status === "approved_setup" && (
          resent === null ? (
            <Button className="w-full" onClick={resend} disabled={busy}>{busy ? <Loader2 className="animate-spin" /> : t("Resend Setup Instructions")}</Button>
          ) : (
            <p className="rounded-lg bg-success/10 p-2.5 text-xs text-success">{t("If your account still needs setting up, a new email is on its way.")}</p>
          )
        )}
        {s.status !== "pending" && s.status !== "rejected" && <Button variant="outline" className="w-full" asChild><Link to="/login">{t("Sign in")}</Link></Button>}
        <SupportContacts />
      </div>
    </AuthLayout>
  );
}

// ── First sign-in with a one-time PIN ─────────────────────────────────

export function ForcedPinChange() {
  const { profile, renewToken, signOut } = useSession();
  const [current, setCurrent] = useState("");
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const problem = !current ? "Enter your one-time PIN" : pin === current ? "Choose a PIN different from the one-time PIN" : pinProblem(pin, confirm);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (problem) return;
    setBusy(true);
    setError("");
    try {
      const r = await api<{ token: string }>("/auth/change-pin", { body: { current_pin: current, new_pin: pin } });
      await renewToken(r.token);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <AuthLayout title="Create your own PIN" subtitle={profile?.tenant.name}>
      <form onSubmit={submit} className="space-y-4">
        <p className="text-sm">{t("Welcome")} {profile?.user.name.split(" ")[0]}! {t("You signed in with a one-time PIN. Replace it with your own PIN to continue.")}</p>
        <input type="email" name="username" autoComplete="username" value={profile?.user.email ?? ""} readOnly hidden />
        <label className="block"><AuthLabel>{t("One-time PIN")}</AuthLabel><PasswordInput withIcon autoComplete="current-password" value={current} onChange={(e) => setCurrent(e.target.value)} maxLength={12} autoFocus /></label>
        <label className="block"><AuthLabel>{t("New PIN")}</AuthLabel><PasswordInput withIcon autoComplete="new-password" value={pin} onChange={(e) => setPin(e.target.value)} maxLength={12} /></label>
        <label className="block"><AuthLabel>{t("Confirm new PIN")}</AuthLabel><PasswordInput withIcon autoComplete="new-password" value={confirm} onChange={(e) => setConfirm(e.target.value)} maxLength={12} /></label>
        {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
        <Button type="submit" className="w-full" disabled={busy || !!problem}>{busy ? <Loader2 className="animate-spin" /> : problem && (current || pin || confirm) ? t(problem) : t("Save PIN & continue")}</Button>
        <button type="button" onClick={signOut} className="block w-full text-center text-xs text-muted-foreground hover:text-foreground">{t("Sign out")}</button>
      </form>
    </AuthLayout>
  );
}
