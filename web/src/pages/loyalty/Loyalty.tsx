import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Award, Coins, Gift, Lock, MessageCircle, Plus, Trophy, UserPlus, Users } from "lucide-react";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, date, maskPhone, money, phone } from "@/lib/format";
import type { Customer, Money, Paged } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState, Loading, PageHeader, Section } from "@/components/Page";
import { Medal, PointsPill } from "@/components/Badges";
import { StatCard } from "@/components/Stat";
import { SearchInput, Segments } from "@/components/Filters";
import { ConfirmDialog, Field } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

type Tab = "overview" | "referrals" | "awards";

export default function Loyalty() {
  const [tab, setTab] = useState<Tab>("overview");
  return (
    <>
      <PageHeader eyebrow="Customers" title="Loyalty & Rewards" description="Points, referrals and award periods that keep customers coming back." />
      <div className="mb-5">
        <Segments value={tab} onChange={setTab} options={[{ value: "overview", label: "Overview" }, { value: "referrals", label: "Referrals" }, { value: "awards", label: "Awards" }]} />
      </div>
      {tab === "overview" && <Overview />}
      {tab === "referrals" && <Referrals />}
      {tab === "awards" && <Awards />}
    </>
  );
}

interface OverviewData {
  totals: { own_points: number; referral_points: number; redeemed: number; expired: number; outstanding: number; outstanding_value: Money; members_with_points: number };
  tiers: { tier: string; count: number }[];
  top_customers: { id: string; name: string; mobile: string; total_spend: Money; own_points: number; referral_points: number; tier: string }[];
  rules: { threshold: Money; points_per: number; referral_bonus_percent: number; point_value: Money; expiry_days: number; min_redemption_points: number; enabled: boolean };
}

function Overview() {
  const { currency, can } = useSession();
  const { data, isLoading } = useQuery({ queryKey: ["loyalty", "overview"], queryFn: () => api<OverviewData>("/loyalty/overview") });
  if (isLoading || !data) return <Loading />;
  const t = data.totals;
  const r = data.rules;
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
        <StatCard label="Outstanding points" value={count(t.outstanding)} icon={Coins} tone="primary" hint={`worth ${money(t.outstanding_value, currency)}`} className="col-span-2 md:col-span-1" />
        <StatCard label="Own points earned" value={count(t.own_points)} icon={Gift} />
        <StatCard label="Referral points" value={count(t.referral_points)} icon={UserPlus} />
        <StatCard label="Redeemed" value={count(t.redeemed)} icon={Award} tone="success" />
        <StatCard label="Expired" value={count(t.expired)} icon={Lock} />
        <StatCard label="Members with points" value={count(t.members_with_points)} icon={Users} />
      </div>
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        <Section title="Top customers">
          <ul className="divide-y">
            {data.top_customers.map((c, i) => (
              <li key={c.id}>
                <Link to={`/customers/${c.id}`} className="flex items-center gap-3 py-3">
                  <span className="num w-6 text-center text-muted-foreground">{i + 1}</span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5 truncate font-medium">{c.name} {i < 3 && <Medal tier={["Gold", "Silver", "Bronze"][i]} />}</div>
                    <div className="num text-xs text-muted-foreground">{maskPhone(c.mobile)}{c.tier && <span className="font-sans"> · {c.tier}</span>}</div>
                  </div>
                  <div className="text-right">
                    <div className="num font-semibold">{money(c.total_spend, currency)}</div>
                    <PointsPill own={c.own_points} referral={c.referral_points} />
                  </div>
                </Link>
              </li>
            ))}
          </ul>
        </Section>
        <div className="space-y-5">
          <Section title="How points work" action={can("settings.manage") && <Link to="/settings/loyalty" className="text-xs text-primary">Configure</Link>}>
            {r.enabled ? (
              <ul className="space-y-2 text-sm">
                <li>🌼 <span className="num font-medium">{r.points_per}</span> point{r.points_per === 1 ? "" : "s"} for every <span className="num font-medium">{money(r.threshold, currency)}</span> spent</li>
                <li>🤝 Referrers earn <span className="font-medium">{r.referral_bonus_percent}%</span> of their referrals' points</li>
                <li>🎁 1 point = <span className="num font-medium">{money(r.point_value, currency, true)}</span> · redeem from {count(r.min_redemption_points)} points</li>
                <li>⏳ {r.expiry_days > 0 ? `Points expire after ${r.expiry_days} days` : "Points never expire"}</li>
              </ul>
            ) : (
              <p className="text-sm text-muted-foreground">Loyalty is turned off.</p>
            )}
          </Section>
          <Section title="Tiers">
            <ul className="space-y-2">
              {data.tiers.map((t) => (
                <li key={t.tier} className="flex items-center justify-between text-sm"><span className="flex items-center gap-1.5"><Medal tier={t.tier} /> {t.tier}</span><span className="num font-medium">{count(t.count)}</span></li>
              ))}
            </ul>
          </Section>
        </div>
      </div>
    </div>
  );
}

