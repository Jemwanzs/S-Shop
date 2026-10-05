import { Suspense, useEffect, useState, type FormEvent } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Bell, Check, ChevronsUpDown, KeyRound, ShieldCheck, SlidersHorizontal, Loader2, LogOut, Menu, Moon, Search, Store, Sun } from "lucide-react";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useLiveEvents } from "@/lib/events";
import { initials } from "@/lib/format";
import type { Notification, Profile } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { PasswordInput } from "@/components/PasswordInput";
import { PoweredBy } from "@/components/PoweredBy";
import { Loading } from "@/components/Page";
import { Field } from "@/components/Form";
import { GlobalSearch } from "./GlobalSearch";
import mark from "@/assets/sshop-mark.png";
import { allowed, BOTTOM, NAV } from "./nav";
import { t } from "@/lib/i18n";

export function useTheme() {
  const [dark, setDark] = useState(() => document.documentElement.classList.contains("dark"));
  const toggle = () => {
    const next = !dark;
    document.documentElement.classList.toggle("dark", next);
    try {
      localStorage.setItem("sshop.theme", next ? "dark" : "light");
    } catch {
      /* ignore */
    }
    setDark(next);
  };
  return { dark, toggle };
}

export function ThemeToggle({ className }: { className?: string }) {
  const { dark, toggle } = useTheme();
  return (
    <Button variant="ghost" size="icon" onClick={toggle} className={className} aria-label="Toggle theme">
      {dark ? <Sun /> : <Moon />}
    </Button>
  );
}

function useNotificationCounts() {
  return useQuery({
    queryKey: ["notifications"],
    queryFn: () => api<{ items: Notification[]; unread: number; pending_approvals: number }>("/notifications", { query: { limit: 30 } }),
    refetchInterval: 120_000,
  });
}

