import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { HandCoins } from "lucide-react";
import { api } from "@/lib/api";
import { useDebounced } from "@/lib/hooks";
import { count, date, money, toNum } from "@/lib/format";
import type { CreditRow, Money } from "@/lib/types";
import { cn } from "@/lib/utils";
import { PageHeader, EmptyState, Section } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { SearchInput, Segments } from "@/components/Filters";
import { StatusBadge } from "@/components/Badges";
import { StatCard } from "@/components/Stat";

const LIMIT = 50;
type Status = "open" | "overdue" | "paid" | "written_off" | "all";

export default function CreditList() {
  
  const navigate = useNavigate();
  const [status, setStatus] = useState<Status>("open");
  const [q, setQ] = useState("");
  const [offset, setOffset] = useState(0);
  const term = useDebounced(q);
  const query = { status, q: term, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["credit", query],
    queryFn: () => api<{ items: CreditRow[]; total: number; summary: { outstanding: Money; overdue: Money } }>("/credit", { query }),
    placeholderData: (p) => p,
  });
  const aging = useQuery({ queryKey: ["credit", "aging"], queryFn: () => api<{ buckets: { bucket: string; count: number; amount: Money }[] }>("/credit/aging") });
  const maxBucket = Math.max(...(aging.data?.buckets.map((b) => toNum(b.amount)) ?? [1]), 1);

  return (
    <>
      <PageHeader eyebrow="Sales" title="Credit Sales" description="Track what customers owe, collect repayments and follow up overdue balances." />
      <div className="mb-5 grid grid-cols-2 gap-3 xl:grid-cols-[1fr_1fr_2fr]">
        <StatCard label="Outstanding" value={money(data?.summary.outstanding)} icon={HandCoins} tone="warning" />
        <StatCard label="Overdue" value={money(data?.summary.overdue)} icon={HandCoins} tone="danger" />
        <Section title="Aging" className="col-span-2 xl:col-span-1">
          <div className="grid grid-cols-5 gap-2">
            {aging.data?.buckets.map((b) => (
              <div key={b.bucket} className="flex flex-col items-center gap-1">
                <div className="flex h-16 w-full items-end overflow-hidden rounded-md bg-muted">
                  <div className={cn("w-full rounded-md", b.bucket === "current" ? "bg-success" : b.bucket === "90+" ? "bg-destructive" : "bg-warning")} style={{ height: `${(toNum(b.amount) / maxBucket) * 100}%` }} />
                </div>
                <span className="text-[11px] text-muted-foreground">{b.bucket === "current" ? "Not due" : `${b.bucket}d`}</span>
                <span className="num text-xs font-medium">{count(b.count)}</span>
              </div>
            ))}
          </div>
        </Section>
      </div>
      <div className="mb-4 space-y-3">
        <Segments
          value={status}
          onChange={(v) => { setStatus(v); setOffset(0); }}
          options={[
            { value: "open", label: "Open" },
            { value: "overdue", label: "Overdue" },
            { value: "paid", label: "Paid" },
            { value: "written_off", label: "Written off" },
            { value: "all", label: "All" },
          ]}
        />
        <SearchInput value={q} onChange={(v) => { setQ(v); setOffset(0); }} placeholder="Customer, mobile or receipt" className="md:max-w-sm" />
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => navigate(`/credit/${r.id}`)}
        empty={<EmptyState icon={HandCoins} title="No credit here" />}
        columns={[
          { key: "customer", header: "Customer", cell: (r) => <div><div className="font-medium">{r.customer_name}</div><div className="num text-xs text-muted-foreground">{r.receipt_no}</div></div> },
          { key: "branch", header: "Branch", cell: (r) => r.branch_name, hideBelow: "xl" },
          { key: "salesperson", header: "Salesperson", cell: (r) => r.salesperson ?? "—", hideBelow: "xl" },
          { key: "amount", header: "Amount", align: "right", cell: (r) => <span className="num">{money(toNum(r.original_amount) - toNum(r.adjustments))}</span>, hideBelow: "lg" },
          { key: "paid", header: "Paid", align: "right", cell: (r) => <span className="num">{money(r.amount_paid)}</span>, hideBelow: "lg" },
          { key: "balance", header: "Balance", align: "right", cell: (r) => <span className="num font-semibold">{money(r.balance)}</span> },
          { key: "due", header: "Due", cell: (r) => <span className="whitespace-nowrap">{date(r.due_date)}</span> },
          { key: "days", header: "Days", align: "right", cell: (r) => <span className="num">{r.days_outstanding}</span>, hideBelow: "lg" },
          { key: "status", header: "Status", cell: (r) => <StatusBadge status={r.status} /> },
        ]}
        mobile={(r) => (
          <CardRow
            title={r.customer_name}
            subtitle={<span className="num">{r.receipt_no} · due {date(r.due_date)}</span>}
            value={money(r.balance)}
            meta={<StatusBadge status={r.status} />}
          />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}
