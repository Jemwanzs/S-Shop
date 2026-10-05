import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Gift, MessageCircle, Pencil, Phone, SlidersHorizontal, UserPlus } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { count, date, dateTime, initials, methodLabel, money, phone, signed, titleCase, toNum } from "@/lib/format";
import type { Customer, Money, Paged } from "@/lib/types";
import { CustomFieldValues } from "@/components/CustomFields";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { Medal, Pill, StatusBadge } from "@/components/Badges";
import { StatCard } from "@/components/Stat";
import { Field } from "@/components/Form";
import { Segments } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { CustomerForm } from "./CustomerForm";

interface Profile {
  customer: Customer;
  points_value: Money;
  sales: { id: string; receipt_no: string; created_at: string; total: Money; status: string; points_earned: number; payment_method: string; branch_name: string }[];
  orders: { id: string; order_no: string; status: string; total: Money; created_at: string }[];
  referred_by: { id: string; name: string } | null;
  referrals: { id: string; name: string; bonus_points: number }[];
  credit: { id: string; receipt_no: string; amount: Money; paid: Money; balance: Money; due_date: string; status: string }[];
}

interface LedgerRow { id: string; kind: string; points: number; receipt_no: string | null; notes: string; expires_at: string | null; user_name: string | null; created_at: string }

