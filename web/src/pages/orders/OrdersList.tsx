import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ClipboardList, Minus, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/api";
import { orderLabel, orderStepEnabled } from "@/lib/orders";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { ago, count, money, toNum } from "@/lib/format";
import type { OrderRow, Paged, PosProduct } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PageHeader, EmptyState } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { SearchInput, Segments } from "@/components/Filters";
import { StatusBadge, Pill } from "@/components/Badges";
import { Field } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

const LIMIT = 50;
const TABS = ["active", "new", "confirmed", "preparing", "dispatched", "on_delivery", "delivered", "completed", "cancelled", "all"] as const;

export default function OrdersList() {
  const { currency, can, profile } = useSession();
  const navigate = useNavigate();
  const [status, setStatus] = useState<(typeof TABS)[number]>("active");
  const [q, setQ] = useState("");
  const [offset, setOffset] = useState(0);
  const [creating, setCreating] = useState(false);
  const term = useDebounced(q);
  const query = { status, q: term, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["orders", query],
    queryFn: () => api<Paged<OrderRow>>("/orders", { query }),
    placeholderData: (p) => p,
    refetchInterval: 60_000,
  });
  const summary = useQuery({ queryKey: ["orders", "summary"], queryFn: () => api<Record<string, number>>("/orders/summary") });
  const counts = summary.data ?? {};
  const activeCount = ["new", "confirmed", "preparing", "dispatched", "on_delivery"].reduce((a, k) => a + (counts[k] ?? 0), 0);

  return (
    <>
      <PageHeader
        eyebrow="Orders"
        title="Customer orders"
        description={profile && <>Ordering link: <a className="text-primary underline" href={`/order/${profile.tenant.slug}`} target="_blank" rel="noreferrer">/order/{profile.tenant.slug}</a></>}
        actions={can("orders.manage") && <Button onClick={() => setCreating(true)}><Plus /> Phone order</Button>}
      />
      <div className="mb-4 space-y-3">
        <Segments
          value={status}
          onChange={(v) => { setStatus(v); setOffset(0); }}
          options={TABS.filter((t) => orderStepEnabled(profile?.settings, t)).map((t) => ({ value: t, label: t === "active" ? "Active" : t === "all" ? "All" : orderLabel(profile?.settings, t), count: t === "active" ? activeCount : ["all", "cancelled"].includes(t) ? undefined : counts[t] }))}
        />
        <SearchInput value={q} onChange={(v) => { setQ(v); setOffset(0); }} placeholder="Order number, customer or mobile" className="md:max-w-sm" />
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        onRowClick={(r) => navigate(`/orders/${r.id}`)}
        empty={<EmptyState icon={ClipboardList} title="No orders here" hint="Share your ordering link with customers to receive orders." />}
        columns={[
          { key: "no", header: "Order", cell: (r) => <span className="num font-medium">{r.order_no}</span> },
          { key: "customer", header: "Customer", cell: (r) => <div><div>{r.customer_name}</div><div className="truncate text-xs text-muted-foreground">{r.delivery_location}</div></div> },
          { key: "items", header: "Items", align: "right", cell: (r) => <span className="num">{count(r.item_count)}</span>, hideBelow: "lg" },
          { key: "source", header: "Source", cell: (r) => <Pill>{r.source === "portal" ? "Online" : "Staff"}</Pill>, hideBelow: "xl" },
          { key: "branch", header: "Branch", cell: (r) => r.branch_name, hideBelow: "xl" },
          { key: "age", header: "Placed", cell: (r) => <span className="whitespace-nowrap text-muted-foreground">{ago(r.created_at)}</span> },
          { key: "status", header: "Status", cell: (r) => <div className="flex gap-1.5"><StatusBadge status={r.status} label={orderLabel(profile?.settings, r.status)} />{r.reserved && <Pill tone="info">reserved</Pill>}</div> },
          { key: "total", header: "Total", align: "right", cell: (r) => <span className="num font-semibold">{money(r.total, currency)}</span> },
        ]}
        mobile={(r) => (
          <CardRow
            title={<span className="flex items-center gap-2">{r.customer_name} {r.status === "new" && <span className="h-2 w-2 rounded-full bg-primary" />}</span>}
            subtitle={<span className="num">{r.order_no} · {ago(r.created_at)}</span>}
            value={money(r.total, currency)}
            meta={<StatusBadge status={r.status} label={orderLabel(profile?.settings, r.status)} />}
          />
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
      <NewOrder open={creating} onOpenChange={setCreating} onCreated={(id) => navigate(`/orders/${id}`)} />
    </>
  );
}

function NewOrder({ open, onOpenChange, onCreated }: { open: boolean; onOpenChange: (o: boolean) => void; onCreated: (id: string) => void }) {
  const { currency } = useSession();
  const qc = useQueryClient();
  const [mobile, setMobile] = useState("");
  const [firstName, setFirstName] = useState("");
  const [location, setLocation] = useState("");
  const [notes, setNotes] = useState("");
  const [q, setQ] = useState("");
  const [items, setItems] = useState<{ product: PosProduct; quantity: number }[]>([]);
  const term = useDebounced(q);
  const products = useQuery({ queryKey: ["pos-products", "order", term], queryFn: () => api<PosProduct[]>("/pos/products", { query: { q: term } }), enabled: open });
  const total = items.reduce((a, i) => a + toNum(i.product.marked_price) * i.quantity, 0);
  const create = useMutation({
    mutationFn: () =>
      api<{ id: string; order_no: string }>("/orders", {
        body: { customer: { mobile, first_name: firstName }, items: items.map((i) => ({ product_id: i.product.id, quantity: i.quantity })), delivery_location: location, notes },
      }),
    onSuccess: (r) => {
      toast.success(`Order ${r.order_no} created`);
      qc.invalidateQueries({ queryKey: ["orders"] });
      onOpenChange(false);
      setItems([]);
      onCreated(r.id);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const setQty = (id: string, qty: number) => setItems((all) => all.map((i) => (i.product.id === id ? { ...i, quantity: Math.max(1, Math.min(qty, i.product.available)) } : i)));

  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={onOpenChange}
      title="New phone order"
      wide
      footer={<Button className="w-full md:w-auto" disabled={!items.length || mobile.replace(/\D/g, "").length < 9 || create.isPending} onClick={() => create.mutate()}>Create order · <span className="num">{money(total, currency)}</span></Button>}
    >
      <div className="grid gap-5 md:grid-cols-2">
        <div className="space-y-3">
          <Field label="Customer mobile"><Input inputMode="tel" className="num" value={mobile} onChange={(e) => setMobile(e.target.value)} placeholder="07XXXXXXXX" /></Field>
          <Field label="First name" hint="Needed if this is a new customer"><Input value={firstName} onChange={(e) => setFirstName(e.target.value)} /></Field>
          <Field label="Deliver to"><Input value={location} onChange={(e) => setLocation(e.target.value)} placeholder="Area, building, landmark" /></Field>
          <Field label="Notes" optional><Input value={notes} onChange={(e) => setNotes(e.target.value)} /></Field>
        </div>
        <div className="space-y-3">
          <SearchInput value={q} onChange={setQ} placeholder="Add products" />
          {term && (
            <ul className="max-h-40 divide-y overflow-y-auto rounded-lg border">
              {products.data?.map((p) => (
                <li key={p.id}>
                  <button className="flex w-full justify-between px-3 py-2 text-left text-sm hover:bg-accent" onClick={() => { if (!items.some((i) => i.product.id === p.id)) setItems([...items, { product: p, quantity: 1 }]); setQ(""); }}>
                    <span>{p.name}</span><span className="num text-muted-foreground">{count(p.available)} · {money(p.marked_price, currency)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <ul className="divide-y rounded-lg border">
            {items.map((i) => (
              <li key={i.product.id} className="flex items-center gap-2 px-3 py-2 text-sm">
                <span className="min-w-0 flex-1 truncate">{i.product.name}</span>
                <button className="p-1" onClick={() => setQty(i.product.id, i.quantity - 1)} aria-label="Less"><Minus className="h-3.5 w-3.5" /></button>
                <span className="num w-6 text-center">{i.quantity}</span>
                <button className="p-1" onClick={() => setQty(i.product.id, i.quantity + 1)} aria-label="More"><Plus className="h-3.5 w-3.5" /></button>
                <button className="p-1 text-muted-foreground" onClick={() => setItems(items.filter((x) => x !== i))} aria-label="Remove"><Trash2 className="h-3.5 w-3.5" /></button>
              </li>
            ))}
            {!items.length && <li className="px-3 py-4 text-center text-sm text-muted-foreground">No products yet</li>}
          </ul>
        </div>
      </div>
    </ResponsiveDialog>
  );
}
