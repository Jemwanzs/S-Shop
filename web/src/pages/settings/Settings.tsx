import { NavLink, Navigate, Route, Routes, useLocation } from "react-router-dom";
import {
  Award,
  Building2,
  SlidersHorizontal,
  Inbox,
  Network,
  ChevronRight,
  ClipboardList,
  CreditCard,
  FileBarChart,
  KeyRound,
  Package,
  Plug,
  ShieldCheck,
  Store,
  Users,
  UserSquare,
  Wallet,
  Warehouse,
  type LucideIcon,
} from "lucide-react";
import { useSession } from "@/lib/session";
import { useMediaQuery } from "@/lib/hooks";
import { cn } from "@/lib/utils";
import { PageHeader } from "@/components/Page";
import { BusinessSettings, IntegrationsSettings } from "./General";
import { BranchesSettings, RolesSettings, UsersSettings } from "./People";
import { CustomerSettings, ExpenseSettings, LoyaltySettings, OrderSettings, ProductSettings, ReportSettings, SalesSettings, StockSettings } from "./Config";
import { WorkflowSettings } from "./Workflows";
import { AccessRequests } from "./AccessRequests";
import { PreferencesSettings } from "./Preferences";
import { BusinessesSettings } from "./Businesses";
import { t } from "@/lib/i18n";

interface SectionDef {
  path: string;
  label: string;
  group: string;
  icon: LucideIcon;
  perm: string;
  element: JSX.Element;
}

const SECTIONS: SectionDef[] = [
  { path: "business", label: "Business profile", group: "Business", icon: Store, perm: "settings.manage", element: <BusinessSettings /> },
  { path: "branches", label: "Branches", group: "Business", icon: Building2, perm: "branches.manage", element: <BranchesSettings /> },
  { path: "users", label: "Users", group: "Business", icon: Users, perm: "users.manage", element: <UsersSettings /> },
  { path: "roles", label: "Roles & permissions", group: "Business", icon: KeyRound, perm: "roles.manage", element: <RolesSettings /> },
  { path: "products", label: "Products", group: "Configuration", icon: Package, perm: "settings.manage", element: <ProductSettings /> },
  { path: "sales", label: "Sales & payments", group: "Configuration", icon: CreditCard, perm: "settings.manage", element: <SalesSettings /> },
  { path: "stock", label: "Stock", group: "Configuration", icon: Warehouse, perm: "settings.manage", element: <StockSettings /> },
  { path: "orders", label: "Orders & ordering link", group: "Configuration", icon: ClipboardList, perm: "settings.manage", element: <OrderSettings /> },
  { path: "customers", label: "Customers", group: "Configuration", icon: UserSquare, perm: "settings.manage", element: <CustomerSettings /> },
  { path: "loyalty", label: "Loyalty & rewards", group: "Configuration", icon: Award, perm: "settings.manage", element: <LoyaltySettings /> },
  { path: "expenses", label: "Expenses", group: "Configuration", icon: Wallet, perm: "settings.manage", element: <ExpenseSettings /> },
  { path: "reports", label: "Reports", group: "Configuration", icon: FileBarChart, perm: "settings.manage", element: <ReportSettings /> },
  { path: "workflows", label: "Workflow engine", group: "Control", icon: ShieldCheck, perm: "settings.manage", element: <WorkflowSettings /> },
  { path: "integrations", label: "M-Pesa & WhatsApp", group: "Control", icon: Plug, perm: "settings.manage", element: <IntegrationsSettings /> },
  // Everyone: their own preferences.
  { path: "preferences", label: "User preferences", group: "Personal", icon: SlidersHorizontal, perm: "", element: <PreferencesSettings /> },
  // "platform": only platform administrators (PLATFORM_ADMIN_EMAILS) see this section.
  { path: "businesses", label: "Businesses", group: "Platform", icon: Network, perm: "platform", element: <BusinessesSettings /> },
  { path: "access-requests", label: "Access requests", group: "Platform", icon: Inbox, perm: "platform", element: <AccessRequests /> },
];

export default function Settings() {
  const { can, profile } = useSession();
  const { pathname } = useLocation();
  const sections = SECTIONS.filter((s) => (s.perm === "platform" ? !!profile?.user.platform_admin : !s.perm || can(s.perm)));
  const groups = [...new Set(sections.map((s) => s.group))];
  const atIndex = /\/settings\/?$/.test(pathname);

  const nav = (
    <nav className="space-y-5">
      {groups.map((g) => (
        <div key={g}>
          <p className="label-caps mb-1.5 px-3">{t(g)}</p>
          <div className="surface overflow-hidden lg:border-0 lg:bg-transparent lg:shadow-none">
            {sections.filter((s) => s.group === g).map((s) => (
              <NavLink
                key={s.path}
                to={`/settings/${s.path}`}
                className={({ isActive }) =>
                  cn(
                    "flex items-center gap-3 border-b px-3 py-3 text-sm last:border-0 lg:rounded-lg lg:border-0 lg:py-2",
                    isActive ? "bg-primary/10 font-medium text-primary" : "hover:bg-accent/50",
                  )
                }
              >
                <s.icon className="h-4 w-4 shrink-0" />
                <span className="flex-1">{t(s.label)}</span>
                <ChevronRight className="h-4 w-4 text-muted-foreground lg:hidden rtl:rotate-180" />
              </NavLink>
            ))}
          </div>
        </div>
      ))}
    </nav>
  );

  return (
    <>
      <PageHeader eyebrow="Admin" title="Settings" back={atIndex ? undefined : "/settings"} />
      <div className="lg:grid lg:grid-cols-[240px_minmax(0,1fr)] lg:gap-8">
        <aside className={cn("lg:block", !atIndex && "hidden")}>
          <div className="lg:sticky lg:top-24">{nav}</div>
        </aside>
        <div className={cn("min-w-0", atIndex && "hidden lg:block")}>
          <Routes>
            {sections.map((s) => <Route key={s.path} path={s.path} element={s.element} />)}
            <Route index element={<IndexRoute first={sections[0]?.path} />} />
            <Route path="*" element={<Navigate to="/settings" replace />} />
          </Routes>
        </div>
      </div>
    </>
  );
}

/** Desktop opens the first section; phones show the section menu at /settings. */
function IndexRoute({ first }: { first?: string }) {
  const desktop = useMediaQuery("(min-width: 1024px)");
  return desktop && first ? <Navigate to={`/settings/${first}`} replace /> : null;
}
