import { lazy, Suspense, useEffect, useState, type ComponentType, type ReactNode } from "react";
import { ArrowLeftRight } from "lucide-react";
import { BrowserRouter, Navigate, rememberNext, Route, Routes, SCREENS, useNavigate, useParams, useRawLocation } from "@/lib/router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Toaster } from "sonner";
import { AlertHost } from "@/lib/toast";
import { api, ApiError } from "@/lib/api";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import type { Profile } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { SessionProvider, useSession } from "@/lib/session";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppShell } from "@/components/layout/AppShell";
import { Loading } from "@/components/Page";
import LoginPage from "@/pages/auth/Login";
import SelectBranchPage from "@/pages/auth/SelectBranch";
import RequestAccessPage from "@/pages/auth/RequestAccess";
import { AccessStatusPage, ForcedPinChange, ForgotPage, SetPinPage } from "@/pages/auth/Recovery";

// Route-level code splitting keeps the first load small on mobile data.
// After a deploy, an open tab may ask for chunk files that no longer exist: reload once to pick up the new version.
function page<T extends ComponentType<object>>(load: () => Promise<{ default: T }>) {
  return lazy(() =>
    load().catch((err) => {
      const key = "sshop.chunk-reload";
      let last = 0;
      try {
        last = Number(sessionStorage.getItem(key) ?? 0);
        sessionStorage.setItem(key, String(Date.now()));
      } catch {
        /* storage unavailable */
      }
      if (Date.now() - last > 30_000) {
        window.location.reload();
        return new Promise<never>(() => {});
      }
      throw err;
    }),
  );
}

const Dashboard = page(() => import("@/pages/Dashboard"));
const Leaderboards = page(() => import("@/pages/Leaderboards"));
const MyDashboard = page(() => import("@/pages/Dashboard").then((m) => ({ default: m.MyDashboard })));
const More = page(() => import("@/pages/More"));
const Notifications = page(() => import("@/pages/Notifications"));
const Pos = page(() => import("@/pages/sales/Pos"));
const SalesList = page(() => import("@/pages/sales/SalesList"));
const SaleDetail = page(() => import("@/pages/sales/SaleDetail"));
const Exchange = page(() => import("@/pages/sales/Exchange"));
const CreditList = page(() => import("@/pages/credit/CreditList"));
const CreditDetail = page(() => import("@/pages/credit/CreditDetail"));
const OrdersList = page(() => import("@/pages/orders/OrdersList"));
const OrderDetail = page(() => import("@/pages/orders/OrderDetail"));
const ProductsList = page(() => import("@/pages/products/ProductsList"));
const ProductDetail = page(() => import("@/pages/products/ProductDetail"));
const ProductForm = page(() => import("@/pages/products/ProductForm"));
const Stock = page(() => import("@/pages/stock/Stock"));
const ReceiveStock = page(() => import("@/pages/stock/ReceiveStock"));
const StockCount = page(() => import("@/pages/stock/StockCount"));
const TransfersList = page(() => import("@/pages/transfers/TransfersList"));
const TransferNew = page(() => import("@/pages/transfers/TransferNew"));
const TransferDetail = page(() => import("@/pages/transfers/TransferDetail"));
const CustomersList = page(() => import("@/pages/customers/CustomersList"));
const CustomerDetail = page(() => import("@/pages/customers/CustomerDetail"));
const Loyalty = page(() => import("@/pages/loyalty/Loyalty"));
const Expenses = page(() => import("@/pages/expenses/Expenses"));
const Reports = page(() => import("@/pages/reports/Reports"));
const ReportView = page(() => import("@/pages/reports/ReportView"));
const Approvals = page(() => import("@/pages/Approvals"));
const Settings = page(() => import("@/pages/settings/Settings"));
const Audit = page(() => import("@/pages/Audit"));
const Portal = page(() => import("@/pages/portal/Portal"));
const Track = page(() => import("@/pages/portal/Track"));
const ReceiptView = page(() => import("@/pages/portal/ReceiptView"));

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 20_000,
      refetchOnWindowFocus: true,
      retry: (n, e) => !(e instanceof ApiError && e.status >= 400 && e.status < 500) && n < 2,
    },
  },
});

function RequireStaff({ children }: { children: ReactNode }) {
  const { profile, loading, needsBranch } = useSession();
  if (loading) return <Loading size="lg" className="min-h-screen" />;
  if (!profile) return <Navigate to="/login" replace />;
  // Signed in with a one-time PIN: replace it before anything else (the server enforces the same).
  if (profile.user.must_change_pin) return <ForcedPinChange />;
  if (needsBranch) return <Navigate to="/select-branch" replace />;
  return <>{children}</>;
}

/** Roadmap 145: `/` goes to the signed-in business's workspace (`/{slug}`), or to sign-in. */
function Root() {
  const { profile, loading } = useSession();
  if (loading) return <Loading size="lg" className="min-h-screen" />;
  return <Navigate to={profile ? "/" : "/login"} replace />;
}

/** Roadmap 145: the first segment of every workspace address is the business slug (`/pablo-niche/products`). The
 * session decides the business — never the address: a slug that isn't the current business is an old address of it
 * (redirected), an old unprefixed bookmark (`/products`, redirected), another business this person may switch to
 * (asked, never switched silently), or nothing of theirs (sent back to their own workspace). */
