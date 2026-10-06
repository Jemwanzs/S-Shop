import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeftRight, ArrowRight, Plus } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { count, dateTime } from "@/lib/format";
import type { Paged } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { Segments } from "@/components/Filters";
import { StatusBadge } from "@/components/Badges";
import { t } from "@/lib/i18n";

export interface TransferRow {
  id: string;
  transfer_no: string;
  from_branch_id: string;
  from_branch_name: string;
  to_branch_id: string;
  to_branch_name: string;
  status: string;
  transfer_date: string;
  notes: string;
  total_units: number;
  created_by_name: string | null;
  approved_by_name: string | null;
  dispatched_by_name: string | null;
  received_by_name: string | null;
  approved_at: string | null;
  dispatched_at: string | null;
  received_at: string | null;
  created_at: string;
  short_units: number;
  damaged_units: number;
  discrepancy_reason: string;
}

const LIMIT = 50;

export default function TransfersList() {
  const { can } = useSession();
  const navigate = useNavigate();
  const [status, setStatus] = useState("");
  const [offset, setOffset] = useState(0);
  const query = { status, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["transfers", query], queryFn: () => api<Paged<TransferRow>>("/transfers", { query }), placeholderData: (p) => p });
  return (
    <>
      <PageHeader eyebrow="Stock" title="Transfers" description="Move stock safely between branches." actions={can("stock.transfer") && <Button asChild><Link to="/transfers/new"><Plus /> {t("New transfer")}</Link></Button>} />
      <div className="mb-4">
        <Segments
          value={status}
          onChange={(v) => { setStatus(v); setOffset(0); }}
          options={[
            { value: "", label: "All" },
            { value: "draft", label: "Draft" },
            { value: "pending_approval", label: "Pending approval" },
            { value: "approved", label: "Approved" },
            { value: "in_transit", label: "In transit" },
            { value: "received", label: "Received" },
          ]}
        />
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => navigate(`/transfers/${r.id}`)}
        empty={<EmptyState icon={ArrowLeftRight} title="No transfers yet" />}
        columns={[
          { key: "no", header: "Transfer", cell: (r) => <span className="num font-medium">{r.transfer_no}</span> },
          { key: "route", header: "Route", cell: (r) => <span className="flex items-center gap-1.5 whitespace-nowrap">{r.from_branch_name} <ArrowRight className="h-3.5 w-3.5 text-muted-foreground" /> {r.to_branch_name}</span> },
          { key: "units", header: "Units", align: "right", cell: (r) => <span className="num">{count(r.total_units)}</span> },
          { key: "by", header: "Created by", cell: (r) => r.created_by_name ?? "—", hideBelow: "lg" },
          { key: "date", header: "Created", cell: (r) => <span className="whitespace-nowrap text-muted-foreground">{dateTime(r.created_at)}</span>, hideBelow: "lg" },
          { key: "status", header: "Status", cell: (r) => <StatusBadge status={r.status === "dispatched" ? "in_transit" : r.status} /> },
        ]}
        mobile={(r) => <CardRow title={`${r.from_branch_name} → ${r.to_branch_name}`} subtitle={<span className="num">{r.transfer_no} · {count(r.total_units)} {t("units")}</span>} meta={<StatusBadge status={r.status === "dispatched" ? "in_transit" : r.status} />} />}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}
