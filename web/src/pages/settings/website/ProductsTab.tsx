/** Products (from S'Shop): what the website publishes, how each product is presented, prices and photos; Categories. */
import { useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, EyeOff, ImagePlus, Search, Star, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { amount } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { Field, Select, ToggleRow } from "@/components/Form";
import { Loading } from "@/components/Page";
import { t } from "@/lib/i18n";
import { blankProduct, mediaUrl, useCatalogue, type CatalogueProduct, type ProductCfg, type SiteConfig } from "./data";
import { Block, Choice, Grid2, MediaField, MediaPicker, Move, moveItem } from "./kit";
import type { TabProps } from "./Website";

const ACTIONS: [string, string][] = [["enquire", "Enquire (contact page)"], ["whatsapp", "Ask on WhatsApp"], ["contact", "Call to order"], ["order", "Order — price confirmed later"]];

function entry(c: SiteConfig, id: string): ProductCfg | undefined {
  return c.products.items.find((p) => p.product_id === id);
}

/** Edits a product's website settings, creating its entry on first change. */
function useEdit(set: TabProps["set"]) {
  return (id: string, fn: (p: ProductCfg, c: SiteConfig) => void) => set((c) => {
    let p = entry(c, id);
    if (!p) {
      p = { ...blankProduct(id), published: c.products.auto_publish_new, sort: c.products.items.length };
      c.products.items.push(p);
    }
    fn(p, c);
  });
}


const PER_PAGE = [6, 10, 12, 16, 20, 24, 30];
export function ProductsTab({ c, set, ov }: TabProps) {
  const { data, isLoading } = useCatalogue();
  const qc = useQueryClient();
  const edit = useEdit(set);
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [filter, setFilter] = useState<"all" | "published" | "hidden" | "featured">("all");
  const prices = useMutation({
    mutationFn: (show: boolean) => api("/website/prices", { method: "PUT", body: { show_prices: show } }),
    onSuccess: () => {
      toast.success("Price visibility updated");
      qc.invalidateQueries({ queryKey: ["website"] });
      qc.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (e) => toast.error(e),
  });
  const published = (p: CatalogueProduct) => entry(c, p.id)?.published ?? c.products.auto_publish_new;
  const list = useMemo(() => {
    const term = q.trim().toLowerCase();
    const rows = (data?.products ?? []).filter((p) => p.is_active && (!term || `${p.name} ${p.code} ${p.category_name ?? ""}`.toLowerCase().includes(term)));
    const shown = rows.filter((p) => filter === "all" || (filter === "published" ? published(p) : filter === "hidden" ? !published(p) : !!entry(c, p.id)?.featured));
    const order = (p: CatalogueProduct) => entry(c, p.id)?.sort ?? Number.MAX_SAFE_INTEGER;
    return shown.sort((a, b) => order(a) - order(b) || a.name.localeCompare(b.name));
  }, [data, q, filter, c]); // eslint-disable-line react-hooks/exhaustive-deps
  if (isLoading) return <Loading />;
  const count = (data?.products ?? []).filter((p) => p.is_active && published(p)).length;

  return (
    <div className="space-y-4">
      <Block title="Prices & publishing">
        <ToggleRow label="Show product prices" hint="One setting for the website and the ordering link — applies straight away. Products can override it below." checked={!!ov.show_prices} disabled={prices.isPending} onChange={(v) => prices.mutate(v)} />
        <ToggleRow label="Publish new products automatically" hint="New S'Shop products appear on the website (after you publish)" checked={c.products.auto_publish_new} onChange={(v) => set((x) => { x.products.auto_publish_new = v; })} />
        <Grid2>
          <Field label="When a price is hidden, visitors can"><Select value={c.products.hidden_action} onChange={(v) => set((x) => { x.products.hidden_action = v; })}>{ACTIONS.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}</Select></Field>
          <Field label="Photos per product"><Choice value={c.products.max_photos} onChange={(v) => set((x) => { x.products.max_photos = v; })} options={[1, 2, 3, 4, 5].map((n) => [n, String(n)])} /></Field>
          <ToggleRow label="Pages of products" hint="Off: a Load more button instead (never the whole catalogue at once)." checked={c.products.pagination !== false} onChange={(v) => set((x) => { x.products.pagination = v; })} />
          <Field label="Products per page" hint="1–100">
            <div className="flex flex-wrap items-center gap-2">
              <Choice value={PER_PAGE.includes(c.products.per_page ?? 10) ? (c.products.per_page ?? 10) : 0}
                onChange={(v) => set((x) => { x.products.per_page = v || (x.products.per_page ?? 10); })}
                options={[...PER_PAGE.map((n) => [n, String(n)] as [number, string]), [0, t("Custom")]]} />
              {!PER_PAGE.includes(c.products.per_page ?? 10) && (
                <Input type="number" min={1} max={100} inputMode="numeric" className="w-24" value={c.products.per_page ?? 10}
                  onChange={(e) => set((x) => { x.products.per_page = Math.min(100, Math.max(1, Math.round(Number(e.target.value) || 1))); })} />
              )}
            </div>
          </Field>
        </Grid2>
      </Block>

      <Block title="Products" hint={`${count} ${t("of")} ${(data?.products ?? []).filter((p) => p.is_active).length} ${t("on the website")}`}>
        <div className="flex flex-wrap gap-2">
          <div className="relative min-w-0 basis-full sm:basis-0 sm:flex-1">
            <Search className="absolute start-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("Search products")} className="ps-8" />
          </div>
          <Choice value={filter} onChange={setFilter} options={[["all", "All"], ["published", "Published"], ["hidden", "Hidden"], ["featured", "Featured"]]} />
        </div>
        <ul className="divide-y">
          {list.map((p, i) => {
            const e = entry(c, p.id);
            const on = published(p);
            return (
              <li key={p.id} className="py-2">
                <div className="flex items-center gap-2">
                  <Switch checked={on} onCheckedChange={(v) => edit(p.id, (x) => { x.published = v; })} aria-label={`${t("Publish")} ${p.name}`} />
                  {p.photos[0] ? <img src={`/api/photos/${p.photos[0]}`} alt="" className="h-10 w-10 shrink-0 rounded-md object-cover" loading="lazy" /> : <span className="h-10 w-10 shrink-0 rounded-md bg-muted" />}
                  <button type="button" className="min-w-0 flex-1 text-start" onClick={() => setOpen(open === p.id ? null : p.id)} aria-expanded={open === p.id}>
                    <span className={cn("block truncate text-sm font-medium", !on && "text-muted-foreground")}>{e?.marketing_name || p.name}</span>
                    <span className="block truncate text-xs text-muted-foreground">
                      {p.category_name ?? t("No category")} · {e?.price === "hide" || (e?.price !== "show" && !ov.show_prices) ? <><EyeOff className="inline h-3 w-3" /> {t("price hidden")}</> : amount(p.price)}
                      {!p.available_for_orders && ` · ${t("not orderable")}`}
                    </span>
                  </button>
                  <Button size="icon" variant="ghost" className="h-8 w-8" aria-label={t("Featured")} aria-pressed={!!e?.featured} onClick={() => edit(p.id, (x) => { x.featured = !x.featured; })}>
                    <Star className={cn(e?.featured && "fill-warning text-warning")} />
                  </Button>
                  {filter === "all" && !q && <Move index={i} count={list.length} onMove={(a, b) => set((x) => {
                    // Persist the visible order as sort positions.
                    const ids = list.map((r) => r.id);
                    moveItem(ids, a, b);
                    ids.forEach((id, n) => {
                      let it = x.products.items.find((y) => y.product_id === id);
                      if (!it) { it = { ...blankProduct(id), published: x.products.auto_publish_new }; x.products.items.push(it); }
                      it.sort = n;
                    });
                  })} />}
                  <ChevronDown className={cn("h-4 w-4 shrink-0 text-muted-foreground transition-transform", open === p.id && "rotate-180")} />
                </div>
                {open === p.id && <ProductEditor c={c} p={p} e={e ?? { ...blankProduct(p.id), published: on }} edit={(fn) => edit(p.id, fn)} categories={data?.categories ?? []} />}
              </li>
            );
          })}
          {!list.length && <li className="py-6 text-center text-sm text-muted-foreground">{t("No products match.")}</li>}
        </ul>
      </Block>
    </div>
  );
}

function ProductEditor({ c, p, e, edit, categories }: { c: SiteConfig; p: CatalogueProduct; e: ProductCfg; edit: (fn: (x: ProductCfg) => void) => void; categories: { id: string; name: string }[] }) {
  const [picker, setPicker] = useState(false);
  const max = c.products.max_photos;
  return (
    <div className="mt-2 space-y-3 rounded-lg border bg-muted/20 p-3">
      <Grid2>
        <Field label="Website name" optional hint={`${t("Default:")} ${p.name}`}><Input value={e.marketing_name} maxLength={90} onChange={(v) => edit((x) => { x.marketing_name = v.target.value; })} /></Field>
        <Field label="Badge"><Choice value={e.badge} onChange={(v) => edit((x) => { x.badge = v; })} options={[["", "None"], ["new", "New"], ["featured", "Featured"], ["offer", "Offer"]]} /></Field>
      </Grid2>
      <Field label="Website description" optional><Textarea value={e.marketing_description} maxLength={2000} onChange={(v) => edit((x) => { x.marketing_description = v.target.value; })} /></Field>
      <Grid2>
        <Field label="Price"><Choice value={e.price} onChange={(v) => edit((x) => { x.price = v; })} options={[["inherit", "Website setting"], ["show", "Always show"], ["hide", "Hide"]]} /></Field>
        <Field label="When the price is hidden"><Select value={e.hidden_action} onChange={(v) => edit((x) => { x.hidden_action = v as ProductCfg["hidden_action"]; })}><option value="">{t("Website default")}</option>{ACTIONS.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}</Select></Field>
        <Field label="Show in category"><Select value={e.category_id ?? ""} onChange={(v) => edit((x) => { x.category_id = v || null; })}><option value="">{p.category_name ? `${t("Its category")} (${p.category_name})` : t("Its category")}</option>{categories.map((k) => <option key={k.id} value={k.id}>{k.name}</option>)}</Select></Field>
        <Field label="Button text" optional hint="Add to cart"><Input value={e.cta_label} maxLength={30} onChange={(v) => edit((x) => { x.cta_label = v.target.value; })} /></Field>
        <Field label="Was price" optional hint="Shown struck through with the saving when higher than the current price">
          <Input type="number" min={0} inputMode="decimal" value={e.compare_at ?? ""} onChange={(v) => edit((x) => { x.compare_at = v.target.value === "" ? null : v.target.value; })} />
        </Field>
      </Grid2>
      {e.price === "hide" && <p className="text-xs text-muted-foreground">{t("Hidden everywhere: website, ordering link, order confirmations and tracking.")}</p>}

      <div className="space-y-2">
        <ToggleRow label="Use the product's own photos" hint={`${t("Up to")} ${max}`} checked={e.use_product_photos} onChange={(v) => edit((x) => { x.use_product_photos = v; })} />
        {e.use_product_photos ? (
          <div className="flex flex-wrap gap-2">
            {p.photos.slice(0, 5).map((ph) => {
              const hidden = e.hidden_photos.includes(ph);
              return (
                <button key={ph} type="button" onClick={() => edit((x) => { x.hidden_photos = hidden ? x.hidden_photos.filter((h) => h !== ph) : [...x.hidden_photos, ph]; })}
                  className={cn("relative h-16 w-16 overflow-hidden rounded-md border", hidden && "opacity-40")} aria-pressed={!hidden} aria-label={hidden ? t("Show photo") : t("Hide photo")}>
                  <img src={`/api/photos/${ph}`} alt="" className="h-full w-full object-cover" />
                  {hidden && <EyeOff className="absolute inset-0 m-auto h-5 w-5" />}
                </button>
              );
            })}
            {!p.photos.length && <p className="text-xs text-muted-foreground">{t("This product has no photos yet — add them in Products or use a website gallery.")}</p>}
          </div>
        ) : (
          <div className="flex flex-wrap gap-2">
            {e.photos.map((m, i) => (
              <div key={m} className="relative h-16 w-16 overflow-hidden rounded-md border">
                <img src={mediaUrl(m, true)} alt="" className="h-full w-full object-cover" />
                <button type="button" className="absolute end-0.5 top-0.5 rounded bg-background/90 p-0.5" aria-label={t("Remove")} onClick={() => edit((x) => { x.photos.splice(i, 1); })}><Trash2 className="h-3 w-3" /></button>
              </div>
            ))}
            {e.photos.length < max && <button type="button" onClick={() => setPicker(true)} className="flex h-16 w-16 items-center justify-center rounded-md border border-dashed" aria-label={t("Add photo")}><ImagePlus className="h-5 w-5 text-muted-foreground" /></button>}
            <MediaPicker open={picker} onOpenChange={setPicker} kind="product" onPick={(id) => { edit((x) => { if (!x.photos.includes(id)) x.photos.push(id); }); setPicker(false); }} />
          </div>
        )}
      </div>
      <Grid2>
        <Field label="Search title" optional><Input value={e.seo_title} maxLength={70} onChange={(v) => edit((x) => { x.seo_title = v.target.value; })} /></Field>
        <Field label="Search description" optional><Input value={e.seo_description} maxLength={160} onChange={(v) => edit((x) => { x.seo_description = v.target.value; })} /></Field>
      </Grid2>
    </div>
  );
}

export function CategoriesTab({ c, set }: TabProps) {
  const { data, isLoading } = useCatalogue();
  if (isLoading) return <Loading />;
  const cats = (data?.categories ?? []).filter((k) => k.is_active);
  const cfg = (id: string) => c.categories.items.find((k) => k.category_id === id);
  const ordered = [...cats].sort((a, b) => (cfg(a.id)?.sort ?? 1e9) - (cfg(b.id)?.sort ?? 1e9) || a.name.localeCompare(b.name));
  const ensure = (x: SiteConfig, id: string) => {
    let it = x.categories.items.find((k) => k.category_id === id);
    if (!it) { it = { category_id: id, visible: true, sort: x.categories.items.length, image: null }; x.categories.items.push(it); }
    return it;
  };
  return (
    <Block title="Categories" hint="Categories come from S'Shop. Only categories with published products appear on the website.">
      <Field label="Home page shows"><Choice value={c.categories.limit} onChange={(v) => set((x) => { x.categories.limit = v; })} options={[[0, "All"], [4, "4"], [6, "6"], [8, "8"], [12, "12"]]} /></Field>
      <ul className="divide-y">
        {ordered.map((k, i) => {
          const it = cfg(k.id);
          return (
            <li key={k.id} className="space-y-2 py-2.5">
              <div className="flex items-center gap-2">
                <Switch checked={it?.visible ?? true} onCheckedChange={(v) => set((x) => { ensure(x, k.id).visible = v; })} aria-label={`${t("Show")} ${k.name}`} />
                <span className="flex-1 truncate text-sm font-medium">{k.name}</span>
                <Move index={i} count={ordered.length} onMove={(a, b) => set((x) => {
                  const ids = ordered.map((r) => r.id);
                  moveItem(ids, a, b);
                  ids.forEach((id, n) => { ensure(x, id).sort = n; });
                })} />
              </div>
              {(it?.visible ?? true) && c.categories.show_images && (
                <div className="ps-11"><MediaField label="Image" kind="other" value={it?.image ?? null} onChange={(id) => set((x) => { ensure(x, k.id).image = id; })} hint="Empty = the first product photo" /></div>
              )}
            </li>
          );
        })}
        {!ordered.length && <li className="py-6 text-center text-sm text-muted-foreground">{t("No categories yet — add them in Products.")}</li>}
      </ul>
    </Block>
  );
}
