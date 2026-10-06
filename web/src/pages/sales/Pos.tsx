import { useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Clock, MessageCircle, PackageSearch, Printer, ScanLine, ShoppingCart, Trash2 } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced, useIsDesktop, usePersistentState } from "@/lib/hooks";
import { count, money, toNum } from "@/lib/format";
import type { PosProduct, Product, SaleDetail } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Drawer, DrawerContent, DrawerDescription, DrawerHeader, DrawerTitle } from "@/components/ui/drawer";
import { Chip, SearchInput } from "@/components/Filters";
import { EmptyState, Loading, PageHeader } from "@/components/Page";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { totals, type CartLine } from "./pos/cart";
import { hoursLabel, useOpenNow } from "@/components/Hours";
import { t as translate } from "@/lib/i18n";
import { ItemSheet } from "./pos/ItemSheet";
import { CartLines, Checkout } from "./pos/Checkout";

const newRef = () => crypto.randomUUID();

interface Lookup {
  product: Product;
  stock_item: { barcode: string; branch_name: string; status: string; in_current_branch: boolean } | null;
  other_branches: { branch_id: string; branch_name: string; available: number }[];
}

export default function Pos() {
  const { profile, branch, can } = useSession();
  const openNow = useOpenNow(branch?.hours, profile?.tenant.timezone);
  const blocked = profile?.settings.workspace.outside_hours === "block" && !can("sales.outside_hours");
  const s = profile!.settings;
  const desktop = useIsDesktop();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  const term = useDebounced(q);
  const [category, setCategory] = useState<string | null>(null);
  const [cart, setCart] = usePersistentState<{ lines: CartLine[]; ref: string }>(`sshop.cart.${branch?.id}`, { lines: [], ref: newRef() });
  const [selected, setSelected] = useState<PosProduct | null>(null);
  const [editing, setEditing] = useState<CartLine | null>(null);
  const [prefillBarcode, setPrefillBarcode] = useState<string | undefined>();
  const [scan, setScan] = useState(false);
  const [cartOpen, setCartOpen] = useState(false);
  const [done, setDone] = useState<SaleDetail | null>(null);

  const products = useQuery({
    queryKey: ["pos-products", branch?.id, term],
    queryFn: () => api<PosProduct[]>("/pos/products", { query: { q: term, all: true } }),
    placeholderData: (p) => p,
  });

  const categories = useMemo(() => [...new Set((products.data ?? []).map((p) => p.category_name).filter(Boolean))] as string[], [products.data]);
  const visible = (products.data ?? []).filter((p) => !category || p.category_name === category);
  const t = totals(cart.lines, s);
  const inCart = (id: string) => cart.lines.filter((l) => l.product.id === id).reduce((a, l) => a + l.quantity, 0);
  const takenBarcodes = cart.lines.map((l) => l.barcode).filter(Boolean) as string[];

  const open = (p: PosProduct, barcode?: string) => {
    if (p.available - inCart(p.id) <= 0 && !p.track_items) {
      toast.error(`${p.name} is out of stock at ${branch?.name}`);
      return;
    }
    setPrefillBarcode(barcode);
    setEditing(null);
    setSelected(p);
  };

  // Scans add straight to the cart (the scanner stays open); tap a line to change price or quantity.
  const cartRef = useRef(cart);
  cartRef.current = cart;
  const onScan = async (code: string): Promise<ScanOutcome> => {
    let r: Lookup;
    try {
      r = await api<Lookup>("/products/lookup", { query: { code } });
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) {
        return {
          tone: "error",
          title: "Barcode not found",
          detail: "No product or item in this business uses this code.",
          actions: [
            { label: "Search product", onClick: () => { setScan(false); setQ(""); } },
            ...(can("products.edit") ? [{ label: "Assign barcode", onClick: () => navigate(`/products?assign=${encodeURIComponent(code)}`) }] : []),
          ],
        };
      }
      return { tone: "error", title: errorMessage(e) };
    }
    const p = { ...(r.product as unknown as PosProduct), ...products.data?.find((x) => x.id === r.product.id) };
    const where = r.other_branches.length ? (
      <span>Available at {r.other_branches.map((b) => `${b.branch_name} (${b.available})`).join(" · ")} — change branch or request a transfer.</span>
    ) : "Not available at any other branch either.";
    if (!r.product.is_active) return { tone: "error", title: `${p.name} is not active for sale` };
    const lines = cartRef.current.lines;

    if (p.track_items) {
      const item = r.stock_item;
      if (!item) return { tone: "error", title: `${p.name} is tracked per item`, detail: "Scan the barcode label of the individual item, not the product barcode." };
      if (item.status === "sold") return { tone: "error", title: "This item has already been sold" };
      if (!item.in_current_branch) return { tone: "error", title: `This item is at ${item.branch_name}`, detail: "It cannot be sold from this branch — change branch or transfer it first." };
      if (item.status !== "in_stock") return { tone: "error", title: `This item is ${item.status.replace("_", " ")}` };
      if (lines.some((l) => l.barcode === code)) return { tone: "info", title: `${p.name} is already in the cart` };
      setCart((c) => ({ ...c, lines: [...c.lines, { key: newRef(), product: p, quantity: 1, unitPrice: toNum(p.marked_price), barcode: code }] }));
      return { tone: "success", title: `${p.name} added` };
    }

    const already = lines.filter((l) => l.product.id === p.id).reduce((a, l) => a + l.quantity, 0);
    if (p.available - already <= 0 && !s.stock.allow_negative) {
      return { tone: "error", title: `${p.name} — out of stock at ${branch?.name}`, detail: where };
    }
    // The product barcode clears it at the counter (Require barcode clearance).
    const cleared = p.barcode === code ? code : undefined;
    const same = lines.find((l) => l.product.id === p.id && !l.product.track_items && l.unitPrice === toNum(p.marked_price));
    setCart((c) => ({
      ...c,
      lines: same
        ? c.lines.map((l) => (l.key === same.key ? { ...l, quantity: l.quantity + 1, barcode: l.barcode ?? cleared } : l))
        : [...c.lines, { key: newRef(), product: p, quantity: 1, unitPrice: toNum(p.marked_price), barcode: cleared }],
    }));
    return { tone: "success", title: `${p.name} added · Qty ${already + 1}` };
  };

  const save = (line: Omit<CartLine, "key"> & { key?: string }) => {
    setCart((c) => {
      const rest = c.lines.filter((l) => l.key !== line.key);
      // Merge identical untracked lines at the same price.
      const same = !line.product.track_items && !line.key ? rest.find((l) => l.product.id === line.product.id && l.unitPrice === line.unitPrice) : undefined;
      if (same) return { ...c, lines: rest.map((l) => (l === same ? { ...l, quantity: l.quantity + line.quantity } : l)) };
      return { ...c, lines: [...rest, { ...line, key: line.key || newRef() }] };
    });
    setSelected(null);
    setEditing(null);
    toast.success(`${line.product.name} added`, { duration: 1200 });
  };

  const finished = (sale: SaleDetail) => {
    setCart({ lines: [], ref: newRef() });
    setCartOpen(false);
    setDone(sale);
    qc.invalidateQueries({ queryKey: ["pos-products"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
  };

  const share = async () => {
    if (!done) return;
    try {
      const r = await api<{ sent: boolean; link: string | null }>(`/sales/${done.sale.id}/share`, { method: "POST" });
      if (r.sent) toast.success("Receipt sent on WhatsApp");
      else if (r.link) window.open(r.link, "_blank");
      else toast.info("Add the customer's mobile to share receipts");
    } catch (e) {
      toast.error(e);
    }
  };

  const cartPanel = (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="font-semibold">Cart <span className="num text-muted-foreground">· {count(t.units)}</span></h2>
        {cart.lines.length > 0 && (
          <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => setCart({ lines: [], ref: newRef() })}>
            <Trash2 /> Clear
          </Button>
        )}
      </div>
      {cart.lines.length === 0 ? (
        <EmptyState icon={ShoppingCart} title="Cart is empty" hint="Search or scan products to add them." />
      ) : (
        <>
          <CartLines lines={cart.lines} onEdit={(l) => { setEditing(l); setSelected(l.product); }} onRemove={(key) => setCart((c) => ({ ...c, lines: c.lines.filter((l) => l.key !== key) }))} />
          <Checkout lines={cart.lines} onDone={finished} clientRef={cart.ref} />
        </>
      )}
    </div>
  );

  return (
    <div className="lg:grid lg:grid-cols-[minmax(0,1fr)_400px] lg:gap-6 2xl:grid-cols-[minmax(0,1fr)_460px]">
      <div className="min-w-0">
        <PageHeader
          eyebrow={branch?.name}
          title="Record Sale"
          actions={<Button variant="ink" onClick={() => setScan(true)}><ScanLine /> Scan</Button>}
        />
        {!openNow && branch?.hours && (
          <div className={cn("mb-3 flex items-start gap-2 rounded-xl p-3 text-sm", blocked ? "bg-destructive/10 text-destructive" : "bg-warning/10 text-warning")}>
            <Clock className="mt-0.5 h-4 w-4 shrink-0" />
            <span>
              {blocked ? translate("Closed — sales are blocked outside trading hours.") : translate("Outside trading hours.")} {hoursLabel(branch.hours)}
            </span>
          </div>
        )}
        <div className="sticky top-14 z-20 -mx-3.5 space-y-2 bg-background/90 px-3.5 pb-3 pt-1 backdrop-blur md:-mx-6 md:px-6 lg:static lg:mx-0 lg:bg-transparent lg:px-0 lg:backdrop-blur-none">
          <SearchInput value={q} onChange={setQ} placeholder="Search name, nickname, code or barcode" autoFocus={desktop} />
          {categories.length > 1 && (
            <div className="scrollbar-none -mx-3.5 flex gap-1.5 overflow-x-auto px-3.5 md:mx-0 md:px-0">
              <Chip active={!category} onClick={() => setCategory(null)}>All</Chip>
              {categories.map((c) => <Chip key={c} active={category === c} onClick={() => setCategory(c)}>{c}</Chip>)}
            </div>
          )}
        </div>

        {products.isLoading ? (
          <Loading />
        ) : visible.length === 0 ? (
          <div className="surface"><EmptyState icon={PackageSearch} title={term ? "No matching products" : "No products with stock here"} hint="Receive stock or check another branch." /></div>
        ) : (
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3 3xl:grid-cols-4">
            {visible.map((p) => {
              const left = p.available - inCart(p.id);
              const out = left <= 0;
              return (
                <button
                  key={p.id}
                  onClick={() => open(p)}
                  disabled={out && !p.track_items}
                  className={cn("surface flex items-center gap-3 p-3 text-start transition hover:border-primary/40 hover:shadow-lift active:scale-[0.99] disabled:opacity-50", inCart(p.id) > 0 && "border-primary/50 bg-primary/5")}
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{p.name}</div>
                    <div className="truncate text-xs text-muted-foreground">
                      {p.code}{p.nickname && ` · ${p.nickname}`}{p.track_items && " · per item"}
                    </div>
                  </div>
                  <div className="shrink-0 text-end">
                    <div className="num font-semibold">{money(p.marked_price)}</div>
                    <div className={cn("num text-xs", out ? "text-destructive" : left <= s.stock.low_stock_threshold ? "text-warning" : "text-success")}>
                      {out ? "Out of stock" : `${count(left)} left`}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* Desktop: always-visible cart */}
      <aside className="hidden lg:block">
        <div className="surface card-body sticky top-24 max-h-[calc(100vh-7rem)] overflow-y-auto">{cartPanel}</div>
      </aside>

      {/* Phones & tablets: floating cart bar */}
      {cart.lines.length > 0 && !desktop && (
        <button onClick={() => setCartOpen(true)} className="fixed inset-x-4 bottom-above-nav mb-3 z-30 flex items-center gap-3 rounded-2xl bg-foreground px-4 py-3.5 text-background shadow-lift animate-fade-up md:inset-x-6 lg:hidden">
          <span className="relative">
            <ShoppingCart className="h-5 w-5" />
            <span className="num absolute -end-2.5 -top-2.5 rounded-full bg-primary px-1.5 text-[10px] font-bold text-primary-foreground">{t.units}</span>
          </span>
          <span className="flex-1 text-start font-medium">View cart & checkout</span>
          <span className="num font-semibold">{money(t.net)}</span>
        </button>
      )}
      {desktop && cart.lines.length > 0 && (
        <button onClick={() => setCartOpen(true)} className="fixed bottom-6 end-6 z-30 flex items-center gap-2 rounded-full bg-foreground px-5 py-3 text-background shadow-lift lg:hidden">
          <ShoppingCart className="h-5 w-5" /> <span className="num">{t.units} · {money(t.net)}</span>
        </button>
      )}
      <Drawer open={cartOpen} onOpenChange={setCartOpen}>
        <DrawerContent className="max-h-[94vh]">
          <DrawerHeader className="sr-only"><DrawerTitle>Cart</DrawerTitle><DrawerDescription>Review items and take payment</DrawerDescription></DrawerHeader>
          <div className="overflow-y-auto px-4 pb-8 pt-2">{cartPanel}</div>
        </DrawerContent>
      </Drawer>

      <ItemSheet
        product={selected}
        editing={editing}
        initialBarcode={prefillBarcode}
        onClose={() => { setSelected(null); setEditing(null); setPrefillBarcode(undefined); }}
        onSave={save}
        takenBarcodes={takenBarcodes}
      />
      <BarcodeScanner open={scan} onOpenChange={setScan} onDetected={onScan} title="Scan to sell" continuous />

      <ResponsiveDialog
        open={!!done}
        onOpenChange={(o) => !o && setDone(null)}
        title="Sale complete"
        footer={
          <div className="grid w-full grid-cols-2 gap-2 md:flex md:w-auto">
            <Button variant="outline" onClick={() => navigate(`/sales/${done?.sale.id}`)}><Printer /> Receipt</Button>
            {can("sales.print") && <Button variant="outline" onClick={share}><MessageCircle /> WhatsApp</Button>}
            <Button className="col-span-2" onClick={() => setDone(null)}>New sale</Button>
          </div>
        }
      >
        {done && (
          <div className="space-y-3 py-2 text-center">
            <CheckCircle2 className="mx-auto h-14 w-14 text-success animate-pop" />
            <p className="num text-3xl font-bold">{money(done.sale.total)}</p>
            <p className="text-sm text-muted-foreground">{done.sale.receipt_no} · {done.sale.payment_method === "credit" ? "on credit" : `paid by ${done.sale.payment_method}`}</p>
            {done.sale.customer && <p className="text-sm">{done.sale.customer.name}</p>}
            {done.sale.points_earned > 0 && <p className="inline-block rounded-full bg-points/15 px-4 py-1.5 font-semibold text-points">🌼 +{done.sale.points_earned} Loyalty Points</p>}
          </div>
        )}
      </ResponsiveDialog>
    </div>
  );
}