export default function CustomerDetail() {
  const { id } = useParams();
  const { currency, can, profile: session } = useSession();
  const qc = useQueryClient();
  const [tab, setTab] = useState<"sales" | "orders" | "credit" | "points" | "referrals">("sales");
  const [editing, setEditing] = useState(false);
  const [pointsAction, setPointsAction] = useState<"redeem" | "adjust" | null>(null);
  const [pts, setPts] = useState("");
  const [notes, setNotes] = useState("");
  const [referring, setReferring] = useState(false);
  const [referred, setReferred] = useState("");

  const { data, isLoading, error, refetch } = useQuery({ queryKey: ["customer", id], queryFn: () => api<Profile>(`/customers/${id}`) });
  const ledger = useQuery({ queryKey: ["customer", id, "ledger"], queryFn: () => api<Paged<LedgerRow>>(`/customers/${id}/loyalty`, { query: { limit: 100 } }), enabled: tab === "points" && can("customers.view_loyalty") });
  const candidates = useQuery({
    queryKey: ["customers", "refer", referred],
    queryFn: () => api<Paged<Customer>>("/customers", { query: { q: referred, limit: 8 } }),
    enabled: referring && referred.length >= 2,
  });

  const points = useMutation({
    mutationFn: () => api(`/customers/${id}/${pointsAction === "redeem" ? "redeem" : "points"}`, { body: { points: parseInt(pts), notes } }),
    onSuccess: () => {
      toast.success(pointsAction === "redeem" ? "Points redeemed" : "Points adjusted");
      setPointsAction(null);
      setPts("");
      setNotes("");
      qc.invalidateQueries({ queryKey: ["customer", id] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const refer = useMutation({
    mutationFn: (referredId: string) => api("/referrals", { body: { referrer_id: id, referred_id: referredId } }),
    onSuccess: () => {
      toast.success("Referral recorded");
      setReferring(false);
      setReferred("");
      qc.invalidateQueries({ queryKey: ["customer", id] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const message = async () => {
    try {
      const r = await api<{ sent: boolean; link: string }>(`/awards/message/${id}`, { method: "POST" });
      if (r.sent) toast.success("Message sent on WhatsApp");
      else window.open(r.link, "_blank");
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  if (error) return <ErrorState error={error} retry={refetch} />;
  if (isLoading || !data) return <Loading />;
  const c = data.customer;
  const loyalty = can("customers.view_loyalty");
  const minRedeem = session?.settings.loyalty.min_redemption_points ?? 0;

  return (
    <>
      <PageHeader
        back="/customers"
        eyebrow={`Customer since ${date(c.created_at)}`}
        title={
          <span className="flex items-center gap-3">
            <span className="hidden h-12 w-12 items-center justify-center rounded-full bg-primary/12 text-lg font-semibold text-primary sm:flex">{initials(`${c.first_name} ${c.other_names}`)}</span>
            <span>{c.first_name} {c.other_names}</span>
            {c.tier && <Medal tier={c.tier} showLabel />}
          </span>
        }
        description={<span className="flex flex-wrap items-center gap-2"><span className="num">{phone(c.mobile)}</span>{c.nickname && <span>· “{c.nickname}”</span>}{!c.is_active && <Pill tone="danger">Inactive</Pill>}</span>}
        actions={
          <>
            <Button variant="outline" asChild><a href={`tel:+${c.mobile}`}><Phone /> Call</a></Button>
            {loyalty && <Button variant="outline" onClick={message}><MessageCircle /> WhatsApp</Button>}
            {can("customers.edit") && <Button onClick={() => setEditing(true)}><Pencil /> Edit</Button>}
          </>
        }
      />

      <div className="mb-5 grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
        <StatCard label="Total spend" value={money(c.total_spend, currency)} tone="success" className="col-span-2 md:col-span-1" />
        <StatCard label="Purchases" value={count(c.purchase_count)} hint={c.last_purchase_at ? `last ${date(c.last_purchase_at)}` : undefined} />
        {loyalty && <StatCard label="Points available" value={count(c.points_available)} tone="primary" hint={`worth ${money(data.points_value, currency)}`} />}
        {loyalty && <StatCard label="Own | referral" value={`${count(c.own_points)} | ${count(c.referral_points)}`} />}
        {loyalty && <StatCard label="Redeemed · expired" value={`${count(c.points_redeemed)} · ${count(c.points_expired)}`} />}
        {can("customers.view_credit") && <StatCard label="Owes" value={money(c.credit_balance, currency)} tone={toNum(c.credit_balance) > 0 ? "danger" : "default"} />}
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
        <div className="min-w-0 space-y-4">
          <Segments
            value={tab}
            onChange={setTab}
            options={[
              { value: "sales", label: "Purchases", count: data.sales.length },
              { value: "orders", label: "Orders", count: data.orders.length },
              ...(can("customers.view_credit") ? [{ value: "credit" as const, label: "Credit", count: data.credit.length }] : []),
              ...(loyalty ? [{ value: "points" as const, label: "Points history" }] : []),
              { value: "referrals" as const, label: "Referrals", count: data.referrals.length },
            ]}
          />
          <div className="surface p-2 md:p-4">
            {tab === "sales" && (
              <List empty="No purchases yet">
                {data.sales.map((s) => (
                  <Row key={s.id} to={`/sales/${s.id}`} title={<span className="num">{s.receipt_no}</span>} sub={`${dateTime(s.created_at)} · ${s.branch_name} · ${methodLabel(s.payment_method)}`} value={money(s.total, currency)} meta={s.status !== "completed" ? <StatusBadge status={s.status} /> : s.points_earned ? `🌼 +${s.points_earned}` : undefined} />
                ))}
              </List>
            )}
            {tab === "orders" && (
              <List empty="No orders yet">
                {data.orders.map((o) => <Row key={o.id} to={`/orders/${o.id}`} title={<span className="num">{o.order_no}</span>} sub={dateTime(o.created_at)} value={money(o.total, currency)} meta={<StatusBadge status={o.status} />} />)}
              </List>
            )}
            {tab === "credit" && (
              <List empty="No credit history">
                {data.credit.map((cr) => <Row key={cr.id} to={`/credit/${cr.id}`} title={<span className="num">{cr.receipt_no}</span>} sub={`Due ${date(cr.due_date)} · paid ${money(cr.paid, currency)}`} value={money(cr.balance, currency)} meta={<StatusBadge status={cr.status} />} />)}
              </List>
            )}
            {tab === "points" && (
              <List empty="No points activity">
                {ledger.data?.items.map((l) => (
                  <li key={l.id} className="flex items-center justify-between gap-3 px-2 py-3 text-sm">
                    <div className="min-w-0">
                      <div className="font-medium">{titleCase(l.kind)}{l.receipt_no && <span className="num text-muted-foreground"> · {l.receipt_no}</span>}</div>
                      <div className="truncate text-xs text-muted-foreground">{dateTime(l.created_at)}{l.notes && ` · ${l.notes}`}{l.expires_at && ` · expires ${date(l.expires_at)}`}</div>
                    </div>
                    <span className={cn("num font-semibold", l.points > 0 ? "text-success" : "text-destructive")}>{signed(l.points)}</span>
                  </li>
                ))}
              </List>
            )}
            {tab === "referrals" && (
              <div className="space-y-3 p-2">
                {data.referred_by && <p className="text-sm">Referred by <Link to={`/customers/${data.referred_by.id}`} className="font-medium text-primary">{data.referred_by.name}</Link></p>}
                <List empty="Has not referred anyone yet">
                  {data.referrals.map((r) => <Row key={r.id} to={`/customers/${r.id}`} title={r.name} sub="Referred customer" value={`+${count(r.bonus_points)} pts`} />)}
                </List>
                {(can("loyalty.manage") || can("customers.create")) && <Button variant="outline" onClick={() => setReferring(true)}><UserPlus /> Record a referral</Button>}
              </div>
            )}
          </div>
        </div>

        <div className="space-y-5">
          {loyalty && (
            <Section title="Loyalty">
              <div className="flex flex-col gap-2">
                {can("customers.redeem_points") && (
                  <Button variant="outline" disabled={c.points_available < minRedeem} onClick={() => setPointsAction("redeem")}><Gift /> Redeem points</Button>
                )}
                {can("loyalty.manage") && <Button variant="outline" onClick={() => setPointsAction("adjust")}><SlidersHorizontal /> Adjust points</Button>}
                {c.points_available < minRedeem && <p className="text-xs text-muted-foreground">Redemption from {count(minRedeem)} points.</p>}
              </div>
            </Section>
          )}
          <Section title="Details">
            <KV label="Mobile"><span className="num">{phone(c.mobile)}</span></KV>
            {c.email && <KV label="Email">{c.email}</KV>}
            <CustomFieldValues kind="customer" values={c.custom_fields} />
          </Section>
        </div>
      </div>

      <CustomerForm open={editing} onOpenChange={setEditing} customer={c} />
      <ResponsiveDialog
        open={!!pointsAction}
        onOpenChange={(o) => !o && setPointsAction(null)}
        title={pointsAction === "redeem" ? "Redeem points" : "Adjust points"}
        description={`${count(c.points_available)} points available`}
        footer={<Button className="w-full md:w-auto" disabled={!pts || parseInt(pts) === 0 || (pointsAction === "adjust" && !notes.trim()) || points.isPending} onClick={() => points.mutate()}>Save</Button>}
      >
        <div className="space-y-4">
          <Field label={pointsAction === "redeem" ? "Points to redeem" : "Points (use − to remove)"}>
            <Input inputMode={pointsAction === "adjust" ? "text" : "numeric"} className="num" value={pts} onChange={(e) => setPts(e.target.value.replace(pointsAction === "adjust" ? /[^\d-]/g : /\D/g, ""))} autoFocus />
          </Field>
          {pointsAction === "redeem" && pts && <p className="num text-sm text-muted-foreground">Worth {money(parseInt(pts) * toNum(session?.settings.loyalty.point_value), currency)}</p>}
          <Field label={pointsAction === "redeem" ? "What was given" : "Reason"} optional={pointsAction === "redeem"}><Input value={notes} onChange={(e) => setNotes(e.target.value)} /></Field>
        </div>
      </ResponsiveDialog>
      <ResponsiveDialog open={referring} onOpenChange={setReferring} title={`${c.first_name} referred…`} description="The referrer earns a share of the new customer's points.">
        <Input placeholder="Search the referred customer" value={referred} onChange={(e) => setReferred(e.target.value)} autoFocus />
        <ul className="mt-2 divide-y rounded-lg border empty:hidden">
          {candidates.data?.items.filter((x) => x.id !== c.id).map((x) => (
            <li key={x.id}><button className="flex w-full justify-between px-3 py-2.5 text-left text-sm hover:bg-accent" onClick={() => refer.mutate(x.id)}><span>{x.first_name} {x.other_names}</span><span className="num text-muted-foreground">{phone(x.mobile)}</span></button></li>
          ))}
        </ul>
      </ResponsiveDialog>
    </>
  );
}

function List({ children, empty }: { children: React.ReactNode; empty: string }) {
  const arr = Array.isArray(children) ? children : [children];
  if (!arr.filter(Boolean).length) return <p className="py-8 text-center text-sm text-muted-foreground">{empty}</p>;
  return <ul className="divide-y">{children}</ul>;
}

function Row({ to, title, sub, value, meta }: { to: string; title: React.ReactNode; sub: string; value: string; meta?: React.ReactNode }) {
  return (
    <li>
      <Link to={to} className="flex items-center justify-between gap-3 rounded-lg px-2 py-3 hover:bg-accent/40">
        <div className="min-w-0">
          <div className="font-medium">{title}</div>
          <div className="truncate text-xs text-muted-foreground">{sub}</div>
        </div>
        <div className="shrink-0 text-right">
          <div className="num font-semibold">{value}</div>
          {meta && <div className="text-xs">{meta}</div>}
        </div>
      </Link>
    </li>
  );
}
