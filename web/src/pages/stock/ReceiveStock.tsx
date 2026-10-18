import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ImagePlus, Images, PackagePlus, Printer, RotateCcw, ScanLine, ShieldCheck, Tags, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, money, todayIso, toNum } from "@/lib/format";
import type { Outcome, Paged, Product, Supplier } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { PageHeader, Section } from "@/components/Page";
import { Field, Select, ToggleRow } from "@/components/Form";
import { SearchInput } from "@/components/Filters";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";
import { BarcodeInput } from "@/components/BarcodeInput";
import { PhotoGallery } from "@/components/PhotoGallery";
import { AddPhotosDialog } from "@/components/PhotoPicker";
import { Pill } from "@/components/Badges";
import { LabelsDialog } from "@/components/LabelsDialog";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { useManualBarcodes, type ScanSource } from "@/lib/scan";

interface ProductDetail {
  product: Product;
  photos: { url: string }[];
  stock_by_branch: { branch_id: string; on_hand: number }[];
}

export default function ReceiveStock() {
  const { profile, branch, can } = useSession();
  const [addingPhotos, setAddingPhotos] = useState(false);
  const maxPhotos = profile?.settings.product.max_photos ?? 5;
  const navigate = useNavigate();
  const s = profile!.settings;
  const qc = useQueryClient();
  const [params] = useSearchParams();
  const [productId, setProductId] = useState<string | null>(params.get("product"));
  const [q, setQ] = useState("");
  const term = useDebounced(q);
  const [branchId, setBranchId] = useState(branch?.id ?? "");
  const [qty, setQty] = useState("1");
  const [barcodes, setBarcodes] = useState<string[]>([]);
  // Roadmap 137: how each scanned code arrived (kept in step with `barcodes`).
  const [sources, setSources] = useState<Record<string, ScanSource>>({});
  const typingAllowed = useManualBarcodes("stock");
  const [cost, setCost] = useState("");
  const [price, setPrice] = useState("");
  const [maxDiscount, setMaxDiscount] = useState("");
  const [supplier, setSupplier] = useState("");
  const [reference, setReference] = useState("");
  const [received, setReceived] = useState(todayIso());
  const [activate, setActivate] = useState(true);
  const [opening, setOpening] = useState(false);
  const [scan, setScan] = useState<"find" | "items" | "verify" | null>(null);
  // Roadmap 135: S'Shop generates one seven-digit barcode per unit → print → attach → (verify) → receive.
  const canGenerate = s.stock.generate_item_barcodes && can("barcodes.generate");
  const [mode, setMode] = useState<"generate" | "scan">(canGenerate ? "generate" : "scan");
  const [genQty, setGenQty] = useState("1");
  const [generated, setGenerated] = useState<{ codes: string[]; business: string; product: string } | null>(null);
  const [printing, setPrinting] = useState(false);
  const [attached, setAttached] = useState(false);
  const [verified, setVerified] = useState<string[]>([]);
  const verifiedRef = useRef(verified);
  verifiedRef.current = verified;
  const mustVerify = s.stock.require_label_verification;
  const [gallery, setGallery] = useState(false);

  const results = useQuery({ queryKey: ["products", "receive", term], queryFn: () => api<Paged<Product>>("/products", { query: { q: term, status: "all", limit: 10 } }), enabled: !!term && !productId });
  const detail = useQuery({ queryKey: ["product", productId], queryFn: () => api<ProductDetail>(`/products/${productId}`), enabled: !!productId });
  const suppliers = useQuery({ queryKey: ["suppliers"], queryFn: () => api<Supplier[]>("/suppliers") });
  const p = detail.data?.product;
  const onHand = detail.data?.stock_by_branch.find((b) => b.branch_id === branchId)?.on_hand ?? 0;

  useEffect(() => {
    if (!p) return;
    setPrice(p.marked_price == null ? "" : String(toNum(p.marked_price)));
    setMaxDiscount(p.max_discount === null ? "" : String(toNum(p.max_discount)));
    setCost(p.cost_price === null ? "" : String(toNum(p.cost_price)));
    setSupplier(p.supplier_id ?? "");
    setBarcodes([]);
    setQty("1");
    setGenerated(null);
    setAttached(false);
    setVerified([]);
    setGenQty("1");
  }, [p]);

  const barcodesRef = useRef(barcodes);
  barcodesRef.current = barcodes;
  const qtyLocked = s.stock.quantity_entry === "locked";
  const barcodesOn = s.stock.barcode_requirement !== "disabled";
  const tracked = !!p?.track_items;
  const quantity = tracked ? barcodes.length : Math.max(0, parseInt(qty) || 0);
  const showCost = s.stock.capture_cost && can("sales.view_financials");

  // Scan a product's barcode to pick it; an unknown code can be assigned to a product or start a new one.
  const findByCode = async (code: string): Promise<ScanOutcome> => {
    try {
      const r = await api<{ product: Product }>("/products/lookup", { query: { code } });
      setProductId(r.product.id);
      if (!r.product.track_items && !r.product.barcode) setBarcodes([code]);
      return { tone: "success", title: `${r.product.name} found` };
    } catch (e) {
      if (!(e instanceof ApiError && e.status === 404)) return { tone: "error", title: errorMessage(e) };
      return {
        tone: "error",
        title: "Barcode not found",
        detail: "No product uses this code yet.",
        actions: [
          ...(can("products.edit") ? [{ label: "Assign barcode", onClick: () => navigate(`/products?assign=${encodeURIComponent(code)}`) }] : []),
          ...(can("products.create") ? [{ label: "New product", onClick: () => navigate(`/products/new?barcode=${encodeURIComponent(code)}`) }] : []),
        ],
      };
    }
  };
  // Tracked products: every unit's own label, checked so a code can never be in stock twice.
  const addItem = async (code: string, source?: ScanSource): Promise<ScanOutcome> => {
    if (barcodesRef.current.includes(code)) return { tone: "info", title: "Already scanned in this delivery" };
    const known = await api<{ product: Product; stock_item: { status: string; branch_name: string } | null }>("/products/lookup", { query: { code } }).catch(() => null);
    if (known?.stock_item && ["in_stock", "reserved", "in_transit"].includes(known.stock_item.status)) {
      return { tone: "error", title: "This barcode is already in stock", detail: `${known.product.name} at ${known.stock_item.branch_name}` };
    }
    if (known && !known.stock_item && known.product.barcode === code) {
      return { tone: "error", title: "That is a product barcode", detail: "Scan the label that identifies this individual item." };
    }
    setBarcodes((b) => [...b, code]);
    if (source) setSources((s) => ({ ...s, [code]: source }));
    return { tone: "success", title: `Item ${barcodesRef.current.length + 1} captured` };
  };

  const generate = async () => {
    const n = Math.max(0, parseInt(genQty) || 0);
    const r = await api<{ codes: string[]; label: { business: string; product: string } }>("/barcodes/generate", { body: { product_id: productId, quantity: n } });
    setGenerated({ codes: r.codes, business: r.label.business, product: r.label.product });
    setBarcodes(r.codes);
    setAttached(false);
    setVerified([]);
    toast.success(`${r.codes.length} ${t("barcodes generated — print the labels")}`);
  };
  const startAgain = () => { setGenerated(null); setBarcodes([]); setAttached(false); setVerified([]); };
  // Verification: every scanned label must be one of this delivery's new labels.
  const verifyLabel = async (code: string): Promise<ScanOutcome> => {
    if (!generated?.codes.includes(code)) return { tone: "error", title: "Not one of these labels", detail: "Scan the labels printed for this delivery." };
    if (verifiedRef.current.includes(code)) return { tone: "info", title: "Already verified" };
    setVerified((v) => [...v, code]);
    const done = verifiedRef.current.length + 1;
    return { tone: "success", title: `${done} / ${generated.codes.length} ${t("verified")}` };
  };
  const masked = (c: string) => `****${c.slice(-3)}`;

  const save = useMutation({
    mutationFn: () =>
      api<Outcome<{ on_hand: number }>>("/stock/receive", {
        body: {
          product_id: productId,
          branch_id: branchId,
          quantity,
          barcodes,
          cost_price: showCost && cost !== "" ? Number(cost) : null,
          marked_price: price !== "" ? Number(price) : null,
          max_discount: maxDiscount !== "" ? Number(maxDiscount) : null,
          supplier_id: supplier || null,
          reference,
          date_received: received,
          activate: !!p && !p.is_active && activate,
          kind: opening ? "opening" : "received",
          labels_attached: attached,
          barcode_sources: barcodes.map((c) => sources[c] ?? "scanner"),
          labels_verified: !!generated && verified.length === generated.codes.length,
        },
      }),
    onSuccess: (r) => {
      toast.success(r.pending_approval ? "Stock receipt sent for approval" : `Received ${count(quantity)} × ${p?.name} · now ${count(r.result?.on_hand)} on hand`);
      qc.invalidateQueries({ queryKey: ["stock"] });
      qc.invalidateQueries({ queryKey: ["products"] });
      qc.invalidateQueries({ queryKey: ["product"] });
      qc.invalidateQueries({ queryKey: ["pos-products"] });
      setBarcodes([]);
      setQty("1");
      setReference("");
      setGenerated(null);
      setAttached(false);
      setVerified([]);
    },
    onError: (e) => toast.error(e),
  });

  const valid = !!p && quantity > 0 && (!qtyLocked || quantity === 1) && (s.stock.barcode_requirement !== "required" || tracked || barcodes.length > 0 || !!p.barcode);
  const labelsPending = tracked && !!generated && (!attached ? "Confirm the labels are attached" : mustVerify && verified.length < generated.codes.length ? "Scan each label to verify it" : false);

  return (
    <>
      <PageHeader back="/stock" eyebrow="Stock" title="Receive stock" description="Every receipt is recorded in the inventory ledger." />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_420px]">
        <div className="space-y-5">
          <Section title="Product">
            {!p ? (
              <div className="space-y-2">
                <div className="flex gap-2">
                  <SearchInput value={q} onChange={setQ} placeholder="Search product name or code" className="flex-1" autoFocus />
                  {barcodesOn && <Button variant="ink" onClick={() => setScan("find")}><ScanLine /> {t("Scan")}</Button>}
                </div>
                {barcodesOn && <BarcodeInput onCode={(code) => findByCode(code)} continuousCamera={false} placeholder="Or enter / scan its barcode" />}
                {detail.isLoading && <p className="text-sm text-muted-foreground">{t("Loading…")}</p>}
                <ul className="divide-y rounded-lg border empty:hidden">
                  {results.data?.items.map((r) => (
                    <li key={r.id}>
                      <button className="flex w-full items-center justify-between gap-2 px-3 py-2.5 text-start text-sm hover:bg-accent" onClick={() => setProductId(r.id)}>
                        <span>{r.name} <span className="text-muted-foreground">· {r.code}</span>{!r.is_active && <Pill tone="danger" className="ms-2">{t("Inactive")}</Pill>}</span>
                        <span className="num text-muted-foreground">{count(r.on_hand)}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : (
              <div className="space-y-3">
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <div className="text-lg font-semibold">{p.name}</div>
                    <div className="text-sm text-muted-foreground">{p.code}{p.track_items ? " · tracked per item" : p.barcode ? ` · ${p.barcode}` : ""}</div>
                  </div>
                  <Button variant="ghost" size="sm" onClick={() => { setProductId(null); setQ(""); }}><X /> {t("Change")}</Button>
                </div>
                <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                  <Mini label="On hand" value={count(onHand)} />
                  <Mini label="Value" value={p.cost_price == null && p.marked_price == null ? t("Not set") : money(onHand * toNum(p.cost_price ?? p.marked_price))} />
                  <Mini label="Marked price" value={p.marked_price == null ? t("Not set") : money(p.marked_price)} />
                  <Mini label="Branch" value={profile?.branches.find((b) => b.id === branchId)?.name ?? ""} />
                </div>
                <div className="flex flex-wrap gap-2">
                  {!p.is_active && <Pill tone="danger">{t("Inactive product")}</Pill>}
                  {p.photo_count > 0 && <Button variant="outline" size="sm" onClick={() => setGallery(true)}><Images /> {t("View photos")}</Button>}
                  {(can("products.edit") || can("products.create")) && p.photo_count < maxPhotos && (
                    <Button variant="outline" size="sm" onClick={() => setAddingPhotos(true)}><ImagePlus /> {p.photo_count ? "Add photos" : "Add photos (none yet)"}</Button>
                  )}
                </div>
              </div>
            )}
          </Section>

          {p && (
            <Section title="Quantity & barcodes">
              {tracked && canGenerate && (
                <div className="mb-3 grid grid-cols-2 gap-1 rounded-xl bg-muted p-1" role="tablist">
                  {([["generate", "Generate barcodes"], ["scan", "Scan / enter existing"]] as const).map(([k, l]) => (
                    <button key={k} type="button" role="tab" aria-selected={mode === k} disabled={!!generated || barcodes.length > 0}
                      onClick={() => setMode(k)} className={cn("h-9 rounded-lg text-sm font-medium disabled:opacity-60", mode === k ? "bg-card shadow-sm" : "text-muted-foreground")}>{t(l)}</button>
                  ))}
                </div>
              )}
              {tracked && canGenerate && mode === "generate" ? (
                !generated ? (
                  <div className="flex flex-wrap items-end gap-3">
                    <Field label="Items received" hint="One seven-digit barcode per item">
                      <Input inputMode="numeric" className="num h-12 w-32 text-lg" value={genQty} onChange={(e) => setGenQty(e.target.value.replace(/\D/g, "").slice(0, 3))} />
                    </Field>
                    <ActionButton size="lg" online blockedBy={[!(parseInt(genQty) > 0) && "Enter how many items arrived", parseInt(genQty) > 500 && "Up to 500 at a time"]} onAction={generate}>
                      <Tags /> {t("Generate")} {parseInt(genQty) || 0} {t(parseInt(genQty) === 1 ? "barcode" : "barcodes")}
                    </ActionButton>
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl bg-success/10 p-3">
                      <p className="text-sm font-medium text-success">{generated.codes.length} {t("labels ready to print")}</p>
                      <div className="flex gap-2">
                        {can("barcodes.print") && <Button size="sm" onClick={() => setPrinting(true)}><Printer /> {t("Print labels")}</Button>}
                        <Button size="sm" variant="ghost" onClick={startAgain}><RotateCcw /> {t("Start again")}</Button>
                      </div>
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      {generated.codes.map((c) => (
                        <span key={c} className={cn("num rounded-full px-2.5 py-1 text-xs", verified.includes(c) ? "bg-success/10 text-success" : "bg-muted text-muted-foreground")}>{masked(c)}</span>
                      ))}
                    </div>
                    <ToggleRow label="I have attached a label to each item" hint="Printing a label is not attaching it — confirm each box has its sticker" checked={attached} onChange={setAttached} />
                    {mustVerify && (
                      <Button variant="ink" className="w-full" size="lg" disabled={!attached} onClick={() => setScan("verify")}>
                        <ShieldCheck /> {t("Verify labels")} ({verified.length}/{generated.codes.length})
                      </Button>
                    )}
                  </div>
                )
              ) : tracked ? (
                <div className="space-y-3">
                  <BarcodeInput onCode={(code, source) => addItem(code, source)} placeholder="Enter or scan each item's barcode" cameraTitle={`Scan each ${p.name}`} />
                  <Button className="w-full" variant="ink" size="lg" onClick={() => setScan("items")}><ScanLine /> {t("Scan items (")}{barcodes.length})</Button>
                  <p className="text-xs text-muted-foreground">{t("Each physical item gets its own barcode. Scanned:")} {barcodes.length}.</p>
                  <div className="flex flex-wrap gap-2">
                    {barcodes.map((b) => (
                      <span key={b} className="num inline-flex items-center gap-1 rounded-full bg-success/10 px-2.5 py-1 text-xs text-success">
                        {b}<button onClick={() => setBarcodes(barcodes.filter((x) => x !== b))} aria-label={`Remove ${b}`}><X className="h-3 w-3" /></button>
                      </span>
                    ))}
                  </div>
                </div>
              ) : (
                <div className="grid gap-4 sm:grid-cols-2">
                  <Field label="Quantity" hint={qtyLocked ? "Locked to 1 — capture each item individually" : undefined}>
                    <Input inputMode="numeric" className="num h-12 text-lg" value={qty} disabled={qtyLocked} onChange={(e) => setQty(e.target.value.replace(/\D/g, ""))} />
                  </Field>
                  {barcodesOn && (
                    <Field label="Barcode / item code" optional={s.stock.barcode_requirement !== "required"} hint={p.barcode ? "Scan to confirm the product" : "Saved as the product barcode"}>
                      <div className="flex gap-2">
                        <Input className="num" value={barcodes[0] ?? ""} readOnly={!typingAllowed} placeholder={p.barcode ?? (typingAllowed ? "" : t("Scan barcode"))}
                          onChange={(e) => { setBarcodes(e.target.value ? [e.target.value] : []); setSources(e.target.value ? { [e.target.value]: "manual" } : {}); }} />
                        <Button variant="outline" onClick={() => setScan("items")} aria-label="Scan"><ScanLine /></Button>
                      </div>
                    </Field>
                  )}
                </div>
              )}
            </Section>
          )}
        </div>

        {p && (
          <div className="space-y-5">
            <Section title="Pricing & details">
              <div className="space-y-4">
                {showCost && <Field label="Cost / purchase price" optional><Input inputMode="decimal" className="num" value={cost} onChange={(e) => setCost(e.target.value.replace(/[^\d.]/g, ""))} /></Field>}
                <div className="grid grid-cols-2 gap-3">
                  <Field label="Marked selling price"><Input inputMode="decimal" className="num" value={price} onChange={(e) => setPrice(e.target.value.replace(/[^\d.]/g, ""))} /></Field>
                  <Field label="Max discount" optional><Input inputMode="decimal" className="num" value={maxDiscount} onChange={(e) => setMaxDiscount(e.target.value.replace(/[^\d.]/g, ""))} placeholder="Optional" /></Field>
                </div>
                {(profile?.branches.length ?? 0) > 1 && (
                  <Field label="Branch">
                    <Select value={branchId} onChange={setBranchId}>{profile?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}</Select>
                  </Field>
                )}
                <Field label="Supplier" optional>
                  <Select value={supplier} onChange={setSupplier}>
                    <option value="">—</option>
                    {suppliers.data?.filter((x) => x.is_active).map((x) => <option key={x.id} value={x.id}>{x.name}</option>)}
                  </Select>
                </Field>
                <Field label="Reference / notes" optional><Input value={reference} onChange={(e) => setReference(e.target.value)} placeholder="Delivery note, invoice no." /></Field>
                <Field label="Date received"><Input type="date" value={received} max={todayIso()} onChange={(e) => setReceived(e.target.value)} /></Field>
                <div className="divide-y">
                  {!p.is_active && can("products.deactivate") && <ToggleRow label="Activate product" hint="Make it sellable with this stock" checked={activate} onChange={setActivate} />}
                  <ToggleRow label="Opening stock" hint="First stock load when starting with S'Shop" checked={opening} onChange={setOpening} />
                </div>
              </div>
            </Section>
            <ActionButton size="lg" className="h-14 w-full text-base" online busy={save.isPending} busyLabel="Receiving…"
              blockedBy={[quantity <= 0 && (tracked && mode === "generate" && canGenerate ? "Generate the barcodes first" : "Enter the quantity"), !valid && REASONS.completeFields, labelsPending]} onAction={() => save.mutateAsync()}>
              <PackagePlus /> {t("Receive")} {count(quantity)} item{quantity === 1 ? "" : "s"}
            </ActionButton>
            {save.isSuccess && !save.isPending && <p className="flex items-center justify-center gap-1.5 text-sm text-success"><CheckCircle2 className="h-4 w-4" /> {t("Saved — ready for the next delivery")}</p>}
          </div>
        )}
      </div>
      <BarcodeScanner
        open={scan !== null}
        onOpenChange={(o) => !o && setScan(null)}
        onDetected={(code, source) => (scan === "find" ? findByCode(code) : scan === "verify" ? verifyLabel(code) : tracked ? addItem(code, source) : (setBarcodes([code]), setSources(source ? { [code]: source } : {})))}
        hint={scan === "verify" ? "Scan each label you attached" : scan === "items" && tracked ? "Each item's own label" : undefined}
        continuous={(scan === "items" && tracked) || scan === "verify"}
        title={scan === "verify" ? "Verify labels" : scan === "items" && tracked ? `Scan each ${p?.name}` : "Scan barcode"}
      />
      {printing && generated && (
        <LabelsDialog business={generated.business} labels={generated.codes.map((code) => ({ code, product: generated.product }))}
          note={`Attach one label to each ${generated.product}`} onClose={() => setPrinting(false)} />
      )}
      {p && (
        <AddPhotosDialog
          productId={p.id}
          existing={p.photo_count}
          max={maxPhotos}
          open={addingPhotos}
          onOpenChange={setAddingPhotos}
          onSaved={() => {
            qc.invalidateQueries({ queryKey: ["product", p.id] });
            qc.invalidateQueries({ queryKey: ["products"] });
          }}
        />
      )}
      {p && <PhotoGallery open={gallery} onOpenChange={setGallery} title={p.name} urls={detail.data?.photos.map((x) => x.url) ?? []} />}
    </>
  );
}

function Mini({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl bg-muted/60 p-3">
      <div className="label-caps">{label}</div>
      <div className="num mt-0.5 truncate font-semibold">{value}</div>
    </div>
  );
}
