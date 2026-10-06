import { useEffect, useMemo, useState } from "react";
import { Link, Navigate, Route, Routes, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, CheckCircle2, ChevronRight, ClipboardList, ImageOff, Loader2, Minus, Plus, ShoppingBag, ShoppingCart, Trash2 } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, photoUrl } from "@/lib/api";
import { usePersistentState } from "@/lib/hooks";
import { amount, count, date, phone, toNum } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ErrorState, Loading } from "@/components/Page";
import { Chip, SearchInput } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";
import { ThemeToggle } from "@/components/layout/AppShell";
import { PortalHeader, PoweredBy, Steps, type Step } from "./shared";
import { t } from "@/lib/i18n";

interface Business { name: string; slug: string; tagline: string; phone: string; currency: string; logo_url: string | null; otp_required: boolean; show_loyalty: boolean;
  /** Settings → Orders & ordering link → Show product prices (prices are not even sent when off). */
  show_prices: boolean }
interface PortalSession { token: string; customer: { first_name: string; nickname: string; mobile: string } }
interface Me { customer: PortalSession["customer"]; total_orders: number; loyalty: { points: number; value: string | null } | null }
interface Item { id: string; name: string; description: string; category_id: string | null; price: string; available: number; primary_photo_id: string | null }
interface PortalOrder { id: string; order_no: string; status: string; status_label: string; total: string; created_at: string; track_token: string; items: { name: string; quantity: number; line_total: string }[]; steps: Step[] }
type Cart = Record<string, { item: Item; qty: number }>;
interface Ident { mobile: string; exists: boolean; first_name: string | null; otp_required: boolean }

export default function Portal() {
  const { slug = "" } = useParams();
  const [sess, setSess] = usePersistentState<PortalSession | null>(`sshop.portal.${slug}`, null);
  const biz = useQuery({ queryKey: ["portal", slug], queryFn: () => api<Business>(`/portal/${slug}`, { token: null }), retry: false });

  if (biz.isLoading) return <Loading className="min-h-screen" />;
  if (biz.error || !biz.data) return <div className="min-h-screen"><ErrorState error={biz.error ?? new Error("Shop not found")} /></div>;
  const b = biz.data;

  return (
    <div className="min-h-screen bg-background">
      <div className="absolute end-3 top-3"><ThemeToggle /></div>
      <Routes>
        <Route index element={<Entry b={b} sess={sess} setSess={setSess} />} />
        <Route path="shop" element={sess ? <Shop b={b} sess={sess} /> : <Navigate to={`/order/${slug}`} replace />} />
        <Route path="orders" element={sess ? <MyOrders b={b} sess={sess} /> : <Navigate to={`/order/${slug}`} replace />} />
        <Route path="*" element={<Navigate to={`/order/${slug}`} replace />} />
      </Routes>
    </div>
  );
}

function Narrow({ children }: { children: React.ReactNode }) {
  return <div className="mx-auto max-w-lg">{children}</div>;
}