function WorkspaceGate() {
  const { profile, loading } = useSession();
  const biz = useParams().biz ?? "";
  const raw = useRawLocation();
  if (loading) return <Loading size="lg" className="min-h-screen" />;
  const here = `${raw.pathname}${raw.search}${raw.hash}`;
  if (!profile) {
    // Signed out (or the session ended): sign in — on the business's own sign-in page — then come back here.
    rememberNext(here);
    return <Navigate to={SCREENS.has(biz) ? "/login" : `/login/${encodeURIComponent(biz)}`} replace />;
  }
  const slug = profile.tenant.slug;
  if (biz === slug) return <RequireStaff><AppShell /></RequireStaff>;
  const rest = `${raw.pathname.slice(biz.length + 1)}${raw.search}${raw.hash}`;
  if (SCREENS.has(biz)) return <Navigate to={`/${slug}${raw.pathname}${raw.search}${raw.hash}`} replace />;
  if (profile.tenant.previous_slugs?.includes(biz)) return <Navigate to={`/${slug}${rest}`} replace />;
  const other = profile.businesses?.find((b) => !b.current && b.slug === biz);
  if (other) return <SwitchPrompt profile={profile} target={other} rest={rest} />;
  return <NotYours />;
}

function NotYours() {
  useEffect(() => {
    toast.error(t("That address isn't one of your businesses — you're back in your own workspace."));
  }, []);
  return <Navigate to="/" replace />;
}

/** A link to another business of this person's tenant: switch only when they choose to. */
function SwitchPrompt({ profile, target, rest }: { profile: Profile; target: { id: string; name: string; slug?: string }; rest: string }) {
  const { switchBusiness } = useSession();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const go = async () => {
    setBusy(true);
    try {
      const r = await api<{ token: string; profile: Profile }>("/auth/switch-business", { body: { tenant_id: target.id } });
      switchBusiness(r.token, r.profile);
      const to = `/${r.profile.tenant.slug}${rest}`;
      toast.success(`${t("Now in")} ${r.profile.tenant.name}`);
      if (r.profile.branches.length > 1 && !r.profile.user.default_branch_id) {
        rememberNext(to);
        navigate("/select-branch", { replace: true });
      } else navigate(to, { replace: true });
    } catch (e) {
      toast.error(e);
      setBusy(false);
    }
  };
  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-4">
      <div className="w-full max-w-sm space-y-4 rounded-2xl border bg-card p-6 text-center shadow-sm">
        <ArrowLeftRight className="mx-auto h-8 w-8 text-primary" />
        <h1 className="text-lg font-semibold">{t("Switch business?")}</h1>
        <p className="text-sm text-muted-foreground">
          {t("This link is for")} <b className="text-foreground">{target.name}</b>. {t("You're working in")} <b className="text-foreground">{profile.tenant.name}</b>.
        </p>
        <div className="flex flex-col gap-2">
          <Button onClick={go} disabled={busy}>{t("Switch to")} {target.name}</Button>
          <Button variant="outline" onClick={() => navigate("/", { replace: true })} disabled={busy}>{t("Stay in")} {profile.tenant.name}</Button>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={300}>
        <SessionProvider>
          <BrowserRouter>
            <Suspense fallback={<Loading size="lg" className="min-h-[60vh]" />}>
              <Routes>
                <Route path="/login" element={<LoginPage />} />
                <Route path="/login/:business" element={<LoginPage />} />
                <Route path="/select-branch" element={<SelectBranchPage />} />
                <Route path="/request-access" element={<RequestAccessPage />} />
                <Route path="/forgot" element={<ForgotPage />} />
                <Route path="/set-pin" element={<SetPinPage />} />
                <Route path="/access-status" element={<AccessStatusPage />} />
                <Route path="/order/:slug/*" element={<Portal />} />
                <Route path="/track/:token" element={<Track />} />
                <Route path="/r/:token" element={<ReceiptView />} />
                <Route path="/" element={<Root />} />
                <Route path="/:biz" element={<WorkspaceGate />}>
                  <Route index element={<Dashboard />} />
                  <Route path="my" element={<MyDashboard />} />
                  <Route path="leaderboards" element={<Leaderboards />} />
                  <Route path="more" element={<More />} />
                  <Route path="notifications" element={<Notifications />} />
                  <Route path="pos" element={<Pos />} />
                  <Route path="sales" element={<SalesList />} />
                  <Route path="sales/:id" element={<SaleDetail />} />
                  <Route path="sales/:id/exchange" element={<Exchange />} />
                  <Route path="credit" element={<CreditList />} />
                  <Route path="credit/:id" element={<CreditDetail />} />
                  <Route path="orders" element={<OrdersList />} />
                  <Route path="orders/:id" element={<OrderDetail />} />
                  <Route path="products" element={<ProductsList />} />
                  <Route path="products/new" element={<ProductForm />} />
                  <Route path="products/:id" element={<ProductDetail />} />
                  <Route path="products/:id/edit" element={<ProductForm />} />
                  <Route path="stock" element={<Stock />} />
                  <Route path="stock/receive" element={<ReceiveStock />} />
                  <Route path="stock/count" element={<StockCount />} />
                  <Route path="transfers" element={<TransfersList />} />
                  <Route path="transfers/new" element={<TransferNew />} />
                  <Route path="transfers/:id" element={<TransferDetail />} />
                  <Route path="customers" element={<CustomersList />} />
                  <Route path="customers/:id" element={<CustomerDetail />} />
                  <Route path="loyalty" element={<Loyalty />} />
                  <Route path="expenses" element={<Expenses />} />
                  <Route path="reports" element={<Reports />} />
                  <Route path="reports/:key" element={<ReportView />} />
                  <Route path="approvals" element={<Approvals />} />
                  <Route path="settings/*" element={<Settings />} />
                  <Route path="audit" element={<Audit />} />
                  <Route path="*" element={<Navigate to="/" replace />} />
                </Route>
              </Routes>
            </Suspense>
          </BrowserRouter>
        </SessionProvider>
      </TooltipProvider>
      <Toaster position="top-center" richColors closeButton duration={5000} toastOptions={{ className: "font-sans" }} />
      <AlertHost />
    </QueryClientProvider>
  );
}
