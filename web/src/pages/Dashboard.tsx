import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Area, AreaChart, CartesianGrid, Cell, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import {
  AlertTriangle,
  Boxes,
  ClipboardList,
  Coins,
  CreditCard,
  Gift,
  HandCoins,
  Package,
  Receipt,
  ShoppingBag,
  ShoppingCart,
  TrendingUp,
  UserPlus,
  Users,
  Wallet,
} from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { compact, count, methodLabel, money, maskPhone, titleCase, toNum } from "@/lib/format";
import type { Category, Money, UserRow } from "@/lib/types";
import { PageHeader, Section, ErrorState, Loading, EmptyState } from "@/components/Page";
import { StatCard } from "@/components/Stat";
import { PeriodFilter, type PeriodValue } from "@/components/Filters";
import { NativeSelect } from "@/components/Form";
import { Medal, PointsPill, Pill } from "@/components/Badges";

interface DashboardData {
  from: string;
  to: string;
  bucket: string;
  kpis: Record<string, Money | number | null>;
  series: { date: string; sales: Money; transactions: number; profit: Money | null }[];
  payment_mix: { method: string; amount: Money; count: number }[];
  top_products_revenue: { product_id: string; name: string; units: number; revenue: Money; medal?: string | null }[];
  top_products_units: { product_id: string; name: string; units: number; revenue: Money; medal?: string | null }[];
  slow_movers: { product_id: string; name: string; units: number; on_hand: number }[];
  low_stock: { product_id: string; name: string; branch_name: string; available: number; threshold: number }[];
  top_customers: { customer_id: string; name: string; mobile: string; spend: Money; own_points: number; referral_points: number; tier: string; medal?: string }[];
  by_branch: { branch_id: string; name: string; sales: Money; transactions: number }[];
  by_user: { user_id: string; name: string; sales: Money; transactions: number; units: number; medal?: string | null }[];
}

const CHART_COLORS = ["hsl(var(--chart-1))", "hsl(var(--chart-2))", "hsl(var(--chart-3))", "hsl(var(--chart-4))", "hsl(var(--chart-5))"];

export default function Dashboard() {
  const { can } = useSession();
  return can("dashboard.view") ? <Analytics /> : <QuickHome />;
}

function QuickHome() {
  const { profile, branch, can } = useSession();
  const tiles = [
    { to: "/pos", label: "New sale", icon: ShoppingCart, perm: "sales.create", primary: true },
    { to: "/orders", label: "Orders", icon: ClipboardList, perm: "orders.view" },
    { to: "/stock", label: "Stock", icon: Boxes, perm: "stock.view" },
    { to: "/customers", label: "Customers", icon: Users, perm: "customers.view" },
    { to: "/credit", label: "Credit", icon: HandCoins, perm: "credit.view" },
    { to: "/sales", label: "My sales", icon: Receipt, perm: "sales.view" },
  ].filter((t) => can(t.perm));
  return (
    <>
      <PageHeader eyebrow={branch?.name} title={`Hello, ${profile?.user.name.split(" ")[0]} 👋`} description="What would you like to do?" />
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-6">
        {tiles.map((t) => (
          <Link key={t.to} to={t.to} className={`surface flex aspect-[4/3] flex-col justify-between p-4 transition hover:shadow-lift ${t.primary ? "border-primary bg-primary text-primary-foreground" : ""}`}>
            <t.icon className="h-6 w-6" />
            <span className="font-semibold">{t.label}</span>
          </Link>
        ))}
      </div>
    </>
  );
}

