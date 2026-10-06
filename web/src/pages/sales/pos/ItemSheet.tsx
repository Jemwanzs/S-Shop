import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ChevronDown, Images, Minus, Plus, ScanLine, Store } from "lucide-react";
import { api, photoUrl } from "@/lib/api";
import { toast } from "@/lib/toast";
import { t } from "@/lib/i18n";
import { useSession } from "@/lib/session";
import { count, money, signed, toNum } from "@/lib/format";
import type { PosProduct } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";
import { PhotoGallery } from "@/components/PhotoGallery";
import { StockIndicator } from "@/components/Badges";
import type { CartLine } from "./cart";

interface Availability {
  branch_id: string;
  branch_name: string;
  available: number;
  is_current: boolean;
}

/** Sale item entry: quantity, selling price with live difference, optional discount, barcode clearance. */
export function ItemSheet({
  product,
  editing,
  onClose,
  onSave,
  takenBarcodes,
  initialBarcode,
}: {
  product: PosProduct | null;
  editing?: CartLine | null;
  onClose: () => void;
  onSave: (line: Omit<CartLine, "key"> & { key?: string }) => void;
  takenBarcodes: string[];
  initialBarcode?: string;
}) {
  const { profile, can } = useSession();
  const verify = (code: string) =>
    api<{ ok: boolean }>("/sales/check-barcode", { body: { product_id: product!.id, barcode: code } });
  const s = profile!.settings;
  const markedPrice = toNum(product?.marked_price);
  const [qty, setQty] = useState(1);
  const [price, setPrice] = useState("");
  const [discount, setDiscount] = useState("");
  const [barcode, setBarcode] = useState<string | undefined>();
  const [scan, setScan] = useState(false);
  const [photos, setPhotos] = useState(false);
  const [showOther, setShowOther] = useState(false);

  useEffect(() => {
    if (!product) return;
    setQty(editing?.quantity ?? 1);
    const p = editing?.unitPrice ?? markedPrice;
    setPrice(String(p));
    setDiscount(markedPrice - p > 0 ? String(markedPrice - p) : "");
    setBarcode(editing?.barcode);
    setShowOther(false);
  }, [product, editing, markedPrice]);

  // A barcode scanned on the till before opening this item is checked like any other scan.
  useEffect(() => {
    if (!product || editing || !initialBarcode) return;
    let live = true;
    verify(initialBarcode)
      .then(() => live && setBarcode(initialBarcode))
      .catch((e) => live && toast.error(e));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per opened item
  }, [product?.id, editing, initialBarcode]);

  const availability = useQuery({
    queryKey: ["availability", product?.id],
    queryFn: () => api<Availability[]>(`/stock/availability/${product!.id}`),
    enabled: !!product && showOther,
  });
  const photoList = useQuery({
    queryKey: ["product-photos", product?.id],
    queryFn: () => api<{ photos: { url: string }[] }>(`/products/${product!.id}`),
    enabled: !!product && photos,
  });

  if (!product) return null;
  const qtyLocked = s.sales.quantity_entry === "locked" || product.track_items;
  const unit = toNum(price);
  const diff = unit - markedPrice;
  const unitDisc = markedPrice - unit;
  const overMax = product.max_discount !== null && unitDisc > toNum(product.max_discount);
  const needsBarcode = product.track_items || (s.sales.require_barcode_clearance && !!product.barcode);
  // `barcode` is only set after the server confirmed it belongs to this product, branch and available stock.
  const barcodeOk = !needsBarcode || !!barcode;
  const belowMarkedBlocked = unitDisc > 0 && !can("sales.discount");
  const maxQty = Math.max(product.available, 0);
  const valid = unit >= 0 && price !== "" && qty >= 1 && qty <= maxQty && barcodeOk && !belowMarkedBlocked;

  const onScan = async (code: string): Promise<ScanOutcome | void> => {
    if (product.track_items && takenBarcodes.includes(code) && code !== editing?.barcode) {
      setBarcode(undefined);
      return { tone: "error", title: t("Already in this sale"), detail: t("This item has already been added to the cart.") };
    }
    setBarcode(undefined);
    await verify(code); // throws a titled error (mismatch, wrong branch, sold …) shown inside the scanner
    setBarcode(code);
  };

  return (
    <>
      <ResponsiveDialog
        open={!!product}
        onOpenChange={(o) => !o && onClose()}
        title={product.name}
        description={`${product.code}${product.nickname ? ` · “${product.nickname}”` : ""}`}
        footer={
          <Button
            size="lg"
            className="w-full"
            disabled={!valid}
            onClick={() => onSave({ key: editing?.key, product, quantity: qty, unitPrice: unit, barcode })}
          >
            {editing ? "Update item" : "Add to cart"} · <span className="num">{money(unit * qty)}</span>
          </Button>
        }
      >
        <div className="space-y-5">
          <div className="flex flex-wrap items-center gap-2">
            <StockIndicator available={product.available} threshold={s.stock.low_stock_threshold} />
            {product.reserved > 0 && <span className="text-xs text-muted-foreground">{count(product.reserved)} reserved for orders</span>}
            {product.photo_count > 0 && (
              <Button variant="outline" size="sm" className="ms-auto" onClick={() => setPhotos(true)}>
                <Images /> {t("View photos")}
              </Button>
            )}
          </div>

          <div className="grid grid-cols-2 gap-4">
            <Field label="Quantity" hint={qtyLocked ? (product.track_items ? "One per scanned item" : "Locked to 1") : `Max ${count(maxQty)}`}>
              <div className="flex h-11 items-center rounded-lg border">
                <button type="button" className="tap flex items-center justify-center disabled:opacity-40" disabled={qtyLocked || qty <= 1} onClick={() => setQty(qty - 1)} aria-label="Less">
                  <Minus className="h-4 w-4" />
                </button>
                <input
                  className="num w-full min-w-0 bg-transparent text-center text-base font-semibold outline-none disabled:opacity-60"
                  inputMode="numeric"
                  value={qty}
                  disabled={qtyLocked}
                  onChange={(e) => setQty(Math.max(1, Math.min(maxQty || 1, parseInt(e.target.value) || 1)))}
                />
                <button type="button" className="tap flex items-center justify-center disabled:opacity-40" disabled={qtyLocked || qty >= maxQty} onClick={() => setQty(qty + 1)} aria-label="More">
                  <Plus className="h-4 w-4" />
                </button>
              </div>
            </Field>
            <Field label="Marked price">
              <div className="num flex h-11 items-center rounded-lg bg-muted px-3 font-semibold text-muted-foreground">{money(markedPrice)}</div>
            </Field>
          </div>

          <Field label="Selling price">
            <Input
              inputMode="decimal"
              className="num h-12 text-lg font-semibold"
              value={price}
              onChange={(e) => {
                const v = e.target.value.replace(/[^\d.]/g, "");
                setPrice(v);
                const d = markedPrice - toNum(v);
                setDiscount(d > 0 ? String(Math.round(d * 100) / 100) : "");
              }}
              onFocus={(e) => e.target.select()}
            />
            <span className={cn("num block text-sm font-medium", diff < 0 ? "text-destructive" : diff > 0 ? "text-success" : "text-muted-foreground")}>
              {t("Difference from marked price:")} {signed(diff)}
            </span>
          </Field>

          <Field label="Discount applied" optional hint="Typing a discount sets the selling price — it is never applied twice.">
            <Input
              inputMode="decimal"
              placeholder="Optional"
              className="num"
              value={discount}
              onChange={(e) => {
                const v = e.target.value.replace(/[^\d.]/g, "");
                setDiscount(v);
                setPrice(String(Math.max(0, markedPrice - toNum(v))));
              }}
            />
          </Field>

          {overMax && (
            <p className="flex gap-2 rounded-lg bg-warning/10 p-3 text-sm text-warning">
              <AlertTriangle className="h-4 w-4 shrink-0" /> {t("Above the maximum discount of")} {money(product.max_discount)}. A supervisor will need to approve at checkout.
            </p>
          )}
          {belowMarkedBlocked && <p className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive">{t("You are not allowed to sell below the marked price.")}</p>}

          {needsBarcode && (
            <div className={cn("flex items-center gap-3 rounded-xl border-2 border-dashed p-3", barcodeOk ? "border-success/50 bg-success/5" : "border-primary/40")}>
              <ScanLine className={cn("h-5 w-5 shrink-0", barcodeOk ? "text-success" : "text-primary")} />
              <div className="min-w-0 flex-1 text-sm">
                <div className="font-medium">{barcodeOk ? "Item cleared" : product.track_items ? "Scan this item's barcode" : "Scan to confirm the product"}</div>
                <div className="num truncate text-muted-foreground">{barcode ?? "Required before adding"}</div>
              </div>
              <Button variant={barcodeOk ? "outline" : "default"} size="sm" onClick={() => setScan(true)}>{barcodeOk ? "Rescan" : "Scan"}</Button>
            </div>
          )}

          <div className="rounded-xl border">
            <button type="button" onClick={() => setShowOther(!showOther)} className="flex w-full items-center gap-2 px-3 py-2.5 text-sm font-medium">
              <Store className="h-4 w-4 text-muted-foreground" /> {t("View stock in other branches")}
              <ChevronDown className={cn("ms-auto h-4 w-4 transition", showOther && "rotate-180")} />
            </button>
            {showOther && (
              <ul className="divide-y border-t text-sm">
                {(availability.data ?? []).map((a) => (
                  <li key={a.branch_id} className="flex justify-between px-3 py-2">
                    <span>{a.branch_name}{a.is_current && <span className="text-muted-foreground"> (current)</span>}</span>
                    <span className={cn("num font-medium", a.available <= 0 && "text-destructive")}>{a.available > 0 ? `${count(a.available)} available` : "Out of stock"}</span>
                  </li>
                ))}
                {availability.isLoading && <li className="px-3 py-2 text-muted-foreground">{t("Loading…")}</li>}
                <li className="px-3 py-2 text-xs text-muted-foreground">{t("Stock in another branch is never deducted here — switch branch or request a transfer.")}</li>
              </ul>
            )}
          </div>
        </div>
      </ResponsiveDialog>
      <BarcodeScanner
        open={scan}
        onOpenChange={setScan}
        onDetected={onScan}
        title={`Scan ${product.name}`}
        hint={takenBarcodes.length ? "Items already in the cart are ignored" : undefined}
      />
      <PhotoGallery open={photos} onOpenChange={setPhotos} title={product.name} urls={photoList.data?.photos.map((p) => p.url) ?? (product.primary_photo_id ? [photoUrl(product.primary_photo_id)!] : [])} />
    </>
  );
}
