import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Images, Package, Plus, ScanBarcode } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, money } from "@/lib/format";
import type { Category, Paged, Product } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { SearchInput, Segments } from "@/components/Filters";
import { Select } from "@/components/Form";
import { Pill, StockIndicator } from "@/components/Badges";

const LIMIT = 50;

export default function ProductsList() {
  const { can, profile } = useSession();
  const navigate = useNavigate();
  // Arrived from a scanner's "Assign barcode": pick the product that should carry this code.
  const [params] = useSearchParams();
  const assign = can("products.edit") ? params.get("assign") : null;
  const [q, setQ] = useState("");
  const [status, setStatus] = useState<"active" | "inactive" | "all">("active");
  const [category, setCategory] = useState("");
  const [offset, setOffset] = useState(0);
  const term = useDebounced(q);
  const query = { q: term, status, category_id: category, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["products", query],
    queryFn: () => api<Paged<Product>>("/products", { query }),
    placeholderData: (p) => p,
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const low = profile?.settings.stock.low_stock_threshold ?? 3;

  return (
    <>
      <PageHeader
        eyebrow="Catalogue"
        title="Products"
        description={data ? `${count(data.total)} products` : undefined}
        actions={can("products.create") && <Button asChild><Link to="/products/new"><Plus /> New product</Link></Button>}
      />
      {assign && (
        <div className="surface card-body mb-4 flex flex-wrap items-center gap-3 border-primary/40 bg-primary/5">
          <ScanBarcode className="h-5 w-5 shrink-0 text-primary" />
          <p className="min-w-0 flex-1 text-sm">Choose the product for barcode <span className="num font-semibold">{assign}</span>, or create a new product with it.</p>
          <div className="flex gap-2">
            {can("products.create") && <Button size="sm" asChild><Link to={`/products/new?barcode=${encodeURIComponent(assign)}`}><Plus /> New product</Link></Button>}
            <Button size="sm" variant="ghost" asChild><Link to="/products">Cancel</Link></Button>
          </div>
        </div>
      )}
      <div className="mb-4 space-y-3">
        <Segments value={status} onChange={(v) => { setStatus(v); setOffset(0); }} options={[{ value: "active", label: "Active" }, { value: "inactive", label: "Inactive" }, { value: "all", label: "All" }]} />
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 md:flex">
          <SearchInput value={q} onChange={(v) => { setQ(v); setOffset(0); }} placeholder="Name, nickname, code or barcode" className="md:w-80" />
          <Select value={category} label="Category" onChange={(v) => { setCategory(v); setOffset(0); }} className="md:w-52">
            <option value="">All categories</option>
            {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
        </div>
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => navigate(assign ? `/products/${r.id}/edit?barcode=${encodeURIComponent(assign)}` : `/products/${r.id}`)}
        empty={<EmptyState icon={Package} title="No products yet" action={can("products.create") && <Button asChild><Link to="/products/new"><Plus /> Add your first product</Link></Button>} />}
        columns={[
          {
            key: "name",
            header: "Product",
            cell: (r) => (
              <div>
                <div className="flex items-center gap-1.5 font-medium">{r.name}{r.photo_count > 0 && <Images className="h-3.5 w-3.5 text-muted-foreground" />}{r.track_items && <ScanBarcode className="h-3.5 w-3.5 text-muted-foreground" />}</div>
                <div className="text-xs text-muted-foreground">{r.code}{r.nickname && ` · ${r.nickname}`}</div>
              </div>
            ),
          },
          { key: "category", header: "Category", cell: (r) => r.category_name ?? "—", hideBelow: "lg" },
          { key: "price", header: "Price", align: "right", cell: (r) => <span className="num font-medium">{money(r.marked_price)}</span> },
          { key: "stock", header: "Here", cell: (r) => <StockIndicator available={r.available} threshold={r.low_stock_threshold ?? low} /> },
          { key: "orders", header: "Online", cell: (r) => (r.available_for_orders ? <Pill tone="success">Yes</Pill> : <Pill>No</Pill>), hideBelow: "xl" },
          { key: "status", header: "Status", cell: (r) => (r.is_active ? <Pill tone="success">Active</Pill> : <Pill tone="danger">Inactive</Pill>) },
        ]}
        mobile={(r) => (
          <CardRow
            title={r.name}
            subtitle={`${r.code}${r.category_name ? ` · ${r.category_name}` : ""}`}
            value={money(r.marked_price)}
            meta={r.is_active ? <span className={r.available > 0 ? "text-success" : "text-destructive"}>{r.available > 0 ? `${count(r.available)} here` : "Out of stock"}</span> : <span className="text-destructive">Inactive</span>}
          />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}
