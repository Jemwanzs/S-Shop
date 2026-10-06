import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { BarChart3, ImagePlus, PackagePlus, Pencil, Power, Star, Trash2 } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { AddPhotosDialog } from "@/components/PhotoPicker";
import { count, dateTime, money, signed, titleCase } from "@/lib/format";
import type { Outcome, Paged, Product } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { Pill, StockIndicator } from "@/components/Badges";
import { ConfirmDialog } from "@/components/Form";
import { PhotoGallery } from "@/components/PhotoGallery";
import { CustomFieldValues } from "@/components/CustomFields";
import { t } from "@/lib/i18n";

interface Detail {
  product: Product;
  photos: { id: string; is_primary: boolean; url: string }[];
  branch_ids: string[];
  stock_by_branch: { branch_id: string; branch_name: string; on_hand: number; reserved: number; available: number; is_current: boolean }[];
  pending_approval_id: string | null;
}

interface Movement {
  id: string;
  created_at: string;
  branch_name: string;
  kind: string;
  quantity: number;
  barcode: string | null;
  notes: string;
  user_name: string | null;
}

export default function ProductDetail() {
  const { id } = useParams();
  const { can, profile } = useSession();
  const qc = useQueryClient();
  const [gallery, setGallery] = useState(false);
  const [toggling, setToggling] = useState(false);
  const [adding, setAdding] = useState(false);
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["product", id], queryFn: () => api<Detail>(`/products/${id}`) });
  const movements = useQuery({
    queryKey: ["stock", "movements", id],
    queryFn: () => api<Paged<Movement>>("/stock/movements", { query: { product_id: id, period: "all", limit: 15 } }),
    enabled: can("stock.view"),
  });
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["product", id] });
    qc.invalidateQueries({ queryKey: ["products"] });
  };

  const status = useMutation({
    mutationFn: () => api<Outcome<unknown>>(`/products/${id}/status`, { body: { is_active: !data!.product.is_active } }),
    onSuccess: (r) => {
      toast.success(r.pending_approval ? "Sent for approval" : data!.product.is_active ? "Product deactivated" : "Product activated");
      setToggling(false);
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const photoAction = async (photoId: string, action: "primary" | "delete") => {
    try {
      await api(`/products/${id}/photos/${photoId}${action === "primary" ? "/primary" : ""}`, { method: action === "primary" ? "POST" : "DELETE" });
      refresh();
    } catch (e) {
      toast.error(e);
    }
  };

  if (error) return <ErrorState error={error} retry={refetch} />;
  if (isLoading || !data) return <Loading />;
  const p = data.product;
  const canPhotos = can("products.edit") || can("products.create");
  const maxPhotos = profile?.settings.product.max_photos ?? 5;
  const totalOnHand = data.stock_by_branch.reduce((a, b) => a + b.on_hand, 0);

  return (
    <>
      <PageHeader
        back="/products"
        eyebrow={`${p.code}${p.category_name ? ` · ${p.category_name}` : ""}`}
        title={p.name}
        description={p.nickname && `“${p.nickname}”`}
        actions={
          <>
            {can("dashboard.view") && <Button variant="outline" asChild><Link to={`/?product=${p.id}`}><BarChart3 /> {t("Performance")}</Link></Button>}
            {can("stock.add") && <Button variant="outline" asChild><Link to={`/stock/receive?product=${p.id}`}><PackagePlus /> {t("Receive stock")}</Link></Button>}
            {can("products.edit") && <Button asChild><Link to={`/products/${p.id}/edit`}><Pencil /> {t("Edit")}</Link></Button>}
          </>
        }
      />
      {data.pending_approval_id && <div className="mb-5 rounded-xl bg-warning/10 p-4 text-sm text-warning">{t("A change to this product is awaiting approval.")}</div>}
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        <div className="space-y-5">
          <div className="grid gap-3 sm:grid-cols-3">
            <div className="surface card-body"><p className="label-caps">{t("Marked price")}</p><p className="num mt-1 text-2xl font-semibold">{money(p.marked_price)}</p></div>
            <div className="surface card-body"><p className="label-caps">{t("Here now")}</p><p className="num mt-1 text-2xl font-semibold">{count(p.available)}</p><p className="num text-xs text-muted-foreground">{count(p.reserved)} reserved</p></div>
            <div className="surface card-body"><p className="label-caps">{t("All branches")}</p><p className="num mt-1 text-2xl font-semibold">{count(totalOnHand)}</p></div>
          </div>

          <Section title={`Photos · ${data.photos.length}/${maxPhotos}`} action={canPhotos && data.photos.length < maxPhotos && (
            <button type="button" className="inline-flex items-center gap-1.5 text-sm text-primary" onClick={() => setAdding(true)}>
              <ImagePlus className="h-4 w-4" /> {t("Add")}
            </button>
          )}>
            {data.photos.length === 0 ? (
              <p className="py-4 text-sm text-muted-foreground">{t("No photos. The primary photo is shown on the ordering link.")}</p>
            ) : (
              <div className="flex flex-wrap gap-3">
                {data.photos.map((ph) => (
                  <div key={ph.id} className={cn("group relative h-28 w-28 overflow-hidden rounded-xl border-2", ph.is_primary ? "border-primary" : "border-transparent")}>
                    <button className="h-full w-full" onClick={() => setGallery(true)}><img src={ph.url} alt="" className="h-full w-full object-cover" loading="lazy" /></button>
                    {ph.is_primary && <span className="absolute start-1 top-1 rounded bg-primary px-1.5 text-[10px] font-semibold text-primary-foreground">{t("Primary")}</span>}
                    {canPhotos && (
                      <div className="absolute inset-x-1 bottom-1 flex justify-end gap-1">
                        {!ph.is_primary && <button className="rounded-full bg-black/60 p-1.5 text-white" onClick={() => photoAction(ph.id, "primary")} aria-label="Make primary"><Star className="h-3 w-3" /></button>}
                        <button className="rounded-full bg-black/60 p-1.5 text-white" onClick={() => photoAction(ph.id, "delete")} aria-label="Remove photo"><Trash2 className="h-3 w-3" /></button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </Section>

          {can("stock.view") && (
            <Section title="Recent stock movements" action={<Link to={`/stock?tab=movements&product=${p.id}`} className="text-xs text-primary">{t("All")}</Link>}>
              {movements.data?.items.length ? (
                <ul className="divide-y text-sm">
                  {movements.data.items.map((m) => (
                    <li key={m.id} className="flex items-center gap-3 py-2.5">
                      <div className="min-w-0 flex-1">
                        <div className="font-medium">{titleCase(m.kind)}{m.barcode && <span className="num text-muted-foreground"> · {m.barcode}</span>}</div>
                        <div className="truncate text-xs text-muted-foreground">{dateTime(m.created_at)} · {m.branch_name} · {m.user_name}{m.notes && ` · ${m.notes}`}</div>
                      </div>
                      <span className={cn("num font-semibold", m.quantity > 0 ? "text-success" : "text-destructive")}>{signed(m.quantity)}</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="py-4 text-sm text-muted-foreground">{t("No stock received yet.")}</p>
              )}
            </Section>
          )}
        </div>

        <div className="space-y-5">
          <Section title="Stock by branch">
            <ul className="divide-y">
              {data.stock_by_branch.map((b) => (
                <li key={b.branch_id} className="flex items-center justify-between gap-2 py-2.5 text-sm">
                  <span>{b.branch_name}{b.is_current && <span className="text-muted-foreground"> (current)</span>}</span>
                  <StockIndicator available={b.available} threshold={p.low_stock_threshold ?? profile?.settings.stock.low_stock_threshold} />
                </li>
              ))}
            </ul>
          </Section>
          <Section title="Details">
            <KV label="Status">{p.is_active ? <Pill tone="success">{t("Active")}</Pill> : <Pill tone="danger">{t("Inactive")}</Pill>}</KV>
            {p.max_discount !== null && <KV label="Max discount"><span className="num">{money(p.max_discount)}</span></KV>}
            {p.cost_price !== null && can("sales.view_financials") && <KV label="Cost price"><span className="num">{money(p.cost_price)}</span></KV>}
            <KV label="Barcode">{p.track_items ? "Per item" : p.barcode ? <span className="num">{p.barcode}</span> : "—"}</KV>
            <KV label="Supplier">{p.supplier_name ?? "—"}</KV>
            <KV label="Ordering link">{p.available_for_orders ? "Visible" : "Hidden"}</KV>
            <KV label="Transfers">{p.transfer_allowed ? "Allowed" : "Not allowed"}</KV>
            <KV label="Branches">{p.all_branches ? "All" : `${data.branch_ids.length} selected`}</KV>
            <KV label="Loyalty">
              {p.loyalty_eligible
                ? `${p.loyalty_points_per ?? profile?.settings.loyalty.points_per} pt per ${money(p.loyalty_threshold ?? profile?.settings.loyalty.threshold)}`
                : "Not eligible"}
            </KV>
            <CustomFieldValues kind="product" values={p.custom_fields ?? {}} />
            {p.description && <p className="mt-2 rounded-lg bg-muted p-3 text-sm">{p.description}</p>}
          </Section>
          {p.track_items && (
            <Button variant="outline" className="w-full" asChild><Link to={`/stock?tab=barcodes&product=${p.id}`}>{t("View barcoded items")}</Link></Button>
          )}
          {can("products.deactivate") && !data.pending_approval_id && (
            <Button variant="outline" className={cn("w-full", p.is_active && "text-destructive")} onClick={() => setToggling(true)}>
              <Power /> {p.is_active ? "Deactivate product" : "Activate product"}
            </Button>
          )}
        </div>
      </div>
      {canPhotos && (
        <AddPhotosDialog productId={id!} existing={data.photos.length} max={maxPhotos} open={adding} onOpenChange={setAdding} onSaved={refresh} />
      )}
      <PhotoGallery open={gallery} onOpenChange={setGallery} title={p.name} urls={data.photos.map((x) => x.url)} />
      <ConfirmDialog
        open={toggling}
        onOpenChange={setToggling}
        title={p.is_active ? `Deactivate ${p.name}?` : `Activate ${p.name}?`}
        description={p.is_active ? "It will no longer be sellable or orderable. History is kept." : "It becomes sellable again."}
        destructive={p.is_active}
        confirmLabel={p.is_active ? "Deactivate" : "Activate"}
        busy={status.isPending}
        onConfirm={() => status.mutate()}
      />
    </>
  );
}
