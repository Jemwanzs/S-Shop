import {
  ArrowLeftRight,
  Award,
  BarChart3,
  Boxes,
  ClipboardList,
  HandCoins,
  History,
  LayoutGrid,
  Package,
  Receipt,
  Settings,
  ShieldCheck,
  ShoppingCart,
  Trophy,
  UserRound,
  Users,
  Wallet,
  type LucideIcon,
} from "lucide-react";

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  /** Shown when the user holds any of these permissions (empty = everyone). */
  perms: string[];
}

export const NAV: { group: string; items: NavItem[] }[] = [
  {
    group: "Overview",
    items: [
      { to: "/", label: "Dashboard", icon: LayoutGrid, perms: [] },
      { to: "/my", label: "My Dashboard", icon: UserRound, perms: ["sales.create", "sales.view", "orders.manage"] },
      { to: "/leaderboards", label: "Leaderboards", icon: Trophy, perms: ["dashboard.view", "reports.view"] },
    ],
  },
  {
    group: "Sell",
    items: [
      { to: "/pos", label: "New Sale", icon: ShoppingCart, perms: ["sales.create"] },
      { to: "/sales", label: "Sales", icon: Receipt, perms: ["sales.view"] },
      { to: "/credit", label: "Credit Sales", icon: HandCoins, perms: ["credit.view"] },
      { to: "/orders", label: "Orders", icon: ClipboardList, perms: ["orders.view"] },
    ],
  },
  {
    group: "Inventory",
    items: [
      { to: "/products", label: "Products", icon: Package, perms: ["products.view"] },
      { to: "/stock", label: "Stock", icon: Boxes, perms: ["stock.view"] },
      { to: "/transfers", label: "Transfers", icon: ArrowLeftRight, perms: ["stock.view"] },
    ],
  },
  {
    group: "Customers",
    items: [
      { to: "/customers", label: "Customers", icon: Users, perms: ["customers.view"] },
      { to: "/loyalty", label: "Loyalty & Rewards", icon: Award, perms: ["customers.view_loyalty"] },
    ],
  },
  {
    group: "Finance",
    items: [
      { to: "/expenses", label: "Expenses", icon: Wallet, perms: ["expenses.view"] },
      { to: "/reports", label: "Reports", icon: BarChart3, perms: ["reports.view"] },
    ],
  },
  {
    group: "Admin",
    items: [
      { to: "/approvals", label: "Approvals", icon: ShieldCheck, perms: [] },
      { to: "/settings", label: "Settings", icon: Settings, perms: [] },
      { to: "/audit", label: "Audit Trail", icon: History, perms: ["audit.view"] },
    ],
  },
];

/** Bottom navigation on phones & tablets: Home | Sales | Stock | Orders | More */
export const BOTTOM: NavItem[] = [
  { to: "/", label: "Home", icon: LayoutGrid, perms: [] },
  { to: "/pos", label: "Sales", icon: ShoppingCart, perms: ["sales.create"] },
  { to: "/stock", label: "Stock", icon: Boxes, perms: ["stock.view"] },
  { to: "/orders", label: "Orders", icon: ClipboardList, perms: ["orders.view"] },
];

export const allowed = (item: NavItem, can: (p: string) => boolean) => item.perms.length === 0 || item.perms.some(can);
