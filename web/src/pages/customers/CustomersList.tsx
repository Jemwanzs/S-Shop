import { useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { UserPlus, Users } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { ago, count, initials, maskPhone, money, toNum } from "@/lib/format";
import type { Customer, Paged } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { SearchInput, Segments } from "@/components/Filters";
import { Medal, Pill, PointsPill } from "@/components/Badges";
import { CustomerForm } from "./CustomerForm";

const LIMIT = 50;

export default function CustomersList() {
  const { currency, can } = useSession();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const [q, setQ] = useState("");
  const [sort, setSort] = useState<"spend" | "points" | "recent" | "name">("spend");
  const [offset, setOffset] = useState(0);
  const term = useDebounced(q);
  const query = { q: term, sort, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["customers", query], queryFn: () => api<Paged<Customer>>("/customers", { query }), placeholderData: (p) => p });
  const showLoyalty = can("customers.view_loyalty");
  const showCredit = can("customers.view_credit");

  return (
    <>
      <PageHeader
        eyebrow="Customers"
        title="Customer book"
        description={data ? `${count(data.total)} customers` : undefined}
        actions={can("customers.create") && <Button onClick={() => setParams({ new: "1" })}><UserPlus /> New customer</Button>}
      />
      <div className="mb-4 flex flex-col gap-3 md:flex-row md:items-center">
        <SearchInput value={q} onChange={(v) => { setQ(v); setOffset(0); }} placeholder="Name, nickname or mobile" className="md:w-80" />
        <Segments value={sort} onChange={(v) => { setSort(v); setOffset(0); }} options={[{ value: "spend", label: "Top spend" }, { value: "points", label: "Points" }, { value: "recent", label: "Recent" }, { value: "name", label: "A–Z" }]} />
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => navigate(`/customers/${r.id}`)}
        empty={<EmptyState icon={Users} title={term ? "No matching customers" : "No customers yet"} hint="Customers are added automatically at checkout or from the ordering link." />}
        columns={[
          {
            key: "name",
            header: "Customer",
            cell: (r) => (
              <div className="flex items-center gap-3">
                <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-primary/15 text-sm font-semibold text-primary">{initials(`${r.first_name} ${r.other_names}`)}</span>
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5 truncate font-medium">{r.first_name} {r.other_names}{r.tier && <Medal tier={r.tier} />}</div>
                  <div className="num text-xs text-muted-foreground">{maskPhone(r.mobile)}{r.nickname && <span className="font-sans"> · “{r.nickname}”</span>}</div>
                </div>
              </div>
            ),
          },
          { key: "spend", header: "Total spend", align: "right", cell: (r) => <span className="num font-medium">{money(r.total_spend, currency)}</span> },
          { key: "visits", header: "Purchases", align: "right", cell: (r) => <span className="num">{count(r.purchase_count)}</span>, hideBelow: "lg" },
          { key: "last", header: "Last purchase", cell: (r) => <span className="text-muted-foreground">{r.last_purchase_at ? ago(r.last_purchase_at) : "—"}</span>, hideBelow: "lg" },
          ...(showLoyalty ? [{ key: "points", header: "Points (own|ref)", align: "right" as const, cell: (r: Customer) => <PointsPill own={r.own_points} referral={r.referral_points} /> }] : []),
          { key: "tier", header: "Tier", cell: (r) => (r.tier ? <Pill tone="primary">{r.tier}</Pill> : <span className="text-muted-foreground">—</span>), hideBelow: "xl" },
          ...(showCredit ? [{ key: "credit", header: "Owes", align: "right" as const, cell: (r: Customer) => (toNum(r.credit_balance) > 0 ? <span className="num text-destructive">{money(r.credit_balance, currency)}</span> : <span className="text-muted-foreground">—</span>), hideBelow: "lg" as const }] : []),
        ]}
        mobile={(r) => (
          <CardRow
            leading={<span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-primary/15 text-sm font-semibold text-primary">{initials(`${r.first_name} ${r.other_names}`)}</span>}
            title={<span className="flex items-center gap-1.5">{r.first_name} {r.other_names}{r.tier && <Medal tier={r.tier} />}</span>}
            subtitle={<span className="num">{maskPhone(r.mobile)}</span>}
            value={money(r.total_spend, currency)}
            meta={showLoyalty ? <PointsPill own={r.own_points} referral={r.referral_points} /> : undefined}
          />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
      <CustomerForm open={params.get("new") === "1"} onOpenChange={(o) => !o && setParams({})} onSaved={(id) => navigate(`/customers/${id}`)} />
    </>
  );
}