/** Welcome → mobile number → (code) → name for new customers → home. */
function Entry({ b, sess, setSess }: { b: Business; sess: PortalSession | null; setSess: (s: PortalSession | null) => void }) {
  const [mobile, setMobile] = useState("");
  const [step, setStep] = useState<"mobile" | "details">("mobile");
  const [ident, setIdent] = useState<Ident | null>(null);
  const [code, setCode] = useState("");
  const [firstName, setFirstName] = useState("");
  const [nickname, setNickname] = useState("");
  const me = useQuery({ queryKey: ["portal", b.slug, "me", sess?.token], queryFn: () => api<Me>(`/portal/${b.slug}/me`, { token: sess!.token }), enabled: !!sess, retry: false });

  const identify = useMutation({
    mutationFn: () => api<Ident>(`/portal/${b.slug}/identify`, { body: { mobile }, token: null }),
    onSuccess: (r) => {
      setIdent(r);
      if (!r.exists || r.otp_required) setStep("details");
      else start.mutate({ mobile: r.mobile });
    },
    onError: (e) => toast.error(e),
  });
  const start = useMutation({
    mutationFn: (body: { mobile: string; code?: string; first_name?: string; nickname?: string }) => api<PortalSession>(`/portal/${b.slug}/session`, { body, token: null }),
    onSuccess: (s) => {
      setSess(s);
      setStep("mobile");
      setMobile("");
    },
    onError: (e) => toast.error(e),
  });

  // An expired or foreign session token: start again with the mobile number.
  useEffect(() => {
    if (sess && me.error) setSess(null);
  }, [sess, me.error, setSess]);

  return (
    <Narrow>
      <PortalHeader name={b.name} tagline={b.tagline} logo={b.logo_url} />
      <main className="flex min-h-[50vh] flex-col justify-end px-5 py-10">
        {sess && me.data ? (
          <div className="space-y-4 animate-fade-up">
            <h2 className="text-2xl font-semibold">{t("Welcome back,")} {me.data.customer.nickname || me.data.customer.first_name} 👋</h2>
            {me.data.loyalty && (
              <div className="rounded-2xl bg-gradient-to-br from-primary to-primary/70 p-5 text-primary-foreground shadow-lift">
                <p className="text-sm opacity-90">{t("Loyalty points")}</p>
                <p className="num mt-1 text-3xl font-bold">{count(me.data.loyalty.points)}</p>
                {me.data.loyalty.value !== null && <p className="num mt-1 text-sm opacity-90">{t("Value:")} {b.currency} {amount(me.data.loyalty.value)}</p>}
              </div>
            )}
            <HomeCard icon={ClipboardList} title="My Orders" subtitle={me.data.total_orders ? `You've ordered ${me.data.total_orders} time${me.data.total_orders === 1 ? "" : "s"} with us` : "No orders yet"} to={`/order/${b.slug}/orders`} />
            <HomeCard icon={ShoppingBag} title="Order Now" subtitle="Browse products and place a new order" to={`/order/${b.slug}/shop`} />
            <button className="w-full py-3 text-center text-muted-foreground hover:text-foreground" onClick={() => setSess(null)}>{t("Not you? Enter a different number")}</button>
          </div>
        ) : sess ? (
          <Loading />
        ) : step === "mobile" ? (
          <form className="space-y-4 animate-fade-up" onSubmit={(e) => { e.preventDefault(); identify.mutate(); }}>
            <div>
              <h2 className="text-2xl font-semibold">{t("Welcome")}</h2>
              <p className="mt-2 text-muted-foreground">{t("Enter the mobile number you used to order with us before (or will use this time).")}</p>
            </div>
            <Field label="Mobile Number">
              <Input inputMode="tel" autoComplete="tel" className="num h-14 rounded-xl text-lg" placeholder="07XXXXXXXX" value={mobile} onChange={(e) => setMobile(e.target.value)} autoFocus />
            </Field>
            <Button type="submit" variant="ink" size="lg" className="h-14 w-full rounded-xl text-base" disabled={mobile.replace(/\D/g, "").length < 9 || identify.isPending || start.isPending}>
              {identify.isPending || start.isPending ? <Loader2 className="animate-spin" /> : "Continue"}
            </Button>
          </form>
        ) : (
          <form
            className="space-y-4 animate-fade-up"
            onSubmit={(e) => {
              e.preventDefault();
              start.mutate({ mobile: ident!.mobile, code: code || undefined, first_name: firstName, nickname });
            }}
          >
            <button type="button" onClick={() => setStep("mobile")} className="inline-flex items-center gap-1 text-sm text-muted-foreground"><ArrowLeft className="h-4 w-4" /> {phone(ident?.mobile)}</button>
            <h2 className="text-2xl font-semibold">{ident?.exists ? "Welcome back 👋" : "Nice to meet you 👋"}</h2>
            {ident?.otp_required && (
              <Field label="Verification code" hint="We sent a 6-digit code to your WhatsApp">
                <Input inputMode="numeric" autoComplete="one-time-code" className="num h-14 rounded-xl text-center text-2xl tracking-[0.5em]" maxLength={6} value={code} onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))} autoFocus />
              </Field>
            )}
            {!ident?.exists && (
              <>
                <Field label="First name"><Input className="h-12 rounded-xl" value={firstName} onChange={(e) => setFirstName(e.target.value)} autoFocus={!ident?.otp_required} /></Field>
                <Field label="Nickname" optional><Input className="h-12 rounded-xl" value={nickname} onChange={(e) => setNickname(e.target.value)} placeholder="Optional" /></Field>
              </>
            )}
            <Button type="submit" variant="ink" size="lg" className="h-14 w-full rounded-xl text-base" disabled={start.isPending || (ident?.otp_required && code.length !== 6) || (!ident?.exists && !firstName.trim())}>
              {start.isPending ? <Loader2 className="animate-spin" /> : "Continue"}
            </Button>
          </form>
        )}
      </main>
      <PoweredBy />
    </Narrow>
  );
}