function BranchSwitcher({ compact }: { compact?: boolean }) {
  const { profile, branch, selectBranch } = useSession();
  if (!profile || !branch) return null;
  const many = profile.branches.length > 1;
  const label = (
    <span className="flex min-w-0 items-center gap-2">
      <Store className="h-4 w-4 shrink-0 text-primary" />
      <span className="truncate">{branch.name}</span>
    </span>
  );
  if (!many) return <div className={cn("flex h-9 items-center rounded-lg px-2 text-sm font-medium", compact && "max-w-[34vw] sm:max-w-[40vw]")}>{label}</div>;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button className={cn("flex h-9 items-center gap-2 rounded-lg border bg-card px-2.5 text-sm font-medium hover:bg-accent", compact ? "max-w-[34vw] sm:max-w-[40vw]" : "w-full justify-between")}>
          {label}
          <ChevronsUpDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
        <DropdownMenuLabel>Current branch</DropdownMenuLabel>
        {profile.branches.map((b) => (
          <DropdownMenuItem key={b.id} onClick={() => { selectBranch(b.id); toast.success(`Now operating from ${b.name}`); }} className="gap-2">
            <Check className={cn("h-4 w-4", b.id === branch.id ? "opacity-100" : "opacity-0")} />
            <span className="truncate">{b.name}</span>
            <span className="ms-auto text-xs text-muted-foreground">{b.code}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function ChangePin({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const { profile } = useSession();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  // Always open empty, so a browser-filled or half-typed PIN is never sent by mistake.
  useEffect(() => {
    if (open) {
      setCurrent("");
      setNext("");
      setConfirm("");
    }
  }, [open]);
  const problem =
    !current ? "Enter your current PIN"
    : next.length < 4 || next.length > 12 ? "The new PIN needs 4–12 characters"
    : next === current ? "The new PIN must be different from the current one"
    : confirm !== next ? "The two new PINs do not match"
    : null;
  const save = async (e?: FormEvent) => {
    e?.preventDefault();
    if (problem || busy) return;
    setBusy(true);
    try {
      await api("/auth/change-pin", { body: { current_pin: current, new_pin: next } });
      toast.success("PIN changed — use the new PIN next time you sign in");
      onOpenChange(false);
    } catch (err) {
      toast.error(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Change PIN"
      description="Use the PIN you signed in with, then choose a new one."
      footer={<Button type="submit" form="change-pin" className="w-full md:w-auto" disabled={!!problem || busy}>{busy ? <Loader2 className="animate-spin" /> : t("Save PIN")}</Button>}
    >
      <form id="change-pin" onSubmit={save} className="space-y-4">
        {/* Lets password managers file the new PIN under the right account. */}
        <input type="email" name="username" autoComplete="username" value={profile?.user.email ?? ""} readOnly hidden />
        <Field label="Current PIN"><PasswordInput value={current} onChange={(e) => setCurrent(e.target.value)} autoComplete="current-password" maxLength={12} autoFocus /></Field>
        <Field label="New PIN" hint="4–12 characters — letters, numbers and symbols allowed"><PasswordInput value={next} onChange={(e) => setNext(e.target.value)} autoComplete="new-password" maxLength={12} /></Field>
        <Field label="Confirm new PIN"><PasswordInput value={confirm} onChange={(e) => setConfirm(e.target.value)} autoComplete="new-password" maxLength={12} /></Field>
        {problem && (current || next || confirm) && <p className="text-sm text-muted-foreground">{problem}</p>}
      </form>
    </ResponsiveDialog>
  );
}

function UserMenu({ full }: { full?: boolean }) {
  const { profile, signOut, displayCurrency } = useSession();
  const navigate = useNavigate();
  const [pinOpen, setPinOpen] = useState(false);
  if (!profile) return null;
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button className={cn("flex items-center gap-2.5 rounded-lg text-start hover:bg-accent", full ? "w-full p-2" : "p-1")}>
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-primary/15 text-sm font-semibold text-primary">
              {initials(profile.user.name)}
            </span>
            {full && (
              <span className="min-w-0">
                <span className="block truncate text-sm font-medium">{profile.user.name}</span>
                <span className="block truncate text-xs text-muted-foreground">{profile.user.role}</span>
              </span>
            )}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <DropdownMenuLabel className="font-normal">
            <div className="truncate font-medium">{profile.user.name}</div>
            <div className="truncate text-xs text-muted-foreground">{profile.user.email}</div>
            <div className="mt-1 text-xs font-medium text-primary">{t("Figures in")} {displayCurrency}</div>
          </DropdownMenuLabel>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => navigate("/settings/preferences")} className="gap-2"><SlidersHorizontal className="h-4 w-4" /> {t("Preferences")}</DropdownMenuItem>
          <DropdownMenuItem onClick={() => setPinOpen(true)} className="gap-2"><KeyRound className="h-4 w-4" /> {t("Change PIN")}</DropdownMenuItem>
          <DropdownMenuItem onClick={signOut} className="gap-2 text-destructive"><LogOut className="h-4 w-4" /> {t("Sign out")}</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <ChangePin open={pinOpen} onOpenChange={setPinOpen} />
    </>
  );
}

function Brand() {
  const { profile } = useSession();
  return (
    <Link to="/" className="flex min-w-0 items-center gap-2.5">
      {profile?.tenant.logo_url ? (
        <img src={profile.tenant.logo_url} alt="" className="h-9 w-9 rounded-lg object-cover" />
      ) : (
        <img src={mark} alt="S'Shop" className="h-9 w-9" />
      )}
      <span className="min-w-0">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="truncate font-semibold leading-tight">{profile?.tenant.name ?? "S'Shop"}</span>
          {profile?.tenant.is_demo && <span className="shrink-0 rounded bg-chart-2/15 px-1 text-[0.6rem] font-bold uppercase tracking-wide text-chart-2">{t("Demo")}</span>}
        </span>
        <span className="text-brand block text-[11px] font-semibold leading-tight">S'Shop</span>
      </span>
    </Link>
  );
}

/** Shown while a platform admin works inside another business: where they are and the way back. */
function ActingBanner() {
  const { profile, switchBusiness } = useSession();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  if (!profile?.acting) return null;
  const back = async () => {
    setBusy(true);
    try {
      const r = await api<{ token: string; profile: Profile }>(`/platform/tenants/${profile.acting!.home_tenant_id}/open`, { method: "POST" });
      switchBusiness(r.token, r.profile);
      navigate(r.profile.branches.length > 1 ? "/select-branch" : "/settings/businesses", { replace: true });
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex items-center gap-2 bg-foreground px-3.5 py-1.5 text-[0.78rem] text-background md:px-6 lg:px-8 print:hidden">
      <ShieldCheck className="h-3.5 w-3.5 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{t("Viewing")} <b>{profile.tenant.name}</b> {t("as platform owner")}</span>
      <button onClick={back} disabled={busy} className="shrink-0 rounded-full bg-background/15 px-2.5 py-0.5 font-medium hover:bg-background/25">
        {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : `${t("Return to")} ${profile.acting.home_tenant_name}`}
      </button>
    </div>
  );
}

function Sidebar({ approvals }: { approvals: number }) {
  const { can } = useSession();
  return (
    <aside className="sticky top-0 hidden h-screen flex-col border-e bg-card/70 backdrop-blur lg:flex print:!hidden">
      <div className="space-y-3 p-4">
        <Brand />
        <BranchSwitcher />
      </div>
      <nav className="flex-1 space-y-5 overflow-y-auto px-3 pb-4">
        {NAV.map((g) => {
          const items = g.items.filter((i) => allowed(i, can));
          if (!items.length) return null;
          return (
            <div key={g.group}>
              <p className="label-caps px-3 pb-1.5">{t(g.group)}</p>
              {items.map((i) => (
                <NavLink
                  key={i.to}
                  to={i.to}
                  end={i.to === "/"}
                  className={({ isActive }) =>
                    cn(
                      "group flex h-10 items-center gap-3 rounded-lg px-3 text-sm transition-colors",
                      isActive ? "bg-primary/10 font-medium text-primary" : "text-muted-foreground hover:bg-accent hover:text-foreground",
                    )
                  }
                >
                  <i.icon className="h-[18px] w-[18px]" />
                  <span className="flex-1 truncate">{t(i.label)}</span>
                  {i.to === "/approvals" && approvals > 0 && (
                    <span className="num rounded-full bg-primary px-1.5 text-[11px] font-semibold text-primary-foreground">{approvals}</span>
                  )}
                </NavLink>
              ))}
            </div>
          );
        })}
      </nav>
      <div className="border-t p-3">
        <UserMenu full />
      </div>
    </aside>
  );
}

function BottomNav() {
  const { can } = useSession();
  const { pathname } = useLocation();
  const items = BOTTOM.filter((i) => allowed(i, can));
  const moreActive = !items.some((i) => (i.to === "/" ? pathname === "/" : pathname.startsWith(i.to)));
  return (
    <nav className="pb-safe fixed inset-x-0 bottom-0 z-40 border-t bg-card/95 backdrop-blur lg:hidden no-print">
      <div className="mx-auto flex max-w-2xl items-stretch justify-around">
        {items.map((i) => {
          const sale = i.to === "/pos";
          return (
            <NavLink
              key={i.to}
              to={i.to}
              end={i.to === "/"}
              className={({ isActive }) => cn("relative flex h-nav flex-1 flex-col items-center justify-center gap-0.5 text-[0.7rem] font-medium", isActive || sale ? "text-primary" : "text-muted-foreground")}
            >
              {({ isActive }) => (
                <>
                  {isActive && <span className="absolute top-0 h-0.5 w-8 rounded-full bg-primary" />}
                  {sale ? (
                    <span className="-mt-4 flex h-11 w-11 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lift ring-4 ring-background">
                      <i.icon className="h-5 w-5" />
                    </span>
                  ) : (
                    <i.icon className="h-5 w-5" strokeWidth={isActive ? 2.4 : 1.8} />
                  )}
                  {t(i.label)}
                </>
              )}
            </NavLink>
          );
        })}
        <NavLink to="/more" className={cn("relative flex h-nav flex-1 flex-col items-center justify-center gap-0.5 text-[0.7rem] font-medium", moreActive ? "text-primary" : "text-muted-foreground")}>
          {moreActive && <span className="absolute top-0 h-0.5 w-8 rounded-full bg-primary" />}
          <Menu className="h-5 w-5" />
          {t("More")}
        </NavLink>
      </div>
    </nav>
  );
}

export function AppShell() {
  const { profile } = useSession();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const [searchOpen, setSearchOpen] = useState(false);
  const notes = useNotificationCounts();
  useLiveEvents(!!profile);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  // Block body: newer browsers return a Promise from scrollTo, which React would treat as a cleanup function.
  useEffect(() => {
    window.scrollTo(0, 0);
  }, [pathname]);

  const unread = notes.data?.unread ?? 0;
  const approvals = notes.data?.pending_approvals ?? 0;

  return (
    <div className="min-h-screen lg:grid lg:grid-cols-[var(--sidebar-w)_minmax(0,1fr)]">
      <Sidebar approvals={approvals} />
      <div className="min-w-0">
        <ActingBanner />
        <header className="sticky top-0 z-30 border-b bg-background/85 backdrop-blur no-print">
          <div className="mx-auto flex h-14 max-w-[1680px] items-center gap-2 px-4 md:px-6 lg:h-16 lg:px-8">
            <div className="min-w-0 flex-1 lg:hidden"><Brand /></div>
            <div className="flex shrink-0 items-center gap-1 lg:flex-1">
              <button
                onClick={() => setSearchOpen(true)}
                className="hidden h-10 w-full max-w-md items-center gap-2 rounded-lg border bg-card px-3 text-sm text-muted-foreground hover:border-primary/40 lg:flex"
              >
                <Search className="h-4 w-4" /> {t("Search products, customers, receipts, orders…")}
                <kbd className="ms-auto rounded border bg-muted px-1.5 text-[11px]">Ctrl K</kbd>
              </button>
              <Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setSearchOpen(true)} aria-label="Search"><Search /></Button>
            </div>
            <div className="min-w-0 shrink lg:hidden"><BranchSwitcher compact /></div>
            <PoweredBy className="me-1 hidden lg:inline-flex" />
            <Button variant="ghost" size="icon" className="relative" onClick={() => navigate("/notifications")} aria-label="Notifications">
              <Bell />
              {unread > 0 && <span className="num absolute end-1.5 top-1.5 min-w-4 rounded-full bg-primary px-1 text-[10px] font-semibold leading-4 text-primary-foreground animate-pop">{unread > 99 ? "99+" : unread}</span>}
            </Button>
            <ThemeToggle className="hidden sm:inline-flex" />
            <div className="hidden lg:block"><UserMenu /></div>
          </div>
        </header>
        <main className="mx-auto w-full max-w-[1680px] px-3.5 pb-[calc(var(--nav-h)+env(safe-area-inset-bottom,0px)+2rem)] pt-4 md:px-6 lg:px-8 lg:pb-12 lg:pt-6">
          <ErrorBoundary key={pathname}>
            <Suspense fallback={<Loading className="min-h-[50vh]" />}>
              <Outlet />
            </Suspense>
          </ErrorBoundary>
        </main>
      </div>
      <BottomNav />
      <GlobalSearch open={searchOpen} onOpenChange={setSearchOpen} />
    </div>
  );
}
