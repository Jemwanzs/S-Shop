import { useEffect, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, ScanLine, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { PhotoPicker, photosValid, uploadPhotos, usePendingPhotos } from "@/components/PhotoPicker";
import type { Category, Outcome, Product, Supplier } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { ActionButton, REASONS, isDirty } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Checkbox } from "@/components/ui/checkbox";
import { Loading, PageHeader, Section } from "@/components/Page";
import { Field, Select, ToggleRow } from "@/components/Form";
import { BarcodeScanner } from "@/components/BarcodeScanner";
import { CustomFieldInputs } from "@/components/CustomFields";
import { t } from "@/lib/i18n";

interface FormState {
  code: string;
  name: string;
  nickname: string;
  description: string;
  category_id: string;
  supplier_id: string;
  marked_price: string;
  max_discount: string;
  cost_price: string;
  barcode: string;
  track_items: boolean;
  is_active: boolean;
  available_for_orders: boolean;
  transfer_allowed: boolean;
  loyalty_eligible: boolean;
  loyalty_threshold: string;
  loyalty_points_per: string;
  low_stock_threshold: string;
  all_branches: boolean;
  branch_ids: string[];
  custom_fields: Record<string, unknown>;
}

const EMPTY: FormState = {
  code: "", name: "", nickname: "", description: "", category_id: "", supplier_id: "", marked_price: "", max_discount: "",
  cost_price: "", barcode: "", track_items: false, is_active: true, available_for_orders: true, transfer_allowed: true,
  loyalty_eligible: true, loyalty_threshold: "", loyalty_points_per: "", low_stock_threshold: "", all_branches: true, branch_ids: [],
  custom_fields: {},
};

const str = (v: unknown) => (v === null || v === undefined ? "" : String(v));
const numOrNull = (v: string) => (v.trim() === "" ? null : Number(v));

