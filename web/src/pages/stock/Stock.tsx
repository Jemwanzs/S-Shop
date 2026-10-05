import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Boxes, ClipboardCheck, PackagePlus, ScanSearch, SlidersHorizontal } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, dateTime, money, signed, titleCase } from "@/lib/format";
import type { Category, Money, Paged, StockLevel } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { PeriodFilter, SearchInput, Segments, type PeriodValue } from "@/components/Filters";
import { NativeSelect } from "@/components/Form";
import { Pill, StatusBadge, StockIndicator } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { BarcodeScanner } from "@/components/BarcodeScanner";
import { AdjustDialog } from "./AdjustDialog";

type Tab = "levels" | "position" | "movements" | "adjustments" | "barcodes";
const LIMIT = 50;

export default function Stock() {
  const { can, branch } = useSession();
  const [params, setParams] = useSearchParams();
  const tab = (params.get("tab") as Tab) ?? "levels";
  const [adjusting, setAdjusting] = useState(false);
  return (
    <>
      <PageHeader
        eyebrow={branch?.name}
        title="Stock & inventory"
        actions={
          <>
            {can("stock.adjust") && <Button variant="outline" onClick={() => setAdjusting(true)}><SlidersHorizontal /> Adjust</Button>}
            {can("stock.adjust") && <Button variant="outline" asChild><Link to="/stock/count"><ClipboardCheck /> Stock take</Link></Button>}
            {can("stock.add") && <Button asChild><Link to="/stock/receive"><PackagePlus /> Receive stock</Link></Button>}
          </>
        }
      />
      <div className="mb-4">
        <Segments
          value={tab}
          onChange={(t) => setParams({ tab: t })}
          options={[
            { value: "levels", label: "Levels" },
            { value: "position", label: "Position" },
            { value: "movements", label: "Movements" },
            { value: "adjustments", label: "Adjustments" },
            { value: "barcodes", label: "Barcoded items" },
          ]}
        />
      </div>
      {tab === "levels" && <Levels />}
      {tab === "position" && <Position />}
      {tab === "movements" && <Movements productId={params.get("product") ?? undefined} />}
      {tab === "adjustments" && <Adjustments />}
      {tab === "barcodes" && <Barcodes productId={params.get("product") ?? undefined} />}
      <AdjustDialog open={adjusting} onOpenChange={setAdjusting} />
    </>
  );
}

