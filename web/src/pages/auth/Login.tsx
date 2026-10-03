import { useState } from "react";
import { Navigate, useNavigate } from "react-router-dom";
import { Eye, EyeOff, Loader2, Lock, Mail } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import type { Profile } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ThemeToggle } from "@/components/layout/AppShell";
import mark from "@/assets/sshop-mark.png";
import stacked from "@/assets/sshop-logo-stacked.png";

export default function LoginPage() {
  const { profile, signIn } = useSession();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [pin, setPin] = useState("");
  const [show, setShow] = useState(false);
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
      navigate(res.profile.branches.length > 1 ? "/select-branch" : "/", { replace: true });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="grid min-h-screen lg:grid-cols-[1.1fr_1fr]">
      {/* Brand panel (wide screens) */}
      {/* Fixed light panel in both themes: the S'Shop logo lettering is dark. */}
      <div className="relative hidden overflow-hidden bg-[#fffaf4] text-[#1c1410] lg:flex lg:flex-col lg:justify-between lg:p-12">
        <div className="bg-brand absolute -right-28 -top-28 h-96 w-96 rounded-full opacity-25 blur-3xl" />
        <div className="bg-brand absolute -bottom-36 -left-24 h-96 w-96 rounded-full opacity-15 blur-3xl" />
        <img src={stacked} alt="S'Shop — Everything you love in one place" className="relative h-auto w-56 xl:w-64" />
        <div className="relative max-w-lg space-y-4">
          <h1 className="text-4xl font-semibold leading-tight xl:text-5xl">
            Sell, stock and reward — <span className="text-brand">from one phone.</span>
          </h1>
          <p className="text-lg text-[#1c1410]/70">Inventory, point of sale, customer orders, credit and loyalty for every branch of your business.</p>
        </div>
        <p className="relative text-sm text-[#1c1410]/50">Inventory is the backbone. Customers are the heart.</p>
      </div>

      <div className="flex min-h-screen flex-col px-5 py-6">
        <div className="flex justify-end"><ThemeToggle /></div>
        <div className="mx-auto flex w-full max-w-sm flex-1 flex-col justify-center">
          <div className="mb-8 text-center animate-fade-up">
            <div className="mb-6 lg:hidden">
              <img src={mark} alt="" className="mx-auto h-20 w-20" />
              <p className="text-brand mt-1 text-3xl font-bold tracking-tight">S'Shop</p>
              <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">Everything you love in one place</p>
            </div>
            <h2 className="text-2xl font-semibold">Welcome back</h2>
            <p className="mt-1 text-sm text-muted-foreground">Sign in with your email and PIN</p>
          </div>
          <form onSubmit={submit} className="space-y-4 animate-fade-up">
            <label className="block space-y-1.5">
              <span className="text-sm font-medium">Email</span>
              <div className="relative">
                <Mail className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input type="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="you@business.com" className="h-12 pl-9" required />
              </div>
            </label>
            <label className="block space-y-1.5">
              <span className="text-sm font-medium">PIN</span>
              <div className="relative">
                <Lock className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  type={show ? "text" : "password"}
                  inputMode="numeric"
                  autoComplete="current-password"
                  value={pin}
                  onChange={(e) => setPin(e.target.value)}
                  placeholder="••••"
                  className="num h-12 pl-9 pr-11 tracking-[0.3em]"
                  required
                />
                <button type="button" onClick={() => setShow(!show)} className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1.5 text-muted-foreground" aria-label={show ? "Hide PIN" : "Show PIN"}>
                  {show ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </label>
            {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>}
            <Button type="submit" size="lg" variant="ink" className="w-full" disabled={busy}>
              {busy ? <Loader2 className="animate-spin" /> : "Sign in"}
            </Button>
            <p className="text-center text-xs text-muted-foreground">Forgot your PIN? Ask an administrator to reset it.</p>
          </form>
        </div>
      </div>
    </div>
  );
}
