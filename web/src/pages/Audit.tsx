import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { History } from "lucide-react";
import { api } from "@/lib/api";
import { dateTime, titleCase } from "@/lib/format";
import type { Paged } from "@/lib/types";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { PeriodFilter, type PeriodValue } from "@/components/Filters";
import { NativeSelect } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

interface AuditRow {
  id: string; created_at: string; user_name: string | null; module: string; action: string; entity_type: string; entity_id: string | null;
  branch_name: string | null; before: unknown; after: unknown; approval_status: string | null; approver_name: string | null; comments: string; ip: string; user_agent: string;
}
const MODULES = ["auth", "sales", "credit", "orders", "products", "stock", "transfers", "customers", "loyalty", "expenses", "approvals", "users", "roles", "branches", "settings"];
const LIMIT = 50;

export default function Audit() {
  const [period, setPeriod] = useState<PeriodValue>({ period: "week" });
  const [module, setModule] = useState("");
  const [offset, setOffset] = useState(0);
  const [open, setOpen] = useState<AuditRow | null>(null);
  const query = { ...period, module, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["audit", query], queryFn: () => api<Paged<AuditRow>>("/audit", { query }), placeholderData: (p) => p });
  return (
    <>
      <PageHeader eyebrow="Admin" title="Audit trail" description="Who did what, when, where — with before and after values." />
      <div className="mb-4 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <PeriodFilter value={period} onChange={(v) => { setPeriod(v); setOffset(0); }} />
        <NativeSelect value={module} onChange={(v) => { setModule(v); setOffset(0); }} className="md:w-48">
          <option value="">All modules</option>
          {MODULES.map((m) => <option key={m} value={m}>{titleCase(m)}</option>)}
        </NativeSelect>
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={setOpen}
        empty={<EmptyState icon={History} title="No activity in this period" />}
        columns={[
          { key: "when", header: "When", cell: (r) => <span className="whitespace-nowrap text-muted-foreground">{dateTime(r.created_at)}</span> },
          { key: "user", header: "User", cell: (r) => r.user_name ?? "System" },
          { key: "module", header: "Module", cell: (r) => <Pill>{titleCase(r.module)}</Pill> },
          { key: "action", header: "Action", cell: (r) => <span className="font-medium">{titleCase(r.action)}</span> },
          { key: "entity", header: "Record", cell: (r) => <span className="text-muted-foreground">{titleCase(r.entity_type)}</span>, hideBelow: "lg" },
          { key: "branch", header: "Branch", cell: (r) => r.branch_name ?? "—", hideBelow: "xl" },
          { key: "approval", header: "Approval", cell: (r) => (r.approver_name ? `✓ ${r.approver_name}` : "—"), hideBelow: "xl" },
          { key: "comments", header: "Comment", cell: (r) => <span className="line-clamp-1">{r.comments}</span>, hideBelow: "2xl" },
        ]}
        mobile={(r) => <CardRow title={`${titleCase(r.action)} · ${titleCase(r.entity_type)}`} subtitle={`${r.user_name ?? "System"} · ${dateTime(r.created_at)}`} meta={titleCase(r.module)} />}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
      <ResponsiveDialog open={!!open} onOpenChange={(o) => !o && setOpen(null)} title={open ? `${titleCase(open.action)} · ${titleCase(open.entity_type)}` : ""} description={open ? `${open.user_name ?? "System"} · ${dateTime(open.created_at)}` : undefined} wide>
        {open && (
          <div className="space-y-4 text-sm">
            {open.comments && <p className="rounded-lg bg-muted p-3">{open.comments}</p>}
            <div className="grid gap-3 md:grid-cols-2">
              <div><p className="label-caps mb-1">Before</p><pre className="max-h-72 overflow-auto rounded-lg bg-muted p-3 text-xs">{open.before ? JSON.stringify(open.before, null, 2) : "—"}</pre></div>
              <div><p className="label-caps mb-1">After</p><pre className="max-h-72 overflow-auto rounded-lg bg-muted p-3 text-xs">{open.after ? JSON.stringify(open.after, null, 2) : "—"}</pre></div>
            </div>
            <p className="text-xs text-muted-foreground">Device: {open.user_agent || "—"} · IP {open.ip || "—"}{open.entity_id && ` · Record ${open.entity_id}`}</p>
          </div>
        )}
      </ResponsiveDialog>
    </>
  );
}