interface Referral { id: string; referrer_id: string; referrer_name: string; referred_id: string; referred_name: string; bonus_points_earned: number; created_at: string }

function CustomerPicker({ label, value, onPick, exclude }: { label: string; value: Customer | null; onPick: (c: Customer | null) => void; exclude?: string }) {
  const [q, setQ] = useState("");
  const term = useDebounced(q);
  const res = useQuery({ queryKey: ["customers", "pick", term], queryFn: () => api<Paged<Customer>>("/customers", { query: { q: term, limit: 6 } }), enabled: term.length >= 2 && !value });
  return (
    <Field label={label}>
      {value ? (
        <div className="flex items-center justify-between rounded-lg border bg-accent/40 px-3 py-2.5 text-sm">
          <span>{value.first_name} {value.other_names} <span className="num text-muted-foreground">· {phone(value.mobile)}</span></span>
          <button className="text-primary" onClick={() => onPick(null)}>Change</button>
        </div>
      ) : (
        <>
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Name or mobile" />
          <ul className="mt-1 divide-y rounded-lg border empty:hidden">
            {res.data?.items.filter((c) => c.id !== exclude).map((c) => (
              <li key={c.id}><button className="flex w-full justify-between px-3 py-2 text-left text-sm hover:bg-accent" onClick={() => { onPick(c); setQ(""); }}><span>{c.first_name} {c.other_names}</span><span className="num text-muted-foreground">{phone(c.mobile)}</span></button></li>
            ))}
          </ul>
        </>
      )}
    </Field>
  );
}

