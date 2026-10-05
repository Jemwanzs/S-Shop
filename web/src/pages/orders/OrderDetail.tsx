import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Copy, MapPin, MessageCircle, Phone } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, errorMessage, photoUrl } from "@/lib/api";
import { useSession } from "@/lib/session";
import { count, dateTime, money, phone, titleCase } from "@/lib/format";
import type { Money, OrderRow } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ORDER_FLOW, orderLabel, orderStepEnabled } from "@/lib/orders";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { Pill, StatusBadge } from "@/components/Badges";
import { Field } from "@/components/Form";
import { Chip } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

interface Detail {
  order: OrderRow;
  items: { product_id: string; product_name: string; quantity: number; unit_price: Money; line_total: Money; photo_id: string | null; on_hand: number; available: number }[];
  events: { status: string; label: string; notes: string; created_at: string; user_name: string | null }[];
  next_statuses: string[];
  sale_on_status: string;
  track_url: string;
}

export default function OrderDetail() {
  const { id } = useParams();
  const { currency, profile } = useSession();
  const qc = useQueryClient();
  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["order", id], queryFn: () => api<Detail>(`/orders/${id}`) });
  const [target, setTarget] = useState<string | null>(null);
  const [notes, setNotes] = useState("");
  const [method, setMethod] = useState("cash");
  const [reference, setReference] = useState("");

  const move = useMutation({
    mutationFn: () =>
      api<{ status: string; sale_id: string | null }>(`/orders/${id}/status`, {
        body: { status: target, notes, payment: needsPayment ? { method, reference } : undefined },
      }),
    onSuccess: (r) => {
      toast.success(r.sale_id && !data?.order.sale_id ? "Order completed and recorded as a sale" : `Order marked ${titleCase(r.status)}`);
      setTarget(null);
      setNotes("");
      setReference("");
      qc.invalidateQueries({ queryKey: ["order", id] });
      qc.invalidateQueries({ queryKey: ["orders"] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  if (error) return <ErrorState error={error} retry={refetch} />;
  if (isLoading || !data) return <Loading />;
  const o = data.order;
  const settings = profile?.settings;
  const label = (k: string) => (k === "cancelled" ? "Cancel order" : k === "rejected" ? "Reject order" : orderLabel(settings, k));
  const steps = ORDER_FLOW.filter((k) => orderStepEnabled(settings, k));
  const rank = ORDER_FLOW.indexOf(o.status);
  const saleRank = ORDER_FLOW.indexOf(data.sale_on_status);
  const needsPayment = !!target && !o.sale_id && ORDER_FLOW.indexOf(target) >= saleRank;
  const forward = data.next_statuses.filter((s) => ORDER_FLOW.includes(s));
  const terminalActions = data.next_statuses.filter((s) => !ORDER_FLOW.includes(s));
  const short = data.items.filter((i) => !o.reserved && !o.sale_id && i.available < i.quantity);
  const methods = profile?.settings.sales.payment_methods.filter((m) => m.enabled) ?? [];
  const waText = `Hi ${o.customer_name.split(" ")[0]}! Your order ${o.order_no} is ${titleCase(o.status)}. Track it here: ${data.track_url}`;

  return (
    <>
      <PageHeader
        back="/orders"
        eyebrow={dateTime(o.created_at)}
        title={<span className="num">{o.order_no}</span>}
        actions={<div className="flex gap-2"><StatusBadge status={o.status} label={orderLabel(profile?.settings, o.status)} />{o.reserved && <Pill tone="info">Stock reserved</Pill>}</div>}
      />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        <div className="space-y-5">
          {rank >= 0 && (
            <div className="surface p-5">
              <ol className="grid gap-1" style={{ gridTemplateColumns: `repeat(${steps.length}, minmax(0, 1fr))` }}>
                {steps.map((s, n) => {
                  const i = ORDER_FLOW.indexOf(s);
                  return (
                  <li key={s} className="flex flex-col items-center gap-1.5 text-center">
                    <span className={cn("flex h-8 w-8 items-center justify-center rounded-full border-2 text-xs", i <= rank ? "border-success bg-success text-success-foreground" : "border-border text-muted-foreground", i === rank && "ring-4 ring-success/20")}>
                      {i <= rank ? <Check className="h-4 w-4" /> : n + 1}
                    </span>
                    <span className={cn("hidden text-[11px] leading-tight sm:block", i <= rank ? "font-medium" : "text-muted-foreground")}>{label(s)}</span>
                  </li>
                  );
                })}
              </ol>
              <p className="mt-3 text-center text-sm sm:hidden">{label(o.status)}</p>
            </div>
          )}

          {forward.length > 0 && (
            <div className="surface flex flex-wrap items-center gap-2 p-4">
              <span className="label-caps mr-auto">Next step</span>
              {forward.slice(0, 3).map((s, i) => (
                <Button key={s} variant={i === 0 ? "default" : "outline"} onClick={() => setTarget(s)}>{label(s)}</Button>
              ))}
              {terminalActions.map((s) => (
                <Button key={s} variant="ghost" className="text-destructive" onClick={() => setTarget(s)}>{label(s)}</Button>
              ))}
            </div>
          )}
          {short.length > 0 && (
            <div className="rounded-xl bg-warning/10 p-4 text-sm text-warning">
              Not enough stock for: {short.map((i) => `${i.product_name} (${i.available} available)`).join(", ")}. Receive stock or request a transfer before confirming.
            </div>
          )}

          <Section title={`Items · ${count(o.item_count)}`}>
            <ul className="divide-y">
              {data.items.map((i) => (
                <li key={i.product_id} className="flex items-center gap-3 py-3">
                  {i.photo_id ? <img src={photoUrl(i.photo_id)} alt="" className="h-12 w-12 rounded-lg object-cover" loading="lazy" /> : <div className="h-12 w-12 rounded-lg bg-muted" />}
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{i.product_name}</div>
                    <div className="num text-xs text-muted-foreground">{count(i.quantity)} × {money(i.unit_price, currency)} · {count(i.available)} available now</div>
                  </div>
                  <span className="num font-semibold">{money(i.line_total, currency)}</span>
                </li>
              ))}
            </ul>
            <div className="flex justify-between border-t pt-3 text-base font-semibold"><span>Total</span><span className="num">{money(o.total, currency)}</span></div>
          </Section>

          <Section title="Timeline">
            <ol className="relative space-y-4 border-l pl-5">
              {data.events.map((e, i) => (
                <li key={i} className="relative">
                  <span className="absolute -left-[26px] top-1 h-2.5 w-2.5 rounded-full bg-primary" />
                  <div className="text-sm font-medium">{e.label}</div>
                  <div className="text-xs text-muted-foreground">{dateTime(e.created_at)}{e.user_name && ` · ${e.user_name}`}</div>
                  {e.notes && <div className="mt-0.5 text-sm">{e.notes}</div>}
                </li>
              ))}
            </ol>
          </Section>
        </div>

        <div className="space-y-5">
          <Section title="Customer">
            <Link to={`/customers/${o.customer_id}`} className="font-semibold hover:underline">{o.customer_name}</Link>
            <div className="mt-2 space-y-2 text-sm">
              <a href={`tel:+${o.customer_mobile}`} className="flex items-center gap-2 text-primary"><Phone className="h-4 w-4" /> <span className="num">{phone(o.customer_mobile)}</span></a>
              <p className="flex items-start gap-2"><MapPin className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" /> {o.delivery_location || "No location given"}</p>
              {o.notes && <p className="rounded-lg bg-muted p-2.5">{o.notes}</p>}
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2">
              <Button variant="outline" size="sm" onClick={() => window.open(`https://wa.me/${o.customer_mobile}?text=${encodeURIComponent(waText)}`, "_blank")}><MessageCircle /> WhatsApp</Button>
              <Button variant="outline" size="sm" onClick={() => { navigator.clipboard.writeText(data.track_url); toast.success("Tracking link copied"); }}><Copy /> Track link</Button>
            </div>
          </Section>
          <Section title="Fulfilment">
            <KV label="Branch">{o.branch_name}</KV>
            <KV label="Source">{o.source === "portal" ? "Ordering link" : "Staff"}</KV>
            <KV label="Stock">{o.sale_id ? "Sold" : o.reserved ? "Reserved" : "Not reserved"}</KV>
            <KV label="Becomes a sale at">{label(data.sale_on_status)}</KV>
            {o.sale_id && <KV label="Receipt"><Link to={`/sales/${o.sale_id}`} className="num text-primary">{o.receipt_no}</Link></KV>}
          </Section>
        </div>
      </div>

      <ResponsiveDialog
        open={!!target}
        onOpenChange={(v) => !v && setTarget(null)}
        title={target ? label(target) : ""}
        description={needsPayment ? "This step completes the order: stock is cleared and a sale is recorded." : undefined}
        footer={
          <Button
            className="w-full md:w-auto"
            variant={target === "cancelled" || target === "rejected" ? "destructive" : "default"}
            disabled={move.isPending || (needsPayment && method === "mpesa" && reference.length < 8)}
            onClick={() => move.mutate()}
          >
            Confirm
          </Button>
        }
      >
        <div className="space-y-4">
          {needsPayment && (
            <>
              <p className="num text-2xl font-bold">{money(o.total, currency)}</p>
              <div className="flex flex-wrap gap-2">
                {methods.map((m) => <Chip key={m.key} active={method === m.key} onClick={() => setMethod(m.key)}>{m.label}</Chip>)}
              </div>
              {method === "mpesa" && <Field label="M-Pesa confirmation code"><Input className="num uppercase" value={reference} onChange={(e) => setReference(e.target.value.toUpperCase())} /></Field>}
            </>
          )}
          <Field label="Note" optional><Textarea value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="e.g. rider name, reason" /></Field>
        </div>
      </ResponsiveDialog>
    </>
  );
}