function Levels() {
  const { profile, can } = useSession();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [status, setStatus] = useState<"all" | "in" | "low" | "out">((params.get("status") as "low") ?? "all");
  const [q, setQ] = useState("");
  const [category, setCategory] = useState("");
  const [offset, setOffset] = useState(0);
  const term = useDebounced(q);
  const query = { q: term, status, category_id: category, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["stock", "levels", query], queryFn: () => api<Paged<StockLevel>>("/stock", { query }), placeholderData: (p) => p });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const showValue = can("sales.view_financials") || profile?.settings.stock.valuation === "selling";
  return (
    <>
      <div className="mb-4 space-y-3">
        <Segments value={status} onChange={(v) => { setStatus(v); setOffset(0); }} options={[{ value: "all", label: "All" }, { value: "in", label: "In stock" }, { value: "low", label: "Low" }, { value: "out", label: "Out of stock" }]} />
        <div className="grid gap-2 sm:grid-cols-2 md:flex">
          <SearchInput value={q} onChange={(v) => { setQ(v); setOffset(0); }} placeholder="Search products" className="md:w-80" />
          <NativeSelect value={category} onChange={(v) => { setCategory(v); setOffset(0); }} className="md:w-52">
            <option value="">All categories</option>
            {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </NativeSelect>
        </div>
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.product_id}
        onRowClick={(r) => navigate(`/products/${r.product_id}`)}
        empty={<EmptyState icon={Boxes} title="No stock matches" />}
        columns={[
          { key: "name", header: "Product", cell: (r) => <div><div className="font-medium">{r.name}</div><div className="text-xs text-muted-foreground">{r.code}{r.track_items && " · per item"}</div></div> },
          { key: "category", header: "Category", cell: (r) => r.category_name ?? "—", hideBelow: "xl" },
          { key: "on_hand", header: "Physical", align: "right", cell: (r) => <span className="num">{count(r.on_hand)}</span> },
          { key: "reserved", header: "Reserved", align: "right", cell: (r) => <span className="num text-muted-foreground">{count(r.reserved)}</span>, hideBelow: "lg" },
          { key: "available", header: "Available", align: "right", cell: (r) => <span className="num font-semibold">{count(r.available)}</span> },
          { key: "status", header: "Status", cell: (r) => <StockIndicator available={r.available} threshold={r.low_threshold} /> },
          ...(showValue ? [{ key: "value", header: "Value", align: "right" as const, cell: (r: StockLevel) => <span className="num">{money(r.value)}</span>, hideBelow: "lg" as const }] : []),
        ]}
        mobile={(r) => (
          <CardRow
            title={r.name}
            subtitle={`${r.code}${r.reserved ? ` · ${count(r.reserved)} reserved` : ""}`}
            value={<span className={cn(r.available <= 0 ? "text-destructive" : r.available <= r.low_threshold ? "text-warning" : "")}>{count(r.available)}</span>}
            meta={r.available <= 0 ? "out of stock" : "available"}
          />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}

interface PositionRow {
  product_id: string; code: string; name: string; category_name: string | null; opening: number; added: number; transfers_in: number;
  transfers_out: number; sold: number; returns: number; adjustments: number; damaged_written_off: number; closing: number; reserved: number;
  available_now: number; low_threshold: number; value: Money;
}

function Position() {
  const { can } = useSession();
  const navigate = useNavigate();
  const [period, setPeriod] = useState<PeriodValue>({ period: "month" });
  const [status, setStatus] = useState<"all" | "low" | "out">("all");
  const query = { ...period, status: status === "all" ? undefined : status };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["stock", "position", query], queryFn: () => api<{ rows: PositionRow[]; total_value: Money }>("/stock/position", { query }) });
  const n = (v: number, tone?: boolean) => <span className={cn("num", tone && v > 0 && "text-success", tone && v < 0 && "text-destructive")}>{v === 0 ? "·" : count(v)}</span>;
  return (
    <>
      <div className="mb-4 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <PeriodFilter value={period} onChange={setPeriod} />
        <Segments value={status} onChange={setStatus} options={[{ value: "all", label: "All" }, { value: "low", label: "Low" }, { value: "out", label: "Out" }]} />
      </div>
      {data && can("sales.view_financials") && <p className="mb-3 text-sm text-muted-foreground">Closing stock value: <span className="num font-semibold text-foreground">{money(data.total_value)}</span></p>}
      <DataList
        rows={data?.rows}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.product_id}
        onRowClick={(r) => navigate(`/products/${r.product_id}`)}
        columns={[
          { key: "name", header: "Product", cell: (r) => <span className="font-medium">{r.name}</span> },
          { key: "opening", header: "Opening", align: "right", cell: (r) => n(r.opening) },
          { key: "added", header: "Added", align: "right", cell: (r) => n(r.added) },
          { key: "tin", header: "Trf in", align: "right", cell: (r) => n(r.transfers_in), hideBelow: "lg" },
          { key: "tout", header: "Trf out", align: "right", cell: (r) => n(r.transfers_out), hideBelow: "lg" },
          { key: "sold", header: "Sold", align: "right", cell: (r) => n(r.sold) },
          { key: "returns", header: "Returns", align: "right", cell: (r) => n(r.returns), hideBelow: "xl" },
          { key: "adj", header: "Adjust", align: "right", cell: (r) => n(r.adjustments, true), hideBelow: "xl" },
          { key: "dmg", header: "Damaged", align: "right", cell: (r) => n(r.damaged_written_off), hideBelow: "xl" },
          { key: "closing", header: "Closing", align: "right", cell: (r) => <span className="num font-semibold">{count(r.closing)}</span> },
          { key: "reserved", header: "Reserved", align: "right", cell: (r) => n(r.reserved), hideBelow: "2xl" },
          ...(can("sales.view_financials") ? [{ key: "value", header: "Value", align: "right" as const, cell: (r: PositionRow) => <span className="num">{money(r.value)}</span>, hideBelow: "lg" as const }] : []),
        ]}
        mobile={(r) => (
          <div>
            <div className="flex justify-between font-medium"><span className="truncate">{r.name}</span><span className="num">{count(r.closing)}</span></div>
            <div className="num mt-1 grid grid-cols-4 gap-1 text-[11px] text-muted-foreground">
              <span>Open {r.opening}</span><span>+{r.added + r.transfers_in}</span><span>−{r.sold + r.transfers_out}</span><span>Adj {r.adjustments - r.damaged_written_off}</span>
            </div>
          </div>
        )}
      />
    </>
  );
}

interface MovementRow { id: string; created_at: string; branch_name: string; product_id: string; product_name: string; kind: string; quantity: number; barcode: string | null; notes: string; user_name: string | null; ref_type: string | null; ref_id: string | null }
const KINDS = ["received", "opening", "sale", "order_completion", "transfer_out", "transfer_in", "customer_return", "sale_reversal", "supplier_return", "damage", "loss", "write_off", "adjustment", "count_variance"];

