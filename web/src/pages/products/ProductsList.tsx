import { useEffect, useState } from "react";
import { Link, useNavigate, useSearchParams } from "@/lib/router";
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
import { ProductThumb } from "@/pages/sales/pos/ProductGrid";
import { Pill, StockIndicator } from "@/components/Badges";
import { t } from "@/lib/i18n";
import { useRememberCatalogue } from "@/lib/catalogue";

const LIMIT = 50;

export default function ProductsList() {
  const { can, profile } = useSession();
  const navigate = useNavigate();
  // Roadmap 143: the filters live in the URL (refresh-safe, shareable, kept by Back): status, category, price, q, page.
  // Arrived from a scanner's "Assign barcode": pick the product that should carry this code.
  const [params, setParams] = useSearchParams();
  const assign = can("products.edit") ? params.get("assign") : null;
  const statusParam = params.get("status");
  const status: "active" | "inactive" | "all" = statusParam === "inactive" || statusParam === "all" ? statusParam : "active";
  const category = params.get("category") ?? "";
  // Roadmap 140: products with or without a selling price (a real 0 counts as set).
  const priceParam = params.get("price");
  const price: "" | "set" | "missing" = priceParam === "set" || priceParam === "missing" ? priceParam : "";
  const page = Math.max(1, Math.floor(Number(params.get("page")) || 1));
  const offset = (page - 1) * LIMIT;
  const qParam = params.get("q") ?? "";
  const [q, setQ] = useState(qParam);
  const term = useDebounced(q);
  // One change of filters → one URL change (replacing the entry, so Back leaves the catalogue rather than undoing filters).
  const update = (patch: Record<string, string | null>, keepPage = false) =>
    setParams((prev) => {
      const next = new URLSearchParams(prev);
      for (const [k, v] of Object.entries(patch)) {
        if (v === null || v === "") next.delete(k);
        else next.set(k, v);
      }
      if (!keepPage) next.delete("page");
      if (next.get("status") === "active") next.delete("status");
      return next;
    }, { replace: true });
  const setStatus = (v: string) => update({ status: v });
  const setCategory = (v: string) => update({ category: v });
  const setPrice = (v: string) => update({ price: v });
  const setOffset = (o: number) => update({ page: o > 0 ? String(Math.floor(o / LIMIT) + 1) : null }, true);
  // The search box writes to the URL once typing settles; the URL (Back, refresh) fills the box.
  useEffect(() => {
    if (term !== qParam) update({ q: term.trim() || null });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only when the settled search changes
  }, [term]);
  useEffect(() => {
    if (qParam !== term) setQ(qParam);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- follow the URL (Back / refresh)
  }, [qParam]);
  const query = { q: term, status, category_id: category, price: price || undefined, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["products", query],
    queryFn: () => api<Paged<Product>>("/products", { query }),
    placeholderData: (p) => p,
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  // Back from a product returns here — same filters, page and scroll position (per business, this tab only).
  const search = params.toString() ? `?${params.toString()}` : "";
  useRememberCatalogue(search, !!data && !isLoading);
  // Edits can empty this page (e.g. the last product here got its price under Missing price): go to the nearest page left.
  useEffect(() => {
    if (data && data.items.length === 0 && page > 1) {
      const last = Math.max(1, Math.ceil(data.total / LIMIT));
      if (last < page) setOffset((last - 1) * LIMIT);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- react to the loaded page only
  }, [data, page]);
  const low = profile?.settings.stock.low_stock_threshold ?? 3;
  // Roadmap 109: product list display preferences.
  const showImages = !!profile?.settings.product.list_show_images;
  // Filtering for missing prices always shows the price column (so "Not set" is visible).
  const showPrices = (profile?.settings.product.list_show_prices ?? true) || price === "missing";

  return (
    <>
      <PageHeader
        eyebrow="Catalogue"
        title="Products"
        description={data ? `${count(data.total)} ${t(data.total === 1 ? "product" : "products")}` : undefined}
        actions={can("products.create") && <Button asChild><Link to="/products/new"><Plus /> {t("New product")}</Link></Button>}
      />
      {assign && (
        <div className="surface card-body mb-4 flex flex-wrap items-center gap-3 border-primary/40 bg-primary/5">
          <ScanBarcode className="h-5 w-5 shrink-0 text-primary" />
          <p className="min-w-0 flex-1 text-sm">{t("Choose the product for barcode")} <span className="num font-semibold">{assign}</span>, or create a new product with it.</p>
          <div className="flex gap-2">
            {can("products.create") && <Button size="sm" asChild><Link to={`/products/new?barcode=${encodeURIComponent(assign)}`}><Plus /> {t("New product")}</Link></Button>}
            <Button size="sm" variant="ghost" asChild><Link to="/products">{t("Cancel")}</Link></Button>
          </div>
        </div>
      )}
      <div className="mb-4 space-y-3">
        <Segments value={status} onChange={(v) => setStatus(v)} options={[{ value: "active", label: "Active" }, { value: "inactive", label: "Inactive" }, { value: "all", label: "All" }]} />
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-3 md:flex">
          <SearchInput value={q} onChange={setQ} placeholder="Name, nickname, code or barcode" className="md:w-80" />
          <Select value={category} label="Category" onChange={(v) => setCategory(v)} className="md:w-52">
            <option value="">{t("All categories")}</option>
            {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
          <Select value={price} label="Price status" onChange={(v) => setPrice(v)} className="md:w-44">
            <option value="">{t("All prices")}</option>
            <option value="set">{t("Price set")}</option>
            <option value="missing">{t("Missing price")}</option>
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
        empty={<EmptyState icon={Package} title="No products yet" action={can("products.create") && <Button asChild><Link to="/products/new"><Plus /> {t("Add your first product")}</Link></Button>} />}
        columns={[
          {
            key: "name",
            header: "Product",
            cell: (r) => (
              <div className="flex items-center gap-2.5">
                {showImages && <ProductThumb id={r.primary_photo_id} className="h-9 w-9" />}
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5 font-medium">{r.name}{r.photo_count > 0 && !showImages && <Images className="h-3.5 w-3.5 text-muted-foreground" />}{r.track_items && <ScanBarcode className="h-3.5 w-3.5 text-muted-foreground" />}</div>
                  <div className="text-xs text-muted-foreground">{r.code}{r.nickname && ` · ${r.nickname}`}</div>
                </div>
              </div>
            ),
          },
          { key: "category", header: "Category", cell: (r) => r.category_name ?? "—", hideBelow: "lg" },
          ...(showPrices ? [{ key: "price", header: "Price", align: "right" as const, cell: (r: Product) => (r.marked_price == null ? <span className="text-xs text-muted-foreground">{t("Not set")}</span> : <span className="num font-medium">{money(r.marked_price)}</span>) }] : []),
          { key: "stock", header: "Here", cell: (r) => <StockIndicator available={r.available} threshold={r.low_stock_threshold ?? low} /> },
          { key: "orders", header: "Online", cell: (r) => (r.available_for_orders ? <Pill tone="success">{t("Yes")}</Pill> : <Pill>No</Pill>), hideBelow: "xl" },
          { key: "status", header: "Status", cell: (r) => (r.is_active ? <Pill tone="success">{t("Active")}</Pill> : <Pill tone="danger">{t("Inactive")}</Pill>) },
        ]}
        mobile={(r) => (
          <CardRow
            title={r.name}
            subtitle={`${r.code}${r.category_name ? ` · ${r.category_name}` : ""}`}
            value={showPrices ? (r.marked_price == null ? t("Price not set") : money(r.marked_price)) : undefined}
            meta={r.is_active ? <span className={r.available > 0 ? "text-success" : "text-destructive"}>{r.available > 0 ? `${count(r.available)} here` : "Out of stock"}</span> : <span className="text-destructive">{t("Inactive")}</span>}
          />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
    </>
  );
}
