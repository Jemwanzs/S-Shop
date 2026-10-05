import { lazy, Suspense, type ComponentType, type ReactNode } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Toaster } from "sonner";
import { AlertHost } from "@/lib/toast";
import { ApiError } from "@/lib/api";
import { SessionProvider, useSession } from "@/lib/session";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppShell } from "@/components/layout/AppShell";
import { Loading } from "@/components/Page";
import LoginPage from "@/pages/auth/Login";
import SelectBranchPage from "@/pages/auth/SelectBranch";
import RequestAccessPage from "@/pages/auth/RequestAccess";

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
const More = page(() => import("@/pages/More"));
const Notifications = page(() => import("@/pages/Notifications"));
const Pos = page(() => import("@/pages/sales/Pos"));
const SalesList = page(() => import("@/pages/sales/SalesList"));
const SaleDetail = page(() => import("@/pages/sales/SaleDetail"));
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
  if (loading) return <Loading className="min-h-screen" />;
  if (!profile) return <Navigate to="/login" replace />;
  if (needsBranch) return <Navigate to="/select-branch" replace />;
  return <>{children}</>;
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={300}>
        <SessionProvider>
          <BrowserRouter>
            <Suspense fallback={<Loading className="min-h-[60vh]" />}>
              <Routes>
                <Route path="/login" element={<LoginPage />} />
                <Route path="/select-branch" element={<SelectBranchPage />} />
                <Route path="/request-access" element={<RequestAccessPage />} />
                <Route path="/order/:slug/*" element={<Portal />} />
                <Route path="/track/:token" element={<Track />} />
                <Route element={<RequireStaff><AppShell /></RequireStaff>}>
                  <Route index element={<Dashboard />} />
                  <Route path="more" element={<More />} />
                  <Route path="notifications" element={<Notifications />} />
                  <Route path="pos" element={<Pos />} />
                  <Route path="sales" element={<SalesList />} />
                  <Route path="sales/:id" element={<SaleDetail />} />
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
