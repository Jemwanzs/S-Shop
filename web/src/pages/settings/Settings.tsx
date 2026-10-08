import { NavLink, Navigate, Route, Routes, useLocation } from "react-router-dom";
import {
  Activity,
  Award,
  Banknote,
  Building2,
  CalendarClock,
  SlidersHorizontal,
  Inbox,
  LifeBuoy,
  Users2,
  Network,
  ChevronRight,
  ClipboardList,
  CreditCard,
  FileBarChart,
  Globe,
  KeyRound,
  Package,
  Plug,
  Receipt,
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
import { CustomerSettings, ExpenseSettings, LoyaltySettings, OrderSettings, ProductSettings, ReportSettings, SalesSettings, StockSettings, WorkspaceSettings } from "./Config";
import { WorkflowSettings } from "./Workflows";
import { AccessRequests } from "./AccessRequests";
import { PreferencesSettings } from "./Preferences";
import { BusinessesSettings } from "./Businesses";
import { BusinessDetail } from "./BusinessDetail";
import { TenantDetail, TenantsSettings } from "./Tenants";
import { SupportAccessSettings } from "./SupportAccess";
import { BillingSettings } from "./Billing";
import { PlatformActivity } from "./PlatformActivity";
import { PlatformBilling } from "./PlatformBilling";
import { WebsiteSettings } from "./website/Website";
import { t } from "@/lib/i18n";

const WEBSITE_PERMS = ["website.view", "website.content", "website.products", "website.photos", "website.categories", "website.media", "website.services",
  "website.testimonials", "website.design", "website.navigation", "website.seo", "website.domain", "website.preview", "website.publish", "website.analytics"];

interface SectionDef {
  path: string;
  label: string;
  group: string;
  icon: LucideIcon;
  perm: string;
  element: JSX.Element;
}

const SECTIONS: SectionDef[] = [
  { path: "business", label: "Business profile", group: "Business", icon: Store, perm: "settings.business", element: <BusinessSettings /> },
  { path: "branches", label: "Branches", group: "Business", icon: Building2, perm: "branches.manage", element: <BranchesSettings /> },
  { path: "users", label: "Users", group: "Business", icon: Users, perm: "users.manage", element: <UsersSettings /> },
  { path: "workspace", label: "Workspace & hours", group: "Business", icon: CalendarClock, perm: "settings.workspace", element: <WorkspaceSettings /> },
  { path: "roles", label: "Roles & permissions", group: "Business", icon: KeyRound, perm: "roles.manage", element: <RolesSettings /> },
  { path: "billing", label: "Billing", group: "Business", icon: Receipt, perm: "settings.billing", element: <BillingSettings /> },
  { path: "products", label: "Products", group: "Configuration", icon: Package, perm: "settings.products", element: <ProductSettings /> },
  { path: "sales", label: "Sales & payments", group: "Configuration", icon: CreditCard, perm: "settings.sales", element: <SalesSettings /> },
  { path: "stock", label: "Stock", group: "Configuration", icon: Warehouse, perm: "settings.stock", element: <StockSettings /> },
  { path: "orders", label: "Orders & ordering link", group: "Configuration", icon: ClipboardList, perm: "settings.orders", element: <OrderSettings /> },
  { path: "customers", label: "Customers", group: "Configuration", icon: UserSquare, perm: "settings.customers", element: <CustomerSettings /> },
  { path: "loyalty", label: "Loyalty & rewards", group: "Configuration", icon: Award, perm: "settings.customers", element: <LoyaltySettings /> },
  { path: "expenses", label: "Expenses", group: "Configuration", icon: Wallet, perm: "settings.expenses", element: <ExpenseSettings /> },
  { path: "reports", label: "Reports", group: "Configuration", icon: FileBarChart, perm: "settings.reports", element: <ReportSettings /> },
  { path: "workflows", label: "Workflow engine", group: "Control", icon: ShieldCheck, perm: "settings.workflows", element: <WorkflowSettings /> },
  { path: "integrations", label: "M-Pesa & WhatsApp", group: "Control", icon: Plug, perm: "settings.integrations", element: <IntegrationsSettings /> },
  // Website Add-On: integrations admins (request / overview) and anyone given a website permission.
  { path: "website", label: "Website", group: "Control", icon: Globe, perm: "website", element: <WebsiteSettings /> },
  // Everyone: their own preferences.
  // Tenant administrators: platform support access to their business (roadmap 71).
  { path: "support", label: "Support access", group: "Control", icon: LifeBuoy, perm: "users.manage", element: <SupportAccessSettings /> },
  { path: "preferences", label: "User preferences", group: "Personal", icon: SlidersHorizontal, perm: "", element: <PreferencesSettings /> },
  // "platform": only platform administrators (PLATFORM_ADMIN_EMAILS) see this section.
  { path: "tenants", label: "Tenants", group: "Platform", icon: Users2, perm: "platform", element: <TenantsSettings /> },
  { path: "businesses", label: "Businesses", group: "Platform", icon: Network, perm: "platform", element: <BusinessesSettings /> },
  { path: "platform-billing", label: "Platform billing", group: "Platform", icon: Banknote, perm: "platform", element: <PlatformBilling /> },
  { path: "activity", label: "Activity", group: "Platform", icon: Activity, perm: "platform", element: <PlatformActivity /> },
  { path: "access-requests", label: "Access requests", group: "Platform", icon: Inbox, perm: "platform", element: <AccessRequests /> },
];

export default function Settings() {
  const { can, profile } = useSession();
  const { pathname } = useLocation();
  const sections = SECTIONS.filter((s) =>
    s.perm === "platform" ? !!profile?.user.platform_admin
      : s.perm === "website" ? can("settings.integrations") || WEBSITE_PERMS.some((p) => can(p))
        : !s.perm || can(s.perm));
  const groups = [...new Set(sections.map((s) => s.group))];
  const atIndex = /\/settings\/?$/.test(pathname);

  const nav = (
    <nav className="space-y-4">
      {groups.map((g) => (
        <div key={g}>
          <p className="label-caps mb-1 px-3">{t(g)}</p>
          <div className="surface overflow-hidden lg:border-0 lg:bg-transparent lg:shadow-none">
            {sections.filter((s) => s.group === g).map((s) => (
              <NavLink
                key={s.path}
                to={`/settings/${s.path}`}
                className={({ isActive }) =>
                  cn(
                    "flex items-center gap-2.5 border-b px-3 py-2.5 text-sm last:border-0 lg:h-8 lg:rounded-md lg:border-0 lg:py-0",
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
            {profile?.user.platform_admin && <Route path="businesses/:id" element={<BusinessDetail />} />}
            {profile?.user.platform_admin && <Route path="tenants/:id" element={<TenantDetail />} />}
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
