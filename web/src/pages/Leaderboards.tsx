import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Trophy } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { count, money, toNum } from "@/lib/format";
import { t } from "@/lib/i18n";
import type { Category, Money } from "@/lib/types";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, CardRow, type Column } from "@/components/DataList";
import { PeriodFilter, Segments, type PeriodValue } from "@/components/Filters";
import { NativeSelect } from "@/components/Form";
import { Medal } from "@/components/Badges";

type Kind = "products" | "staff";
interface Row {
  id: string;
  name: string;
  medal: string | null;
  [k: string]: unknown;
}

const LABELS: Record<string, string> = {
  revenue: "Sales value", units: "Units", sales: "Sales", orders: "Orders", profit: "Profit", margin: "Margin",
  transactions: "Transactions", avg_sale: "Average sale", customers: "Customers served", new_customers: "New customers",
  discounts: "Discounts given", credit: "Credit sales",
};
const MONEY = new Set(["revenue", "profit", "discounts", "credit", "avg_sale"]);

function fmt(metric: string, v: unknown) {
  if (v === null || v === undefined) return "—";
  if (metric === "margin") return `${toNum(v as Money).toFixed(1)}%`;
  return MONEY.has(metric) ? money(v as Money) : count(v as number);
}

/** Ranked products and staff by a chosen metric — same figures as the dashboard and reports. */
export default function Leaderboards() {
  const { profile, can } = useSession();
  const navigate = useNavigate();
  const canStaff = can("staff.view_others");
  const [kind, setKind] = useState<Kind>("products");
  const [metric, setMetric] = useState("revenue");
  const [period, setPeriod] = useState<PeriodValue>({ period: "month" });
  const [branchId, setBranchId] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const query = { ...period, branch_id: branchId, category_id: kind === "products" ? categoryId : "", metric, limit: 50 };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["leaderboard", kind, query],
    queryFn: () => api<{ metric: string; metrics: string[]; items: Row[]; from: string; to: string }>(`/leaderboards/${kind}`, { query }),
    placeholderData: (p) => p,
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories"), enabled: kind === "products" });
  const metrics = data?.metrics ?? ["revenue"];
  const secondary = (kind === "products" ? ["revenue", "units", "sales", "orders", "profit", "margin"] : ["revenue", "transactions", "units", "avg_sale", "customers"])
    .filter((m) => m !== metric && metrics.includes(m))
    .slice(0, 2);

  const rank = (r: Row, i: number) => (
    <span className="num flex w-6 shrink-0 justify-center text-xs text-muted-foreground">{r.medal ? <Medal tier={r.medal} /> : i + 1}</span>
  );
  const rows = data?.items ?? [];
  const index = new Map(rows.map((r, i) => [r.id, i]));
  const columns: Column<Row>[] = [
    { key: "rank", header: "#", cell: (r) => rank(r, index.get(r.id) ?? 0), className: "w-10" },
    {
      key: "name",
      header: kind === "products" ? t("Product") : t("Employee"),
      cell: (r) => (
        <div className="min-w-0">
          <div className="truncate font-medium">{r.name}</div>
          <div className="truncate text-xs text-muted-foreground">{kind === "products" ? [r.code, r.category].filter(Boolean).join(" · ") : String(r.role ?? "")}</div>
        </div>
      ),
    },
    ...metrics.map((m) => ({
      key: m,
      header: t(LABELS[m] ?? m),
      align: "right" as const,
      cell: (r: Row) => <span className={m === metric ? "num font-semibold" : "num text-muted-foreground"}>{fmt(m, r[m])}</span>,
      hideBelow: m === metric || ["revenue", "units"].includes(m) ? undefined : ("xl" as const),
    })),
  ];

  return (
    <>
      <PageHeader eyebrow="Analytics" title="Leaderboards" description={data ? `${data.from === data.to ? data.from : `${data.from} → ${data.to}`}` : undefined} />
      <div className="mb-4 space-y-3">
        {canStaff && (
          <Segments
            value={kind}
            onChange={(k) => { setKind(k); setMetric("revenue"); }}
            options={[{ value: "products", label: "Products" }, { value: "staff", label: "Staff" }]}
          />
        )}
        <PeriodFilter value={period} onChange={setPeriod} />
        <div className="grid grid-cols-2 gap-2 sm:flex sm:flex-wrap">
          {(profile?.branches.length ?? 0) > 1 && (
            <NativeSelect value={branchId} onChange={setBranchId} className="sm:w-48">
              <option value="">{t("All my branches")}</option>
              {profile?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
            </NativeSelect>
          )}
          {kind === "products" && (
            <NativeSelect value={categoryId} onChange={setCategoryId} className="sm:w-48">
              <option value="">{t("All categories")}</option>
              {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
            </NativeSelect>
          )}
        </div>
        <div>
          <p className="label-caps mb-1.5">{t("Rank by")}</p>
          <Segments value={metric} onChange={setMetric} options={metrics.map((m) => ({ value: m, label: LABELS[m] ?? m }))} />
        </div>
      </div>
      <DataList
        rows={rows}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={kind === "products" ? (r) => navigate(`/products/${r.id}`) : undefined}
        empty={<EmptyState icon={Trophy} title="No activity in this period" />}
        columns={columns}
        mobile={(r) => (
          <CardRow
            leading={rank(r, index.get(r.id) ?? 0)}
            title={r.name}
            subtitle={secondary.map((m) => `${t(LABELS[m])} ${fmt(m, r[m])}`).join(" · ")}
            value={<span className="num">{fmt(metric, r[metric])}</span>}
            meta={t(LABELS[metric] ?? metric)}
          />
        )}
      />
    </>
  );
}