function refLink(r: MovementRow) {
  if (r.ref_type === "sale") return `/sales/${r.ref_id}`;
  if (r.ref_type === "transfer") return `/transfers/${r.ref_id}`;
  return null;
}

function Movements({ productId }: { productId?: string }) {
  const navigate = useNavigate();
  const [period, setPeriod] = useState<PeriodValue>({ period: productId ? "year" : "week" });
  const [kind, setKind] = useState("");
  const [offset, setOffset] = useState(0);
  const query = { ...period, kind, product_id: productId, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["stock", "movements", query], queryFn: () => api<Paged<MovementRow>>("/stock/movements", { query }), placeholderData: (p) => p });
  return (
    <>
      <div className="mb-4 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <PeriodFilter value={period} onChange={(v) => { setPeriod(v); setOffset(0); }} />
        <NativeSelect value={kind} onChange={(v) => { setKind(v); setOffset(0); }} className="md:w-52">
          <option value="">All movements</option>
          {KINDS.map((k) => <option key={k} value={k}>{titleCase(k)}</option>)}
        </NativeSelect>
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => { const l = refLink(r); if (l) navigate(l); }}
        columns={[
          { key: "date", header: "When", cell: (r) => <span className="whitespace-nowrap text-muted-foreground">{dateTime(r.created_at)}</span> },
          { key: "product", header: "Product", cell: (r) => <div><div className="font-medium">{r.product_name}</div>{r.barcode && <div className="num text-xs text-muted-foreground">{r.barcode}</div>}</div> },
          { key: "kind", header: "Movement", cell: (r) => <Pill tone={r.quantity > 0 ? "success" : "danger"}>{titleCase(r.kind)}</Pill> },
          { key: "qty", header: "Qty", align: "right", cell: (r) => <span className={cn("num font-semibold", r.quantity > 0 ? "text-success" : "text-destructive")}>{signed(r.quantity)}</span> },
          { key: "branch", header: "Branch", cell: (r) => r.branch_name, hideBelow: "lg" },
          { key: "notes", header: "Reference", cell: (r) => <span className="text-muted-foreground">{r.notes}</span>, hideBelow: "lg" },
          { key: "user", header: "By", cell: (r) => r.user_name ?? "—", hideBelow: "xl" },
        ]}
        mobile={(r) => (
          <CardRow title={r.product_name} subtitle={`${titleCase(r.kind)} · ${dateTime(r.created_at)}${r.notes ? ` · ${r.notes}` : ""}`} value={<span className={r.quantity > 0 ? "text-success" : "text-destructive"}>{signed(r.quantity)}</span>} />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}

interface AdjRow { id: string; created_at: string; branch_name: string; product_name: string; barcode: string | null; kind: string; previous_qty: number; delta: number; new_qty: number; reason: string; status: string; created_by_name: string | null; decided_by_name: string | null }

function Adjustments() {
  const [period, setPeriod] = useState<PeriodValue>({ period: "month" });
  const [offset, setOffset] = useState(0);
  const query = { ...period, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["stock", "adjustments", query], queryFn: () => api<Paged<AdjRow>>("/stock/adjustments", { query }), placeholderData: (p) => p });
  return (
    <>
      <div className="mb-4"><PeriodFilter value={period} onChange={(v) => { setPeriod(v); setOffset(0); }} /></div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        empty={<EmptyState icon={SlidersHorizontal} title="No adjustments in this period" />}
        columns={[
          { key: "date", header: "When", cell: (r) => <span className="whitespace-nowrap text-muted-foreground">{dateTime(r.created_at)}</span> },
          { key: "product", header: "Product", cell: (r) => <div><div className="font-medium">{r.product_name}</div>{r.barcode && <div className="num text-xs text-muted-foreground">{r.barcode}</div>}</div> },
          { key: "kind", header: "Type", cell: (r) => titleCase(r.kind) },
          { key: "change", header: "Before → after", align: "right", cell: (r) => <span className="num whitespace-nowrap">{r.previous_qty} → {r.new_qty} <span className={r.delta > 0 ? "text-success" : "text-destructive"}>({signed(r.delta)})</span></span> },
          { key: "reason", header: "Reason", cell: (r) => r.reason, hideBelow: "lg" },
          { key: "by", header: "By / approved", cell: (r) => <span className="text-sm">{r.created_by_name}{r.decided_by_name && r.decided_by_name !== r.created_by_name && <span className="text-muted-foreground"> · ✓ {r.decided_by_name}</span>}</span>, hideBelow: "xl" },
          { key: "status", header: "Status", cell: (r) => <StatusBadge status={r.status} /> },
        ]}
        mobile={(r) => <CardRow title={r.product_name} subtitle={`${titleCase(r.kind)} · ${r.reason}`} value={signed(r.delta)} meta={<StatusBadge status={r.status} />} />}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}

interface ItemRow { id: string; barcode: string; status: string; product_id: string; product_name: string; branch_name: string; created_at: string; updated_at: string }
interface History { barcode: string; items: ItemRow[]; history: MovementRow[]; product_barcode_of: { id: string; name: string } | null }

function Barcodes({ productId }: { productId?: string }) {
  const [status, setStatus] = useState("in_stock");
  const [q, setQ] = useState("");
  const [offset, setOffset] = useState(0);
  const [lookup, setLookup] = useState<string | null>(null);
  const [scan, setScan] = useState(false);
  const term = useDebounced(q);
  const query = { status: status || undefined, q: term, product_id: productId, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["stock", "items", query], queryFn: () => api<Paged<ItemRow>>("/stock/items", { query }), placeholderData: (p) => p });
  const history = useQuery({ queryKey: ["stock", "barcode", lookup], queryFn: () => api<History>(`/stock/barcode/${encodeURIComponent(lookup!)}`), enabled: !!lookup });
  return (
    <>
      <div className="mb-4 space-y-3">
        <Segments value={status} onChange={(v) => { setStatus(v); setOffset(0); }} options={[{ value: "in_stock", label: "In stock" }, { value: "reserved", label: "Reserved" }, { value: "in_transit", label: "In transit" }, { value: "sold", label: "Sold" }, { value: "written_off", label: "Written off" }, { value: "", label: "All" }]} />
        <div className="flex gap-2">
          <SearchInput value={q} onChange={(v) => { setQ(v); setOffset(0); }} placeholder="Search barcode" className="flex-1 md:max-w-sm" />
          <Button variant="outline" onClick={() => setScan(true)}><ScanSearch /> Trace barcode</Button>
        </div>
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => setLookup(r.barcode)}
        empty={<EmptyState icon={ScanSearch} title="No barcoded items" hint="Products set to “track each item” appear here once received." />}
        columns={[
          { key: "barcode", header: "Barcode", cell: (r) => <span className="num font-medium">{r.barcode}</span> },
          { key: "product", header: "Product", cell: (r) => r.product_name },
          { key: "branch", header: "Branch", cell: (r) => r.branch_name, hideBelow: "lg" },
          { key: "status", header: "Status", cell: (r) => <StatusBadge status={r.status} /> },
          { key: "updated", header: "Last change", cell: (r) => <span className="text-muted-foreground">{dateTime(r.updated_at)}</span>, hideBelow: "lg" },
        ]}
        mobile={(r) => <CardRow title={<span className="num">{r.barcode}</span>} subtitle={`${r.product_name} · ${r.branch_name}`} meta={<StatusBadge status={r.status} />} />}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
      <BarcodeScanner open={scan} onOpenChange={setScan} onDetected={setLookup} title="Trace a barcode" />
      <ResponsiveDialog open={!!lookup} onOpenChange={(o) => !o && setLookup(null)} title={<span className="num">{lookup}</span>} description="Barcode history">
        {history.isLoading ? (
          <p className="py-6 text-center text-sm text-muted-foreground">Loading…</p>
        ) : history.data && (
          <div className="space-y-4">
            {history.data.product_barcode_of && <p className="text-sm">Product barcode of <span className="font-medium">{history.data.product_barcode_of.name}</span></p>}
            {history.data.items.map((i) => <p key={i.id} className="flex justify-between text-sm"><span>{i.product_name} · {i.branch_name}</span><StatusBadge status={i.status} /></p>)}
            <ol className="relative space-y-3 border-l pl-5">
              {history.data.history.map((m) => (
                <li key={m.id} className="relative text-sm">
                  <span className={cn("absolute -left-[25px] top-1.5 h-2.5 w-2.5 rounded-full", m.quantity > 0 ? "bg-success" : "bg-destructive")} />
                  <div className="font-medium">{titleCase(m.kind)} · {m.branch_name}</div>
                  <div className="text-xs text-muted-foreground">{dateTime(m.created_at)} · {m.user_name}{m.notes && ` · ${m.notes}`}</div>
                </li>
              ))}
              {history.data.history.length === 0 && <li className="text-sm text-muted-foreground">No movements recorded for this barcode.</li>}
            </ol>
          </div>
        )}
        {history.error && <p className="text-sm text-destructive">Not found</p>}
      </ResponsiveDialog>
    </>
  );
}
