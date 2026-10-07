import { useState } from "react";
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { FileDown, FileSpreadsheet, Loader2 } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, download } from "@/lib/api";
import { useSession } from "@/lib/session";
import { amount, date, dateTime, titleCase } from "@/lib/format";
import { tablePdf } from "@/lib/pdf";
import type { Category, Paged, Product, UserRow } from "@/lib/types";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { ErrorState, Loading, PageHeader, EmptyState } from "@/components/Page";
import { PeriodFilter, type PeriodValue } from "@/components/Filters";
import { Select } from "@/components/Form";
import { DataList, type Column } from "@/components/DataList";
import { t } from "@/lib/i18n";

interface Col {
  key: string;
  label: string;
  kind: "text" | "money" | "int" | "percent" | "date" | "datetime";
}
interface ReportData {
  report: { key: string; title: string; description: string };
  business: string;
  from: string;
  to: string;
  columns: Col[];
  rows: Record<string, unknown>[];
  totals: Record<string, unknown>;
  can_export: boolean;
}

function cell(kind: Col["kind"], v: unknown) {
  if (v === null || v === undefined || v === "") return <span className="text-muted-foreground">—</span>;
  switch (kind) {
    case "money":
      return <span className="num">{amount(v as string, true)}</span>;
    case "int":
      return <span className="num">{amount(v as string)}</span>;
    case "percent":
      return <span className="num">{String(v)}%</span>;
    case "datetime":
      return <span className="whitespace-nowrap">{dateTime(String(v))}</span>;
    case "date":
      return <span className="whitespace-nowrap">{date(String(v))}</span>;
    default:
      return typeof v === "string" && /^[a-z_]+$/.test(v) ? titleCase(v) : String(v);
  }
}

export default function ReportView() {
  const { key } = useParams();
  const { profile, can } = useSession();
  const [period, setPeriod] = useState<PeriodValue>({ period: "month" });
  const [branchId, setBranchId] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [productId, setProductId] = useState("");
  const [userId, setUserId] = useState("");
  const [exporting, setExporting] = useState<"pdf" | "xlsx" | null>(null);
  const query = { ...period, branch_id: branchId, category_id: categoryId, product_id: productId, user_id: userId };
  const { data, isLoading, error, refetch, isFetching } = useQuery({
    queryKey: ["report", key, query],
    queryFn: () => api<ReportData>(`/reports/${key}`, { query }),
    placeholderData: (p) => p,
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const products = useQuery({ queryKey: ["products", "report-filter"], queryFn: () => api<Paged<Product>>("/products", { query: { status: "all", limit: 500 } }), enabled: can("products.view") });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api<UserRow[]>("/users"), enabled: can("users.manage") || can("approvals.approve") });

  const exportPdf = async () => {
    if (!data) return;
    setExporting("pdf");
    try {
      await tablePdf({
        title: data.report.title,
        subtitle: `${date(data.from)} – ${date(data.to)}`,
        business: data.business,
        columns: data.columns,
        rows: data.rows,
        totals: data.totals,
        filename: `${data.report.key}-${data.from}-${data.to}.pdf`,
      });
    } finally {
      setExporting(null);
    }
  };
  const exportXlsx = async () => {
    if (!data) return;
    setExporting("xlsx");
    try {
      await download(`/reports/${key}`, { ...query, format: "xlsx" }, `${data.report.key}-${data.from}-${data.to}.xlsx`);
    } catch (e) {
      toast.error(e);
    } finally {
      setExporting(null);
    }
  };

  if (error && !data) return <ErrorState error={error} retry={refetch} />;
  if (isLoading || !data) return <Loading />;
  const columns: Column<Record<string, unknown>>[] = data.columns.map((c, i) => ({
    key: c.key,
    header: c.label,
    align: ["money", "int", "percent"].includes(c.kind) ? "right" : "left",
    cell: (r) => cell(c.kind, r[c.key]),
    hideBelow: i >= 7 ? "xl" : undefined,
  }));
  const [first, ...rest] = data.columns;
  const firstNumeric = rest.find((c) => ["money", "int"].includes(c.kind));

  return (
    <>
      <PageHeader
        back="/reports"
        eyebrow={`${date(data.from)} – ${date(data.to)}`}
        title={data.report.title}
        description={data.report.description}
        actions={
          data.can_export && (
            <>
              <ActionButton variant="outline" busy={exporting === "pdf"} disabled={!!exporting} blockedBy={[!data.rows.length && REASONS.noRecords]} onAction={exportPdf}><FileDown /> PDF</ActionButton>
              <ActionButton variant="outline" online busy={exporting === "xlsx"} disabled={!!exporting} blockedBy={[!data.rows.length && REASONS.noRecords]} onAction={exportXlsx}><FileSpreadsheet /> Excel</ActionButton>
            </>
          )
        }
      />
      <div className="mb-4 space-y-3">
        <PeriodFilter value={period} onChange={setPeriod} />
        <div className="grid grid-cols-2 gap-2 md:flex md:flex-wrap">
          {(profile?.branches.length ?? 0) > 1 && (
            <Select value={branchId} onChange={setBranchId} className="md:w-48" label="Branch">
              <option value="">{t("All my branches")}</option>
              {profile?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
            </Select>
          )}
          <Select value={categoryId} onChange={setCategoryId} className="md:w-48" label="Category">
            <option value="">{t("All categories")}</option>
            {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
          {products.data && (
            <Select value={productId} onChange={setProductId} className="md:w-56">
              <option value="">{t("All products")}</option>
              {products.data.items.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
            </Select>
          )}
          {users.data && (
            <Select value={userId} onChange={setUserId} className="md:w-48">
              <option value="">{t("All users")}</option>
              {users.data.map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
            </Select>
          )}
          {isFetching && <Loader2 className="h-5 w-5 animate-spin self-center text-muted-foreground" />}
        </div>
      </div>
      <DataList
        rows={data.rows.map((row, i): Record<string, unknown> => ({ ...row, __row: i }))}
        rowKey={(r) => String(r.__row)}
        columns={columns}
        empty={<EmptyState icon={FileSpreadsheet} title="No data for these filters" />}
        mobile={(r) => (
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="truncate font-medium">{cell(first.kind, r[first.key])}</div>
              <div className="mt-0.5 space-x-2 text-xs text-muted-foreground">
                {rest.filter((c) => c !== firstNumeric).slice(0, 3).map((c) => <span key={c.key}>{c.label}: {cell(c.kind, r[c.key])}</span>)}
              </div>
            </div>
            {firstNumeric && <div className="num shrink-0 font-semibold">{cell(firstNumeric.kind, r[firstNumeric.key])}</div>}
          </div>
        )}
        footer={
          Object.keys(data.totals).length > 0 && (
            <div className="flex flex-wrap gap-x-6 gap-y-1 border-t bg-muted/40 px-4 py-3 text-sm">
              <span className="font-semibold">{t("Totals")}</span>
              {data.columns.filter((c) => data.totals[c.key] !== undefined).map((c) => (
                <span key={c.key} className="text-muted-foreground">{c.label}: <span className="font-semibold text-foreground">{cell(c.kind, data.totals[c.key])}</span></span>
              ))}
              <span className="ms-auto text-muted-foreground">{data.rows.length} rows</span>
            </div>
          )
        }
      />
    </>
  );
}