function HomeCard({ icon: Icon, title, subtitle, to }: { icon: typeof ClipboardList; title: string; subtitle: string; to: string }) {
  return (
    <Link to={to} className="flex items-center gap-5 rounded-2xl border bg-card/60 px-6 py-6 transition hover:border-primary/40 hover:shadow-lift active:scale-[0.99]">
      <Icon className="h-7 w-7 shrink-0 text-muted-foreground" strokeWidth={1.6} />
      <span className="min-w-0 flex-1">
        <span className="block text-xl font-medium">{title}</span>
        <span className="block text-muted-foreground">{subtitle}</span>
      </span>
      <ChevronRight className="h-5 w-5 text-muted-foreground" />
    </Link>
  );
}

function Shop({ b, sess }: { b: Business; sess: PortalSession }) {
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const [cart, setCart] = usePersistentState<Cart>(`sshop.portal.cart.${b.slug}`, {});
  const [open, setOpen] = useState<Item | null>(null);
  const [cartOpen, setCartOpen] = useState(false);
  const [location, setLocation] = useState("");
  const [notes, setNotes] = useState("");
  const [placed, setPlaced] = useState<{ order_no: string; track_token: string } | null>(null);
  const cat = useQuery({ queryKey: ["portal", b.slug, "catalogue"], queryFn: () => api<{ products: Item[]; categories: { id: string; name: string }[] }>(`/portal/${b.slug}/catalogue`, { token: null }) });
  const items = useMemo(
    () => (cat.data?.products ?? []).filter((p) => (!category || p.category_id === category) && (!q || p.name.toLowerCase().includes(q.toLowerCase()))),
    [cat.data, category, q],
  );
  const lines = Object.values(cart);
  const units = lines.reduce((a, l) => a + l.qty, 0);
  const total = lines.reduce((a, l) => a + toNum(l.item.price) * l.qty, 0);
  const setQty = (item: Item, qty: number) =>
    setCart((c) => {
      const next = { ...c };
      if (qty <= 0) delete next[item.id];
      else next[item.id] = { item, qty: Math.min(qty, item.available) };
      return next;
    });

  const submit = useMutation({
    mutationFn: () => api<{ order_no: string; track_token: string }>(`/portal/${b.slug}/orders`, { token: sess.token, body: { items: lines.map((l) => ({ product_id: l.item.id, quantity: l.qty })), delivery_location: location, notes } }),
    onSuccess: (r) => {
      setPlaced(r);
      setCart({});
      setCartOpen(false);
      qc.invalidateQueries({ queryKey: ["portal", b.slug] });
    },
    onError: (e) => toast.error(e),
  });

  if (placed) {
    return (
      <Narrow>
        <PortalHeader name={b.name} logo={b.logo_url} />
        <div className="space-y-4 px-5 py-12 text-center animate-fade-up">
          <CheckCircle2 className="mx-auto h-16 w-16 text-success animate-pop" />
          <h2 className="text-2xl font-semibold">{t("Order submitted successfully.")}</h2>
          <p className="text-muted-foreground">{t("We are now processing your order.")}</p>
          <p className="num text-lg">{placed.order_no}</p>
          <Button asChild variant="ink" size="lg" className="h-14 w-full rounded-xl"><Link to={`/track/${placed.track_token}`}>{t("Track My Order")}</Link></Button>
          <Button asChild variant="ghost" className="w-full"><Link to={`/order/${b.slug}`}>{t("Back to home")}</Link></Button>
        </div>
      </Narrow>
    );
  }

  return (
    <div className="pb-28">
      <header className="sticky top-0 z-20 border-b bg-background/90 backdrop-blur">
        <div className="mx-auto flex max-w-6xl items-center gap-3 px-4 py-3">
          <Link to={`/order/${b.slug}`} className="rounded-full p-2 hover:bg-muted" aria-label="Back"><ArrowLeft className="h-5 w-5" /></Link>
          {b.logo_url && <img src={b.logo_url} alt="" className="h-9 w-9 rounded-lg object-cover" />}
          <span className="truncate font-semibold">{b.name}</span>
        </div>
        <div className="mx-auto max-w-6xl space-y-2 px-4 pb-3">
          <SearchInput value={q} onChange={setQ} placeholder="Search products" />
          {(cat.data?.categories.length ?? 0) > 0 && (
            <div className="scrollbar-none -mx-4 flex gap-1.5 overflow-x-auto px-4">
              <Chip active={!category} onClick={() => setCategory(null)}>{t("All")}</Chip>
              {cat.data!.categories.map((c) => <Chip key={c.id} active={category === c.id} onClick={() => setCategory(c.id)}>{c.name}</Chip>)}
            </div>
          )}
        </div>
      </header>
      <main className="mx-auto max-w-6xl px-4 pt-4">
        {cat.isLoading ? <Loading /> : items.length === 0 ? (
          <p className="py-16 text-center text-muted-foreground">{t("No products found.")}</p>
        ) : (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
            {items.map((p) => {
              const inCart = cart[p.id]?.qty ?? 0;
              const out = p.available <= 0;
              return (
                <button key={p.id} onClick={() => setOpen(p)} className={cn("surface group overflow-hidden text-start transition hover:shadow-lift", out && "opacity-60")}>
                  <div className="relative aspect-square bg-muted">
                    {p.primary_photo_id ? <img src={photoUrl(p.primary_photo_id)} alt={p.name} loading="lazy" className="h-full w-full object-cover transition group-hover:scale-105" /> : <ImageOff className="absolute inset-0 m-auto h-8 w-8 text-muted-foreground" />}
                    {inCart > 0 && <span className="num absolute end-2 top-2 rounded-full bg-primary px-2 py-0.5 text-xs font-bold text-primary-foreground">{inCart}</span>}
                  </div>
                  <div className="space-y-1 p-3">
                    <p className="line-clamp-2 font-medium leading-snug">{p.name}</p>
                    {b.show_prices && <p className="num font-semibold">{b.currency} {amount(p.price)}</p>}
                    <p className={cn("text-xs font-medium", out ? "text-destructive" : "text-success")}>{out ? "Out of stock" : "In stock"}</p>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </main>

      {units > 0 && (
        <button onClick={() => setCartOpen(true)} className="fixed bottom-6 end-5 z-30 flex h-16 w-16 items-center justify-center rounded-full bg-foreground text-background shadow-lift animate-pop" aria-label={`Cart, ${units} ${t("items")}`}>
          <ShoppingCart className="h-6 w-6" />
          <span className="num absolute -end-1 -top-1 flex h-6 min-w-6 items-center justify-center rounded-full bg-primary px-1.5 text-xs font-bold text-primary-foreground">{units}</span>
        </button>
      )}

      <ProductSheet b={b} item={open} qty={open ? cart[open.id]?.qty ?? 0 : 0} onClose={() => setOpen(null)} onAdd={(item, qty) => { setQty(item, qty); setOpen(null); toast.success("Added to cart"); }} />

      <ResponsiveDialog
        open={cartOpen}
        onOpenChange={setCartOpen}
        title="Your order"
        footer={
          <Button variant="ink" size="lg" className="h-14 w-full rounded-xl" disabled={!lines.length || !location.trim() || submit.isPending} onClick={() => submit.mutate()}>
            {submit.isPending ? <Loader2 className="animate-spin" /> : <>Submit Order{b.show_prices && <> · <span className="num">{b.currency} {amount(total)}</span></>}</>}
          </Button>
        }
      >
        <div className="space-y-5">
          <ul className="divide-y">
            {lines.map((l) => (
              <li key={l.item.id} className="flex items-center gap-3 py-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">{l.item.name}</p>
                  {b.show_prices && <p className="num text-sm text-muted-foreground">{b.currency} {amount(l.item.price)}</p>}
                </div>
                <div className="flex items-center rounded-full border">
                  <button className="p-2" onClick={() => setQty(l.item, l.qty - 1)} aria-label="Less">{l.qty === 1 ? <Trash2 className="h-4 w-4" /> : <Minus className="h-4 w-4" />}</button>
                  <span className="num w-6 text-center">{l.qty}</span>
                  <button className="p-2" onClick={() => setQty(l.item, l.qty + 1)} disabled={l.qty >= l.item.available} aria-label="More"><Plus className="h-4 w-4" /></button>
                </div>
                {b.show_prices && <span className="num w-20 text-end font-semibold">{amount(toNum(l.item.price) * l.qty)}</span>}
              </li>
            ))}
          </ul>
          {b.show_prices && <div className="flex justify-between text-lg font-semibold"><span>{t("Total")}</span><span className="num">{b.currency} {amount(total)}</span></div>}
          <div className="space-y-1 rounded-xl bg-muted/60 p-4 text-sm">
            <p><span className="text-muted-foreground">{t("Customer:")}</span> {sess.customer.first_name}{sess.customer.nickname && ` (${sess.customer.nickname})`}</p>
            <p><span className="text-muted-foreground">{t("Mobile:")}</span> <span className="num">{phone(sess.customer.mobile)}</span></p>
          </div>
          <Field label="Deliver to"><Input className="h-12 rounded-xl" value={location} onChange={(e) => setLocation(e.target.value)} placeholder="Area, building, landmark" /></Field>
          <Field label="Notes" optional><Textarea value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Anything we should know?" /></Field>
        </div>
      </ResponsiveDialog>
    </div>
  );
}

function ProductSheet({ b, item, qty, onClose, onAdd }: { b: Business; item: Item | null; qty: number; onClose: () => void; onAdd: (item: Item, qty: number) => void }) {
  const [n, setN] = useState(1);
  const [photo, setPhoto] = useState(0);
  const detail = useQuery({ queryKey: ["portal", b.slug, "product", item?.id], queryFn: () => api<{ photos: string[] }>(`/portal/${b.slug}/products/${item!.id}`, { token: null }), enabled: !!item });
  if (!item) return null;
  const photos = detail.data?.photos ?? (item.primary_photo_id ? [photoUrl(item.primary_photo_id)!] : []);
  const out = item.available <= 0;
  return (
    <ResponsiveDialog
      open={!!item}
      onOpenChange={(o) => { if (!o) { onClose(); setN(1); setPhoto(0); } }}
      title={item.name}
      wide
      footer={
        <div className="flex w-full items-center gap-3">
          <div className="flex items-center rounded-full border">
            <button className="p-3" onClick={() => setN(Math.max(1, n - 1))} aria-label="Less"><Minus className="h-4 w-4" /></button>
            <span className="num w-8 text-center text-lg">{n}</span>
            <button className="p-3" onClick={() => setN(Math.min(item.available, n + 1))} aria-label="More"><Plus className="h-4 w-4" /></button>
          </div>
          <Button variant="ink" size="lg" className="h-12 flex-1 rounded-xl" disabled={out} onClick={() => onAdd(item, qty + n)}>
            {out ? "Out of stock" : <>Add to Cart{b.show_prices && <> · <span className="num">{b.currency} {amount(toNum(item.price) * n)}</span></>}</>}
          </Button>
        </div>
      }
    >
      <div className="grid gap-5 md:grid-cols-2">
        <div className="space-y-2">
          <div className="aspect-square overflow-hidden rounded-2xl bg-muted">
            {photos[photo] ? <img src={photos[photo]} alt={item.name} className="h-full w-full object-cover" /> : <ImageOff className="m-auto mt-[40%] h-10 w-10 text-muted-foreground" />}
          </div>
          {photos.length > 1 && (
            <div className="scrollbar-none flex gap-2 overflow-x-auto">
              {photos.map((u, i) => (
                <button key={u} onClick={() => setPhoto(i)} className={cn("h-16 w-16 shrink-0 overflow-hidden rounded-lg border-2", i === photo ? "border-primary" : "border-transparent")}><img src={u} alt="" className="h-full w-full object-cover" /></button>
              ))}
            </div>
          )}
        </div>
        <div className="space-y-3">
          {b.show_prices && <p className="num text-2xl font-bold">{b.currency} {amount(item.price)}</p>}
          <p className={cn("text-sm font-medium", out ? "text-destructive" : "text-success")}>{out ? "Out of stock" : item.available <= 5 ? `Only ${item.available} left` : "In stock"}</p>
          {item.description && <p className="whitespace-pre-line text-muted-foreground">{item.description}</p>}
          {qty > 0 && <p className="text-sm text-muted-foreground">{qty} already in your cart</p>}
        </div>
      </div>
    </ResponsiveDialog>
  );
}

function MyOrders({ b, sess }: { b: Business; sess: PortalSession }) {
  const { data, isLoading } = useQuery({ queryKey: ["portal", b.slug, "orders", sess.token], queryFn: () => api<{ total_orders: number; orders: PortalOrder[] }>(`/portal/${b.slug}/orders`, { token: sess.token }) });
  return (
    <Narrow>
      <PortalHeader name={b.name} tagline={b.tagline} logo={b.logo_url} />
      <main className="space-y-5 px-5 py-8">
        <Link to={`/order/${b.slug}`} className="inline-flex items-center gap-2 text-muted-foreground hover:text-foreground"><ArrowLeft className="h-4 w-4" /> {t("Back")}</Link>
        <div>
          <h2 className="text-2xl font-semibold">{t("My Orders")}</h2>
          {data && <p className="mt-1 text-muted-foreground">{t("Total orders:")} {data.total_orders}.{data.total_orders > 3 && " Showing your 3 most recent."}</p>}
        </div>
        {isLoading ? <Loading /> : data?.orders.map((o, i) => {
          const active = !["completed", "cancelled", "rejected", "returned"].includes(o.status);
          return (
            <div key={o.id} className="surface card-body space-y-4">
              <Link to={`/track/${o.track_token}`} className="flex items-start justify-between gap-3">
                <div><p className="num font-medium">{o.order_no}</p><p className="num text-sm text-muted-foreground">{date(o.created_at)}</p></div>
                <div className="text-end">{b.show_prices && <p className="num font-semibold">{amount(o.total)}</p>}<p className="text-sm text-muted-foreground">{o.status_label}</p></div>
              </Link>
              {active && i === 0 && <Steps steps={o.steps} compact />}
            </div>
          );
        })}
        <Button asChild variant="ink" size="lg" className="h-14 w-full rounded-xl"><Link to={`/order/${b.slug}/shop`}>{t("Order Now")}</Link></Button>
      </main>
      <PoweredBy />
    </Narrow>
  );
}