function Analytics() {
  const { profile, currency, can } = useSession();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [period, setPeriod] = useState<PeriodValue>({ period: "week" });
  const [branchId, setBranchId] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [userId, setUserId] = useState("");
  const productId = params.get("product") ?? "";

  const query = { ...period, branch_id: branchId, category_id: categoryId, user_id: userId, product_id: productId };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["dashboard", query],
    queryFn: () => api<DashboardData>("/dashboard", { query }),
    placeholderData: (prev) => prev,
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api<UserRow[]>("/users"), enabled: can("users.manage") || can("approvals.approve") });

  const k = data?.kpis ?? {};
  const m = (v: unknown) => money(v as Money, currency);
  const multiBranch = (profile?.branches.length ?? 0) > 1;

  return (
    <>
      <PageHeader
        eyebrow="Overview"
        title="Dashboard"
        description={data ? `${data.from === data.to ? data.from : `${data.from} → ${data.to}`}` : undefined}
      />

      <div className="mb-5 space-y-3">
        <PeriodFilter value={period} onChange={setPeriod} />
        <div className="grid grid-cols-2 gap-2 sm:flex sm:flex-wrap">
          {multiBranch && (
            <NativeSelect value={branchId} onChange={setBranchId} className="sm:w-48">
              <option value="">All my branches</option>
              {profile?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
            </NativeSelect>
          )}
          <NativeSelect value={categoryId} onChange={setCategoryId} className="sm:w-48">
            <option value="">All categories</option>
            {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </NativeSelect>
          {users.data && (
            <NativeSelect value={userId} onChange={setUserId} className="sm:w-48">
              <option value="">All staff</option>
              {users.data.map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
            </NativeSelect>
          )}
          {productId && <Pill tone="primary" className="h-11 px-3">Filtered by product · <Link to="/" className="underline">clear</Link></Pill>}
        </div>
      </div>

      {error ? (
        <ErrorState error={error} retry={refetch} />
      ) : isLoading || !data ? (
        <Loading />
      ) : (
        <div className="space-y-5">
          <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-6">
            <StatCard label="Sales" value={m(k.sales)} icon={TrendingUp} tone="success" change={k.sales_change_pct as number | null} hint="vs previous period" className="col-span-2 md:col-span-1" />
            <StatCard label="Transactions" value={count(k.transactions as number)} icon={ShoppingBag} change={k.transactions_change_pct as number | null} />
            <StatCard label="Avg. transaction" value={m(k.average_transaction)} icon={CreditCard} />
            <StatCard label="Units sold" value={count(k.units_sold as number)} icon={Package} />
            <StatCard label="Orders" value={count(k.orders as number)} icon={ClipboardList} hint={`${count(k.open_orders as number)} open`} tone="primary" />
            {k.gross_profit !== null && <StatCard label="Gross profit" value={m(k.gross_profit)} icon={Coins} tone="success" hint={k.profit_coverage_pct !== null ? `${k.profit_coverage_pct}% of sales costed` : undefined} />}
            <StatCard label="Expenses" value={m(k.expenses)} icon={Wallet} tone="warning" />
            {k.net_performance !== null && <StatCard label="Net performance" value={m(k.net_performance)} icon={TrendingUp} tone={toNum(k.net_performance as Money) >= 0 ? "success" : "danger"} />}
            {k.stock_value !== null && <StatCard label="Stock value" value={m(k.stock_value)} icon={Boxes} hint={`${count(k.stock_units as number)} units`} />}
            <StatCard label="Customers" value={count(k.customers as number)} icon={Users} hint={`${count(k.new_customers as number)} new`} />
            {k.credit_outstanding !== null && <StatCard label="Credit outstanding" value={m(k.credit_outstanding)} icon={HandCoins} tone="danger" />}
            <StatCard label="Points issued" value={count(k.points_issued as number)} icon={Gift} tone="primary" hint={`${count(k.points_redeemed as number)} redeemed`} />
          </div>

          <div className="grid gap-5 xl:grid-cols-3">
            <Section title="Sales trend" className="xl:col-span-2">
              {data.series.length === 0 ? (
                <EmptyState icon={TrendingUp} title="No sales in this period" />
              ) : (
                <div className="h-64 lg:h-80">
                  <ResponsiveContainer>
                    <AreaChart data={data.series.map((s) => ({ ...s, sales: toNum(s.sales), profit: s.profit === null ? undefined : toNum(s.profit) }))} margin={{ left: 0, right: 8, top: 8 }}>
                      <defs>
                        <linearGradient id="salesFill" x1="0" y1="0" x2="0" y2="1">
                          <stop offset="0%" stopColor="hsl(var(--chart-1))" stopOpacity={0.35} />
                          <stop offset="100%" stopColor="hsl(var(--chart-1))" stopOpacity={0} />
                        </linearGradient>
                      </defs>
                      <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" vertical={false} />
                      <XAxis dataKey="date" tickFormatter={(d) => new Date(d).toLocaleDateString("en-GB", { day: "2-digit", month: "short" })} tick={{ fontSize: 11, fill: "hsl(var(--muted-foreground))" }} axisLine={false} tickLine={false} minTickGap={16} />
                      <YAxis tickFormatter={(v) => compact(v)} tick={{ fontSize: 11, fill: "hsl(var(--muted-foreground))" }} axisLine={false} tickLine={false} width={44} />
                      <Tooltip
                        contentStyle={{ background: "hsl(var(--popover))", border: "1px solid hsl(var(--border))", borderRadius: 12, fontSize: 12 }}
                        formatter={(v: number, name) => [money(v, currency), titleCase(String(name))]}
                        labelFormatter={(d) => new Date(d).toDateString()}
                      />
                      <Area type="monotone" dataKey="sales" stroke="hsl(var(--chart-1))" strokeWidth={2.5} fill="url(#salesFill)" />
                      {k.gross_profit !== null && <Area type="monotone" dataKey="profit" stroke="hsl(var(--chart-2))" strokeWidth={2} fill="transparent" />}
                    </AreaChart>
                  </ResponsiveContainer>
                </div>
              )}
            </Section>
            <Section title="Payment mix">
              {data.payment_mix.length === 0 ? (
                <EmptyState icon={CreditCard} title="No payments yet" />
              ) : (
                <div className="flex flex-col items-center gap-4 sm:flex-row xl:flex-col">
                  <div className="h-44 w-44 shrink-0">
                    <ResponsiveContainer>
                      <PieChart>
                        <Pie data={data.payment_mix.map((p) => ({ name: p.method, value: toNum(p.amount) }))} dataKey="value" innerRadius={52} outerRadius={80} paddingAngle={2} stroke="none">
                          {data.payment_mix.map((p, i) => <Cell key={p.method} fill={CHART_COLORS[i % CHART_COLORS.length]} />)}
                        </Pie>
                      </PieChart>
                    </ResponsiveContainer>
                  </div>
                  <ul className="w-full space-y-2">
                    {data.payment_mix.map((p, i) => (
                      <li key={p.method} className="flex items-center gap-2 text-sm">
                        <span className="h-2.5 w-2.5 rounded-full" style={{ background: CHART_COLORS[i % CHART_COLORS.length] }} />
                        <span className="flex-1">{methodLabel(p.method)} <span className="text-muted-foreground">· {p.count}</span></span>
                        <span className="num font-medium">{m(p.amount)}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </Section>
          </div>

          <div className="grid gap-5 md:grid-cols-2 2xl:grid-cols-4">
            <RankList title="Best sellers · revenue" rows={data.top_products_revenue.map((p) => ({ id: p.product_id, name: p.name, value: m(p.revenue), sub: `${count(p.units)} units`, medal: p.medal }))} onClick={(id) => navigate(`/products/${id}`)} />
            <RankList title="Best sellers · quantity" rows={data.top_products_units.map((p) => ({ id: p.product_id, name: p.name, value: `${count(p.units)} units`, sub: m(p.revenue), medal: p.medal }))} onClick={(id) => navigate(`/products/${id}`)} />
            <RankList title="Slow movers" rows={data.slow_movers.map((p) => ({ id: p.product_id, name: p.name, value: `${count(p.units)} sold`, sub: `${count(p.on_hand)} in stock` }))} onClick={(id) => navigate(`/products/${id}`)} plain />
            <Section title="Low stock" action={<Link to="/stock?status=low" className="text-xs text-primary">View all</Link>}>
              {data.low_stock.length === 0 ? (
                <p className="py-6 text-center text-sm text-muted-foreground">All stocked up ✨</p>
              ) : (
                <ul className="divide-y">
                  {data.low_stock.map((p) => (
                    <li key={p.product_id + p.branch_name} className="flex items-center gap-2 py-2.5 text-sm">
                      <AlertTriangle className={`h-4 w-4 shrink-0 ${p.available <= 0 ? "text-destructive" : "text-warning"}`} />
                      <span className="min-w-0 flex-1 truncate">{p.name}{multiBranch && <span className="text-muted-foreground"> · {p.branch_name}</span>}</span>
                      <span className="num font-semibold">{count(p.available)}</span>
                    </li>
                  ))}
                </ul>
              )}
            </Section>
          </div>

          <div className="grid gap-5 lg:grid-cols-2 2xl:grid-cols-3">
            <Section title="Top customers" action={<Link to="/loyalty" className="text-xs text-primary">Loyalty</Link>}>
              {data.top_customers.length === 0 ? (
                <p className="py-6 text-center text-sm text-muted-foreground">No identified customers yet</p>
              ) : (
                <ul className="divide-y">
                  {data.top_customers.map((c, i) => (
                    <li key={c.customer_id}>
                      <Link to={`/customers/${c.customer_id}`} className="flex items-center gap-3 py-3">
                        <span className="num w-5 text-center text-sm text-muted-foreground">{i + 1}</span>
                        <span className="min-w-0 flex-1">
                          <span className="flex items-center gap-1.5 truncate font-medium">{c.name} <Medal tier={c.medal} /></span>
                          <span className="num block text-xs text-muted-foreground">{maskPhone(c.mobile)}</span>
                        </span>
                        <span className="text-right">
                          <span className="num block font-semibold">{m(c.spend)}</span>
                          <PointsPill own={c.own_points} referral={c.referral_points} />
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              )}
            </Section>
            <Section title="Staff performance">
              {data.by_user.length === 0 ? (
                <p className="py-6 text-center text-sm text-muted-foreground">No sales yet</p>
              ) : (
                <ul className="divide-y">
                  {data.by_user.map((u) => (
                    <li key={u.user_id} className="flex items-center gap-3 py-3">
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-1.5 truncate font-medium">{u.name} <Medal tier={u.medal} /></span>
                        <span className="text-xs text-muted-foreground">{count(u.transactions)} sales · {count(u.units)} units</span>
                      </span>
                      <span className="num font-semibold">{m(u.sales)}</span>
                    </li>
                  ))}
                </ul>
              )}
            </Section>
            {data.by_branch.length > 1 && (
              <Section title="Branches">
                <ul className="space-y-3">
                  {(() => {
                    const max = Math.max(...data.by_branch.map((b) => toNum(b.sales)), 1);
                    return data.by_branch.map((b) => (
                      <li key={b.branch_id} className="space-y-1">
                        <div className="flex justify-between text-sm">
                          <span className="truncate font-medium">{b.name}</span>
                          <span className="num">{m(b.sales)}</span>
                        </div>
                        <div className="h-2 overflow-hidden rounded-full bg-muted">
                          <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${(toNum(b.sales) / max) * 100}%` }} />
                        </div>
                      </li>
                    ));
                  })()}
                </ul>
              </Section>
            )}
            {can("customers.create") && data.by_branch.length <= 1 && (
              <Link to="/customers?new=1" className="surface flex items-center gap-3 p-5 transition hover:shadow-lift">
                <span className="rounded-xl bg-primary/10 p-3 text-primary"><UserPlus className="h-5 w-5" /></span>
                <span>
                  <span className="block font-semibold">Grow your customer book</span>
                  <span className="text-sm text-muted-foreground">Every identified sale earns loyalty points.</span>
                </span>
              </Link>
            )}
          </div>
        </div>
      )}
    </>
  );
}

function RankList({ title, rows, onClick, plain }: { title: string; rows: { id: string; name: string; value: string; sub: string; medal?: string | null }[]; onClick: (id: string) => void; plain?: boolean }) {
  return (
    <Section title={title}>
      {rows.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted-foreground">Nothing yet</p>
      ) : (
        <ul className="divide-y">
          {rows.map((r, i) => (
            <li key={r.id}>
              <button onClick={() => onClick(r.id)} className="flex w-full items-center gap-3 py-2.5 text-left text-sm">
                {!plain && <span className="num flex w-5 shrink-0 justify-center text-xs text-muted-foreground">{r.medal ? <Medal tier={r.medal} /> : i + 1}</span>}
                <span className="min-w-0 flex-1 truncate">{r.name}</span>
                <span className="text-right">
                  <span className="num block font-medium">{r.value}</span>
                  <span className="num block text-xs text-muted-foreground">{r.sub}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}
