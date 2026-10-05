import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Plus, Receipt } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, dateTime, methodLabel, money, time } from "@/lib/format";
import type { SaleRow, UserRow } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { PeriodFilter, SearchInput, type PeriodValue } from "@/components/Filters";
import { NativeSelect } from "@/components/Form";
import { StatusBadge } from "@/components/Badges";
import { StatCard } from "@/components/Stat";

const LIMIT = 50;

export default function SalesList() {
  const { profile, can } = useSession();
  const navigate = useNavigate();
  const [period, setPeriod] = useState<PeriodValue>({ period: "today" });
  const [q, setQ] = useState("");
  const [method, setMethod] = useState("");
  const [status, setStatus] = useState("");
  const [branchId, setBranchId] = useState("");
  const [userId, setUserId] = useState("");
  const [offset, setOffset] = useState(0);
  const term = useDebounced(q);
  const query = { ...period, q: term, payment_method: method, status, branch_id: branchId, user_id: userId, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["sales", query],
    queryFn: () => api<{ items: SaleRow[]; total: number; summary: { count: number; total: string; discount: string } }>("/sales", { query }),
    placeholderData: (p) => p,
  });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api<UserRow[]>("/users"), enabled: can("users.manage") || can("approvals.approve") });
  const reset = <T,>(fn: (v: T) => void) => (v: T) => { fn(v); setOffset(0); };

  return (
    <>
      <PageHeader
        eyebrow="Sales"
        title="Sales history"
        actions={can("sales.create") && <Button asChild><Link to="/pos"><Plus /> New sale</Link></Button>}
      />
      <div className="mb-4 space-y-3">
        <PeriodFilter value={period} onChange={reset(setPeriod)} />
        <div className="grid grid-cols-2 gap-2 md:flex md:flex-wrap">
          <SearchInput value={q} onChange={reset(setQ)} placeholder="Receipt, customer or mobile" className="col-span-2 md:w-72" />
          <NativeSelect value={method} onChange={reset(setMethod)} className="md:w-40">
            <option value="">All payments</option>
            {profile?.settings.sales.payment_methods.map((m) => <option key={m.key} value={m.key}>{m.label}</option>)}
          </NativeSelect>
          <NativeSelect value={status} onChange={reset(setStatus)} className="md:w-44">
            <option value="">All statuses</option>
            <option value="completed">Completed</option>
            <option value="partially_returned">Part returned</option>
            <option value="returned">Returned</option>
            <option value="cancelled">Cancelled</option>
          </NativeSelect>
          {(profile?.branches.length ?? 0) > 1 && (
            <NativeSelect value={branchId} onChange={reset(setBranchId)} className="md:w-44">
              <option value="">All my branches</option>
              {profile?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
            </NativeSelect>
          )}
          {users.data && (
            <NativeSelect value={userId} onChange={reset(setUserId)} className="md:w-44">
              <option value="">All staff</option>
              {users.data.map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
            </NativeSelect>
          )}
        </div>
      </div>
      {data && (
        <div className="mb-4 grid grid-cols-3 gap-3 lg:max-w-3xl">
          <StatCard label="Sales" value={count(data.summary.count)} />
          <StatCard label="Value" value={money(data.summary.total)} tone="success" />
          <StatCard label="Discounts" value={money(data.summary.discount)} />
        </div>
      )}
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => navigate(`/sales/${r.id}`)}
        empty={<EmptyState icon={Receipt} title="No sales in this period" />}
        columns={[
          { key: "receipt", header: "Receipt", cell: (r) => <span className="num font-medium">{r.receipt_no}</span> },
          { key: "date", header: "Date", cell: (r) => <span className="whitespace-nowrap text-muted-foreground">{dateTime(r.created_at)}</span> },
          { key: "customer", header: "Customer", cell: (r) => r.customer_name ?? <span className="text-muted-foreground">Walk-in</span> },
          { key: "branch", header: "Branch", cell: (r) => r.branch_name, hideBelow: "xl" },
          { key: "user", header: "Salesperson", cell: (r) => r.user_name ?? "—", hideBelow: "lg" },
          { key: "items", header: "Items", align: "right", cell: (r) => <span className="num">{count(r.item_count)}</span>, hideBelow: "xl" },
          { key: "method", header: "Payment", cell: (r) => methodLabel(r.payment_method) },
          { key: "status", header: "Status", cell: (r) => <StatusBadge status={r.status} /> },
          { key: "total", header: "Total", align: "right", cell: (r) => <span className="num font-semibold">{money(r.total)}</span> },
        ]}
        mobile={(r) => (
          <CardRow
            title={r.customer_name ?? "Walk-in customer"}
            subtitle={<span className="num">{r.receipt_no} · {time(r.created_at)} · {methodLabel(r.payment_method)}</span>}
            value={money(r.total)}
            meta={r.status !== "completed" ? <StatusBadge status={r.status} /> : r.points_earned > 0 ? `🌼 +${r.points_earned}` : undefined}
          />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}