export default function ProductForm() {
  const { id } = useParams();
  const editing = !!id;
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { profile, can } = useSession();
  // ?barcode= comes from a scanner's "Assign barcode" (unknown code scanned at the counter or when receiving).
  const [params] = useSearchParams();
  const assignBarcode = params.get("barcode")?.trim() || null;
  const [f, setF] = useState<FormState>(() => (assignBarcode ? { ...EMPTY, barcode: assignBarcode } : EMPTY));
  // The product as loaded, to tell whether an edit has anything to save.
  const [loaded, setLoaded] = useState<FormState | null>(null);
  const [scan, setScan] = useState(false);
  const photos = usePendingPhotos();
  const [uploaded, setUploaded] = useState<number | null>(null);
  const [newCategory, setNewCategory] = useState<string | null>(null);
  const [newSupplier, setNewSupplier] = useState<string | null>(null);
  const set = <K extends keyof FormState>(k: K, v: FormState[K]) => setF((s) => ({ ...s, [k]: v }));

  const existing = useQuery({ queryKey: ["product", id], queryFn: () => api<{ product: Product; branch_ids: string[] }>(`/products/${id}`), enabled: editing });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const suppliers = useQuery({ queryKey: ["suppliers"], queryFn: () => api<Supplier[]>("/suppliers") });
  const barcodesDisabled = profile?.settings.stock.barcode_requirement === "disabled";
  const maxPhotos = profile?.settings.product.max_photos ?? 5;

  useEffect(() => {
    const p = existing.data?.product;
    if (!p) return;
    const next: FormState = {
      code: p.code, name: p.name, nickname: p.nickname, description: p.description, category_id: str(p.category_id), supplier_id: str(p.supplier_id),
      marked_price: str(p.marked_price), max_discount: str(p.max_discount), cost_price: str(p.cost_price), barcode: str(p.barcode),
      track_items: p.track_items, is_active: p.is_active, available_for_orders: p.available_for_orders, transfer_allowed: p.transfer_allowed,
      loyalty_eligible: p.loyalty_eligible, loyalty_threshold: str(p.loyalty_threshold), loyalty_points_per: str(p.loyalty_points_per),
      low_stock_threshold: str(p.low_stock_threshold), all_branches: p.all_branches, branch_ids: existing.data!.branch_ids,
      custom_fields: p.custom_fields ?? {},
      ...(assignBarcode && !p.track_items ? { barcode: assignBarcode } : {}),
    };
    setF(next);
    setLoaded(next);
  }, [existing.data, assignBarcode]);

  const quickAdd = async (kind: "categories" | "suppliers", name: string) => {
    try {
      const r = await api<{ id: string }>(`/${kind}`, { body: { name } });
      await qc.invalidateQueries({ queryKey: [kind] });
      set(kind === "categories" ? "category_id" : "supplier_id", r.id);
      if (kind === "categories") setNewCategory(null);
      else setNewSupplier(null);
    } catch (e) {
      toast.error(e);
    }
  };

  const save = useMutation({
    mutationFn: async () => {
      const body = {
        ...f,
        code: f.code.trim() || null,
        category_id: f.category_id || null,
        supplier_id: f.supplier_id || null,
        marked_price: Number(f.marked_price || 0),
        max_discount: numOrNull(f.max_discount),
        cost_price: numOrNull(f.cost_price),
        barcode: f.barcode.trim() || null,
        loyalty_threshold: numOrNull(f.loyalty_threshold),
        loyalty_points_per: numOrNull(f.loyalty_points_per),
        low_stock_threshold: numOrNull(f.low_stock_threshold),
      };
      const r = await api<Outcome<{ id: string }>>(editing ? `/products/${id}` : "/products", { method: editing ? "PUT" : "POST", body });
      const productId = editing ? id! : (r.result?.id ?? null);
      // New products: the product is saved first; each photo then succeeds or fails on its own, so a photo
      // problem never makes a saved product look failed (and the form is left, so it is never created twice).
      let upload = null;
      if (!editing && productId && photos.items.length) {
        setUploaded(0);
        upload = await uploadPhotos(productId, photos.items, setUploaded);
      }
      return { r, productId, upload };
    },
    onSuccess: ({ r, productId, upload }) => {
      qc.invalidateQueries({ queryKey: ["products"] });
      qc.invalidateQueries({ queryKey: ["product", id] });
      qc.invalidateQueries({ queryKey: ["product", productId] });
      setUploaded(null);
      if (upload?.failed.length) {
        toast.error(`${editing ? "Product updated" : "Product created"} — ${upload.failed.length} photo(s) not saved`, {
          description: upload.failed.map((f) => `${f.name}: ${f.reason}`).join(" · ") + ` · ${upload.saved} saved. Add the rest from the product page.`,
        });
      } else if (r.pending_approval) {
        toast.success("Submitted for approval");
      } else {
        toast.success(editing ? "Product updated" : upload?.saved ? `Product created with ${upload.saved} photo(s)` : "Product created");
      }
      navigate(r.pending_approval ? "/products" : `/products/${productId}`);
    },
    onError: (e) => toast.error(e),
  });

  if (editing && existing.isLoading) return <Loading />;
  const valid = f.name.trim() && f.marked_price !== "" && (f.all_branches || f.branch_ids.length > 0);

  return (
    <>
      <PageHeader back={editing ? `/products/${id}` : "/products"} eyebrow="Catalogue" title={editing ? `Edit ${f.name}` : "New product"} />
      <form
        className="grid gap-5 pb-24 xl:grid-cols-[minmax(0,1fr)_400px]"
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) save.mutate();
        }}
      >
        <div className="space-y-5">
          <Section title="Basics">
            <div className="grid gap-4 md:grid-cols-2">
              <Field label="Product name" className="md:col-span-2"><Input value={f.name} onChange={(e) => set("name", e.target.value)} required autoFocus={!editing} /></Field>
              <Field label="Nickname / short name" optional><Input value={f.nickname} onChange={(e) => set("nickname", e.target.value)} /></Field>
              <Field label="Product code" hint={editing ? undefined : "Leave blank to auto-generate"}><Input value={f.code} onChange={(e) => set("code", e.target.value.toUpperCase())} className="num uppercase" /></Field>
              <Field label="Category" optional>
                {newCategory === null ? (
                  <div className="flex gap-2">
                    <Select value={f.category_id} onChange={(v) => set("category_id", v)}>
                      <option value="">{t("No category")}</option>
                      {categories.data?.filter((c) => c.is_active || c.id === f.category_id).map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                    </Select>
                    <Button type="button" variant="outline" size="icon" onClick={() => setNewCategory("")} aria-label="New category"><Plus /></Button>
                  </div>
                ) : (
                  <div className="flex gap-2">
                    <Input value={newCategory} onChange={(e) => setNewCategory(e.target.value)} placeholder="New category name" autoFocus />
                    <Button type="button" onClick={() => newCategory.trim() && quickAdd("categories", newCategory.trim())}>{t("Add")}</Button>
                    <Button type="button" variant="ghost" size="icon" onClick={() => setNewCategory(null)} aria-label="Cancel"><X /></Button>
                  </div>
                )}
              </Field>
              <Field label="Supplier" optional>
                {newSupplier === null ? (
                  <div className="flex gap-2">
                    <Select value={f.supplier_id} onChange={(v) => set("supplier_id", v)}>
                      <option value="">{t("No supplier")}</option>
                      {suppliers.data?.filter((s) => s.is_active || s.id === f.supplier_id).map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
                    </Select>
                    <Button type="button" variant="outline" size="icon" onClick={() => setNewSupplier("")} aria-label="New supplier"><Plus /></Button>
                  </div>
                ) : (
                  <div className="flex gap-2">
                    <Input value={newSupplier} onChange={(e) => setNewSupplier(e.target.value)} placeholder="Supplier name" autoFocus />
                    <Button type="button" onClick={() => newSupplier.trim() && quickAdd("suppliers", newSupplier.trim())}>{t("Add")}</Button>
                    <Button type="button" variant="ghost" size="icon" onClick={() => setNewSupplier(null)} aria-label="Cancel"><X /></Button>
                  </div>
                )}
              </Field>
              <CustomFieldInputs kind="product" values={f.custom_fields} onChange={(v) => set("custom_fields", v)} />
              <Field label="Description" optional className="md:col-span-2"><Textarea value={f.description} onChange={(e) => set("description", e.target.value)} rows={3} placeholder="Shown to customers on the ordering link" /></Field>
            </div>
          </Section>

          <Section title="Pricing">
            <div className="grid gap-4 sm:grid-cols-3">
              <Field label="Marked selling price"><Input inputMode="decimal" className="num" value={f.marked_price} onChange={(e) => set("marked_price", e.target.value.replace(/[^\d.]/g, ""))} required /></Field>
              <Field label="Maximum discount" optional hint="Above this needs supervisor approval"><Input inputMode="decimal" className="num" value={f.max_discount} onChange={(e) => set("max_discount", e.target.value.replace(/[^\d.]/g, ""))} placeholder="Optional" /></Field>
              {(profile?.settings.stock.capture_cost ?? true) && can("sales.view_financials") && (
                <Field label="Cost price" optional hint="Used for profit reports"><Input inputMode="decimal" className="num" value={f.cost_price} onChange={(e) => set("cost_price", e.target.value.replace(/[^\d.]/g, ""))} placeholder="Optional" /></Field>
              )}
            </div>
          </Section>

          {!barcodesDisabled && (
            <Section title="Barcode">
              {assignBarcode && (
                <p className="mb-2 rounded-lg bg-primary/10 p-2.5 text-xs">
                  {f.track_items
                    ? <>{t("This product is tracked per item, so")} <span className="num font-semibold">{assignBarcode}</span> should be captured as an item barcode when receiving stock.</>
                    : <>{t("Assigning scanned barcode")} <span className="num font-semibold">{assignBarcode}</span>{existing.data?.product.barcode && existing.data.product.barcode !== assignBarcode ? <> — it replaces <span className="num">{existing.data.product.barcode}</span></> : null}. Save to apply.</>}
                </p>
              )}
              <ToggleRow
                label="Track each item individually"
                hint="Every physical unit gets its own barcode, scanned when received and when sold."
                checked={f.track_items}
                onChange={(v) => set("track_items", v)}
              />
              {!f.track_items && (
                <Field label="Product barcode" optional hint="Manufacturer barcode shared by all units">
                  <div className="flex gap-2">
                    <Input className="num" value={f.barcode} onChange={(e) => set("barcode", e.target.value)} />
                    <Button type="button" variant="outline" onClick={() => setScan(true)}><ScanLine /> {t("Scan")}</Button>
                  </div>
                </Field>
              )}
            </Section>
          )}

          {!editing && (
            <Section title={`Photos · up to ${maxPhotos}`}>
              <PhotoPicker photos={photos} room={maxPhotos} max={maxPhotos} disabled={save.isPending} />
            </Section>
          )}
        </div>

        <div className="space-y-5">
          <Section title="Availability">
            <div className="divide-y">
              {!editing && <ToggleRow label="Active for sale" checked={f.is_active} onChange={(v) => set("is_active", v)} />}
              <ToggleRow label="Available on ordering link" hint="Customers can order it online" checked={f.available_for_orders} onChange={(v) => set("available_for_orders", v)} />
              <ToggleRow label="Transfers allowed" checked={f.transfer_allowed} onChange={(v) => set("transfer_allowed", v)} />
              <ToggleRow label="All branches" hint="Turn off to restrict to selected branches" checked={f.all_branches} onChange={(v) => set("all_branches", v)} />
            </div>
            {!f.all_branches && (
              <div className="mt-2 space-y-2 rounded-xl bg-muted/50 p-3">
                {profile?.branches.map((b) => (
                  <label key={b.id} className="flex items-center gap-2.5 text-sm">
                    <Checkbox checked={f.branch_ids.includes(b.id)} onCheckedChange={(c) => set("branch_ids", c ? [...f.branch_ids, b.id] : f.branch_ids.filter((x) => x !== b.id))} />
                    {b.name}
                  </label>
                ))}
              </div>
            )}
            <Field label="Low-stock alert at" optional className="mt-3" hint={`Default ${profile?.settings.stock.low_stock_threshold ?? 3}`}>
              <Input inputMode="numeric" className="num" value={f.low_stock_threshold} onChange={(e) => set("low_stock_threshold", e.target.value.replace(/\D/g, ""))} />
            </Field>
          </Section>

          <Section title="Loyalty">
            <ToggleRow label="Earns loyalty points" checked={f.loyalty_eligible} onChange={(v) => set("loyalty_eligible", v)} />
            {f.loyalty_eligible && (
              <div className="grid grid-cols-2 gap-3">
                <Field label="Every (spend)" optional hint={`Default ${profile?.settings.loyalty.threshold}`}><Input inputMode="decimal" className="num" value={f.loyalty_threshold} onChange={(e) => set("loyalty_threshold", e.target.value.replace(/[^\d.]/g, ""))} /></Field>
                <Field label="Earns (points)" optional hint={`Default ${profile?.settings.loyalty.points_per}`}><Input inputMode="numeric" className="num" value={f.loyalty_points_per} onChange={(e) => set("loyalty_points_per", e.target.value.replace(/\D/g, ""))} /></Field>
              </div>
            )}
          </Section>
        </div>

        <div className="fixed inset-x-0 bottom-above-nav z-20 border-t bg-background/95 p-3 backdrop-blur lg:bottom-0 lg:start-sidebar">
          <div className="mx-auto flex max-w-[1680px] justify-end gap-2 px-1 md:px-3 lg:px-5">
            <Button type="button" variant="outline" onClick={() => navigate(-1)}>{t("Cancel")}</Button>
            <ActionButton type="submit" className="min-w-32" online busy={save.isPending}
              busyLabel={uploaded !== null ? `${t("Uploading…")} ${uploaded}/${photos.items.length}` : "Saving…"}
              blockedBy={[
                !valid && REASONS.completeFields,
                editing && loaded && !isDirty(loaded, f) && REASONS.nothingToSave,
                !editing && !photosValid(photos.items, maxPhotos) && "Check the photos",
              ]}>
              {editing ? "Save changes" : "Create product"}
            </ActionButton>
          </div>
        </div>
      </form>
      <BarcodeScanner open={scan} onOpenChange={setScan} onDetected={(c) => set("barcode", c)} />
    </>
  );
}