function Referrals() {
  const { can } = useSession();
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const [adding, setAdding] = useState(false);
  const [referrer, setReferrer] = useState<Customer | null>(null);
  const [referred, setReferred] = useState<Customer | null>(null);
  const [removing, setRemoving] = useState<Referral | null>(null);
  const term = useDebounced(q);
  const { data, isLoading } = useQuery({ queryKey: ["referrals", term], queryFn: () => api<{ items: Referral[]; summary: { total: number; referrers: number; bonus_points: number } }>("/referrals", { query: { q: term } }) });
  const create = useMutation({
    mutationFn: () => api("/referrals", { body: { referrer_id: referrer!.id, referred_id: referred!.id } }),
    onSuccess: () => {
      toast.success("Referral recorded");
      setAdding(false);
      setReferrer(null);
      setReferred(null);
      qc.invalidateQueries({ queryKey: ["referrals"] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: (rid: string) => api(`/referrals/${rid}/deactivate`, { method: "POST" }),
    onSuccess: () => {
      toast.success("Referral removed");
      setRemoving(null);
      qc.invalidateQueries({ queryKey: ["referrals"] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const groups = new Map<string, Referral[]>();
  data?.items.forEach((r) => groups.set(r.referrer_id, [...(groups.get(r.referrer_id) ?? []), r]));

  return (
    <div className="space-y-5">
      <div className="grid grid-cols-3 gap-3 lg:max-w-2xl">
        <StatCard label="Referrals" value={count(data?.summary.total)} />
        <StatCard label="Referrers" value={count(data?.summary.referrers)} />
        <StatCard label="Bonus pts" value={count(data?.summary.bonus_points)} tone="primary" />
      </div>
      <div className="flex flex-col gap-2 sm:flex-row">
        <SearchInput value={q} onChange={setQ} placeholder="Filter referrals" className="sm:w-80" />
        {(can("loyalty.manage") || can("customers.create")) && <Button onClick={() => setAdding(true)}><Plus /> Record referral</Button>}
      </div>
      {isLoading ? <Loading /> : groups.size === 0 ? (
        <div className="surface"><EmptyState icon={UserPlus} title="No referrals yet" hint="Referrers earn bonus points every time the people they bring in shop." /></div>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 2xl:grid-cols-3">
          {[...groups.values()].map((rs) => (
            <div key={rs[0].referrer_id} className="surface overflow-hidden">
              <Link to={`/customers/${rs[0].referrer_id}`} className="flex items-center justify-between gap-2 bg-primary/5 px-4 py-3">
                <div className="min-w-0"><div className="truncate font-semibold">{rs[0].referrer_name}</div><div className="text-xs text-muted-foreground">Referred {rs.length} customer{rs.length === 1 ? "" : "s"}</div></div>
                <PointsPill own={rs.reduce((a, r) => a + r.bonus_points_earned, 0)} />
              </Link>
              <ul className="divide-y">
                {rs.map((r) => (
                  <li key={r.id} className="flex items-center justify-between gap-2 px-4 py-2.5 text-sm">
                    <div><Link to={`/customers/${r.referred_id}`} className="font-medium hover:underline">→ {r.referred_name}</Link><div className="text-xs text-muted-foreground">{date(r.created_at)}</div></div>
                    <div className="flex items-center gap-2">
                      <span className="num text-points">+{count(r.bonus_points_earned)}</span>
                      {can("loyalty.manage") && <button className="text-xs text-muted-foreground hover:text-destructive" onClick={() => setRemoving(r)}>Remove</button>}
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
      <ResponsiveDialog
        open={adding}
        onOpenChange={setAdding}
        title="Record a referral"
        footer={<Button className="w-full md:w-auto" disabled={!referrer || !referred || create.isPending} onClick={() => create.mutate()}>Save referral</Button>}
      >
        <div className="space-y-4">
          <CustomerPicker label="Who referred? (referrer)" value={referrer} onPick={setReferrer} exclude={referred?.id} />
          <CustomerPicker label="Who was referred? (new customer)" value={referred} onPick={setReferred} exclude={referrer?.id} />
        </div>
      </ResponsiveDialog>
      <ConfirmDialog
        open={!!removing}
        onOpenChange={(o) => !o && setRemoving(null)}
        title="Remove this referral?"
        description="Future purchases stop earning a bonus. Points already earned are kept."
        destructive
        confirmLabel="Remove"
        busy={remove.isPending}
        onConfirm={() => removing && remove.mutate(removing.id)}
      />
    </div>
  );
}

interface Period {
  id: string;
  name: string;
  start_date: string;
  end_date: string | null;
  status: "open" | "closed";
  winners: { customer_id: string; customer_name: string; tier: string; rank: number; total_spend: Money; points: number }[];
  standings: { customer_id: string; name: string; mobile: string; period_spend: Money; period_points: number; period_referral_points: number }[];
}

function Awards() {
  const { currency, can } = useSession();
  const qc = useQueryClient();
  const [closing, setClosing] = useState<Period | null>(null);
  const [opening, setOpening] = useState(false);
  const [name, setName] = useState("");
  const { data, isLoading } = useQuery({ queryKey: ["awards"], queryFn: () => api<{ periods: Period[]; winners_per_period: number }>("/awards") });
  const close = useMutation({
    mutationFn: (id: string) => api(`/awards/${id}/close`, { method: "POST" }),
    onSuccess: () => { toast.success("Award period closed — winners recorded 🏆"); setClosing(null); qc.invalidateQueries({ queryKey: ["awards"] }); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const open = useMutation({
    mutationFn: () => api("/awards", { body: { name } }),
    onSuccess: () => { toast.success("New award period opened"); setOpening(false); setName(""); qc.invalidateQueries({ queryKey: ["awards"] }); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const message = async (cid: string) => {
    try {
      const r = await api<{ sent: boolean; link: string }>(`/awards/message/${cid}`, { method: "POST" });
      if (r.sent) toast.success("Sent on WhatsApp");
      else window.open(r.link, "_blank");
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };
  if (isLoading || !data) return <Loading />;
  const current = data.periods.find((p) => p.status === "open");
  const past = data.periods.filter((p) => p.status === "closed");
  const medals = ["Gold", "Silver", "Bronze"];

  return (
    <div className="space-y-5">
      {current ? (
        <Section
          title={`${current.name} · since ${date(current.start_date)}`}
          action={can("loyalty.manage") && <Button size="sm" variant="outline" onClick={() => setClosing(current)}><Trophy /> Close & award</Button>}
        >
          {current.standings.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">No identified purchases in this period yet.</p>
          ) : (
            <ul className="divide-y">
              {current.standings.map((s, i) => (
                <li key={s.customer_id} className="flex items-center gap-3 py-3">
                  <span className="num w-6 text-center text-muted-foreground">{i + 1}</span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5 truncate font-medium">{s.name} {i < data.winners_per_period && <Medal tier={medals[i] ?? "Bronze"} />}</div>
                    <div className="num text-xs text-muted-foreground">{maskPhone(s.mobile)}</div>
                  </div>
                  <div className="text-right">
                    <div className="num font-semibold">{money(s.period_spend, currency)}</div>
                    <PointsPill own={s.period_points} referral={s.period_referral_points} />
                  </div>
                  <Button variant="ghost" size="icon-sm" onClick={() => message(s.customer_id)} aria-label="Message"><MessageCircle /></Button>
                </li>
              ))}
            </ul>
          )}
        </Section>
      ) : (
        <div className="surface flex flex-col items-center gap-3 p-8 text-center">
          <Trophy className="h-8 w-8 text-gold" />
          <p className="font-medium">No award period is running</p>
          {can("loyalty.manage") && <Button onClick={() => setOpening(true)}><Plus /> Open a new period</Button>}
        </div>
      )}
      <p className="text-xs text-muted-foreground">Standings rank customers by spend within the period. Closing a period records winners — balances and sales history are never reset.</p>
      {past.length > 0 && (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {past.map((p) => (
            <Section key={p.id} title={`${p.name} · ${date(p.start_date)} – ${date(p.end_date)}`}>
              <ul className="space-y-2">
                {p.winners.map((w) => (
                  <li key={w.customer_id} className="flex items-center justify-between gap-2 text-sm">
                    <span className="flex min-w-0 items-center gap-1.5 truncate"><Medal tier={w.tier} /> {w.customer_name}</span>
                    <span className="num">{money(w.total_spend, currency)}</span>
                  </li>
                ))}
                {p.winners.length === 0 && <li className="text-sm text-muted-foreground">No winners</li>}
              </ul>
            </Section>
          ))}
        </div>
      )}
      <ConfirmDialog
        open={!!closing}
        onOpenChange={(o) => !o && setClosing(null)}
        title={`Close ${closing?.name}?`}
        description={`The top ${data.winners_per_period} customers by spend are recorded as Gold, Silver and Bronze winners.`}
        confirmLabel="Close & award"
        busy={close.isPending}
        onConfirm={() => closing && close.mutate(closing.id)}
      />
      <ResponsiveDialog open={opening} onOpenChange={setOpening} title="Open award period" footer={<Button className="w-full md:w-auto" disabled={!name.trim() || open.isPending} onClick={() => open.mutate()}>Open period</Button>}>
        <Field label="Name"><Input value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Q4 2026 Awards" autoFocus /></Field>
      </ResponsiveDialog>
    </div>
  );
}
