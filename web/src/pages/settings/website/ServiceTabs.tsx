/** Users & Access, Domain, SEO and Analytics. */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ChevronDown, Copy, ExternalLink, RefreshCw, Trash2, XCircle } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { amount, count, dateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Checkbox } from "@/components/ui/checkbox";
import { ActionButton } from "@/components/ActionButton";
import { ConfirmDialog, Field } from "@/components/Form";
import { Loading } from "@/components/Page";
import { StatCard as Stat } from "@/components/Stat";
import { t } from "@/lib/i18n";
import type { Overview } from "./data";
import { Block, Choice, MediaField } from "./kit";
import type { TabProps } from "./Website";

// ── Users & Access ────────────────────────────────────────────────────

const PERM_LABELS: Record<string, string> = {
  "website.view": "View the website centre", "website.content": "Edit content", "website.products": "Manage website products & prices",
  "website.photos": "Manage product photos", "website.categories": "Manage categories", "website.media": "Upload & manage media",
  "website.services": "Manage services", "website.testimonials": "Manage testimonials", "website.design": "Change design & colours",
  "website.navigation": "Change navigation", "website.seo": "Edit SEO", "website.domain": "Manage the domain",
  "website.preview": "Preview", "website.publish": "Publish changes", "website.analytics": "View analytics",
};

interface AccessUser { id: string; name: string; email: string; role: string; is_active: boolean; administrator: boolean; from_role: string[]; individual: string[] }

export function AccessTab() {
  const qc = useQueryClient();
  const { data, isLoading } = useQuery({ queryKey: ["website-access"], queryFn: () => api<{ permissions: string[]; can_manage: boolean; users: AccessUser[] }>("/website/access") });
  const [open, setOpen] = useState<string | null>(null);
  const [edits, setEdits] = useState<Record<string, string[]>>({});
  const save = useMutation({
    mutationFn: (u: AccessUser) => api(`/website/access/${u.id}`, { method: "PUT", body: { permissions: edits[u.id] ?? u.individual } }),
    onSuccess: (_, u) => {
      toast.success(`${t("Website access saved for")} ${u.name}`);
      setEdits((e) => { const n = { ...e }; delete n[u.id]; return n; });
      qc.invalidateQueries({ queryKey: ["website-access"] });
    },
    onError: (e) => toast.error(e),
  });
  if (isLoading || !data) return <Loading />;
  return (
    <Block title="Users & Access" hint="Give existing S'Shop users website permissions. Website access never opens sales, stock, finance or settings.">
      <ul className="divide-y">
        {data.users.map((u) => {
          const mine = edits[u.id] ?? u.individual;
          const total = new Set([...u.from_role, ...mine]).size;
          return (
            <li key={u.id} className="py-2">
              <button type="button" className="flex w-full items-center gap-3 text-start" onClick={() => setOpen(open === u.id ? null : u.id)} aria-expanded={open === u.id}>
                <span className="min-w-0 flex-1">
                  <span className={cn("block truncate text-sm font-medium", !u.is_active && "text-muted-foreground line-through")}>{u.name}</span>
                  <span className="block truncate text-xs text-muted-foreground">{u.role} · {u.email}</span>
                </span>
                <span className="text-xs text-muted-foreground">{u.administrator ? t("Full access") : `${total} / ${data.permissions.length}`}</span>
                <ChevronDown className={cn("h-4 w-4 text-muted-foreground transition-transform", open === u.id && "rotate-180")} />
              </button>
              {open === u.id && (
                <div className="mt-2 space-y-2 rounded-lg border bg-muted/20 p-3">
                  <div className="grid gap-1.5 sm:grid-cols-2">
                    {data.permissions.map((p) => {
                      const viaRole = u.from_role.includes(p);
                      return (
                        <label key={p} className="flex items-center gap-2 text-sm">
                          <Checkbox checked={viaRole || mine.includes(p)} disabled={viaRole || !data.can_manage}
                            onCheckedChange={(v) => setEdits((e) => ({ ...e, [u.id]: v ? [...mine, p] : mine.filter((x) => x !== p) }))} />
                          <span>{t(PERM_LABELS[p] ?? p)}{viaRole && <span className="text-xs text-muted-foreground"> · {t("from role")}</span>}</span>
                        </label>
                      );
                    })}
                  </div>
                  {data.can_manage && !u.administrator && (
                    <ActionButton size="sm" online busy={save.isPending} blockedBy={[!edits[u.id] && "No changes"]} onAction={() => save.mutateAsync(u)}>Save access</ActionButton>
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </Block>
  );
}

// ── Domain ────────────────────────────────────────────────────────────

interface DomainRecord { kind: string; name: string; value: string; status: "ok" | "missing" | "wrong" | "pending"; note: string }
interface DomainView { domain: string; status: string; message: string; records: DomainRecord[]; checked_at: string | null; verified_at: string | null; active_at: string | null; url: string; automatic: boolean }

const DOMAIN_STATUS: Record<string, [string, string]> = {
  dns_required: ["DNS records needed", "bg-warning/15 text-warning"],
  verifying: ["Connecting", "bg-primary/10 text-primary"],
  points_elsewhere: ["Points elsewhere", "bg-destructive/10 text-destructive"],
  ssl_pending: ["Certificate pending", "bg-primary/10 text-primary"],
  active: ["Active", "bg-success/15 text-success"],
  misconfigured: ["Misconfigured", "bg-destructive/10 text-destructive"],
};

function CopyValue({ value }: { value: string }) {
  return (
    <span className="flex min-w-0 items-center gap-1">
      <code className="num min-w-0 flex-1 break-all rounded bg-muted px-1.5 py-0.5 text-xs">{value}</code>
      <Button size="icon" variant="ghost" className="h-7 w-7 shrink-0" aria-label={t("Copy")} onClick={() => { navigator.clipboard.writeText(value); toast.success("Copied"); }}><Copy /></Button>
    </span>
  );
}

export function DomainTab({ ov }: { ov: Overview }) {
  const qc = useQueryClient();
  const { data, isLoading } = useQuery({ queryKey: ["website-domain"], queryFn: () => api<{ domain: DomainView | null; automatic: boolean }>("/website/domain") });
  const [input, setInput] = useState("");
  const [remove, setRemove] = useState(false);
  const done = () => {
    qc.invalidateQueries({ queryKey: ["website-domain"] });
    qc.invalidateQueries({ queryKey: ["website"] });
  };
  const connect = useMutation({ mutationFn: () => api("/website/domain", { method: "PUT", body: { domain: input } }), onSuccess: () => { setInput(""); done(); }, onError: (e) => toast.error(e) });
  const check = useMutation({ mutationFn: () => api<{ domain: DomainView }>("/website/domain/check", { body: {} }), onSuccess: (r) => { done(); if (r.domain.status === "active") toast.success("Your domain is live"); else toast(r.domain.message); }, onError: (e) => toast.error(e) });
  const del = useMutation({ mutationFn: () => api("/website/domain", { method: "DELETE" }), onSuccess: () => { setRemove(false); done(); }, onError: (e) => toast.error(e) });
  if (isLoading) return <Loading />;
  const d = data?.domain;
  return (
    <div className="space-y-4">
      <Block title="Your S'Shop address" hint="Always works, with or without your own domain.">
        <CopyValue value={`${location.origin}/s/${ov.slug}`} />
      </Block>
      {!d ? (
        <Block title="Connect your own domain" hint="For example myshop.co.ke or shop.mybrand.com. You need access to its DNS settings.">
          <form className="flex flex-wrap gap-2" onSubmit={(e) => { e.preventDefault(); connect.mutate(); }}>
            <Input value={input} onChange={(e) => setInput(e.target.value)} placeholder="myshop.co.ke" className="min-w-0 flex-1" autoCapitalize="none" spellCheck={false} />
            <ActionButton type="submit" online busy={connect.isPending} blockedBy={[!input.trim() && "Enter a domain"]}>Add domain</ActionButton>
          </form>
        </Block>
      ) : (
        <Block title={d.domain} action={<span className={cn("rounded-full px-2.5 py-0.5 text-xs font-medium", DOMAIN_STATUS[d.status]?.[1] ?? "bg-muted")}>{t(DOMAIN_STATUS[d.status]?.[0] ?? d.status)}</span>}>
          <p className="text-sm">{t(d.message)}</p>
          {d.status !== "active" && (
            <div className="space-y-2">
              <p className="text-sm font-medium">{t("Add these records at your domain provider:")}</p>
              {d.records.map((r) => (
                <div key={`${r.kind}-${r.name}`} className="space-y-1.5 rounded-lg border p-3">
                  <div className="flex items-center justify-between gap-2">
                    <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs font-semibold">{r.kind}</span>
                    {r.status === "ok" ? <span className="inline-flex items-center gap-1 text-xs text-success"><CheckCircle2 className="h-3.5 w-3.5" /> {t("Found")}</span>
                      : r.status === "pending" ? <span className="text-xs text-muted-foreground">{t("Not checked yet")}</span>
                        : <span className="inline-flex items-center gap-1 text-xs text-destructive"><XCircle className="h-3.5 w-3.5" /> {r.status === "wrong" ? t("Points elsewhere") : t("Not found yet")}</span>}
                  </div>
                  <div className="grid gap-1 text-xs sm:grid-cols-[4rem_1fr]"><span className="text-muted-foreground">{t("Name")}</span><CopyValue value={r.name} /></div>
                  <div className="grid gap-1 text-xs sm:grid-cols-[4rem_1fr]"><span className="text-muted-foreground">{t("Value")}</span><CopyValue value={r.value} /></div>
                  {r.note && <p className="text-xs text-muted-foreground">{t(r.note)}</p>}
                </div>
              ))}
              {!d.automatic && d.records.length === 1 && d.verified_at && <p className="text-xs text-muted-foreground">{t("S'Shop has been told and will add the routing record for you to copy here.")}</p>}
              <p className="text-xs text-muted-foreground">{t("DNS changes can take from a few minutes up to 48 hours to appear.")}</p>
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <ActionButton size="sm" variant="outline" online busy={check.isPending} busyLabel="Checking…" onAction={() => check.mutateAsync()}><RefreshCw /> {t("Check now")}</ActionButton>
            {d.status === "active" && <Button size="sm" variant="outline" asChild><a href={d.url} target="_blank" rel="noreferrer"><ExternalLink /> {t("Open")}</a></Button>}
            <Button size="sm" variant="ghost" onClick={() => setRemove(true)}><Trash2 /> {t("Remove domain")}</Button>
            {d.checked_at && <span className="ms-auto text-xs text-muted-foreground">{t("Checked")} {dateTime(d.checked_at)}</span>}
          </div>
        </Block>
      )}
      <ConfirmDialog open={remove} onOpenChange={setRemove} title="Remove this domain?" description="Your website stays available at its S'Shop address. You can connect a domain again any time."
        confirmLabel="Remove domain" destructive busy={del.isPending} onConfirm={() => del.mutate()} />
    </div>
  );
}

// ── SEO ───────────────────────────────────────────────────────────────

export function SeoTab({ c, set, ov }: TabProps) {
  const title = c.seo.title || c.brand.name;
  const desc = c.seo.description || c.brand.tagline;
  return (
    <div className="space-y-4">
      <Block title="Search engines & sharing">
        <Field label="Website title" hint={`${c.seo.title.length}/70`}><Input value={c.seo.title} maxLength={70} onChange={(e) => set((x) => { x.seo.title = e.target.value; })} /></Field>
        <Field label="Description" hint={`${c.seo.description.length}/160`}><Textarea value={c.seo.description} maxLength={160} onChange={(e) => set((x) => { x.seo.description = e.target.value; })} /></Field>
        <Field label="About the business" optional hint="Used for search engines"><Textarea value={c.seo.business_description} maxLength={500} onChange={(e) => set((x) => { x.seo.business_description = e.target.value; })} /></Field>
        <MediaField label="Sharing image" kind="banner" value={c.seo.share_image} onChange={(id) => set((x) => { x.seo.share_image = id; })} hint="Shown when your link is shared on WhatsApp or social media (empty = the hero image)." />
      </Block>
      <Block title="Search result preview">
        <div className="rounded-lg border p-3">
          <p className="truncate text-xs text-success">{ov.public_url}</p>
          <p className="truncate text-base text-primary">{title}</p>
          <p className="line-clamp-2 text-sm text-muted-foreground">{desc || t("Add a description so people know what you sell.")}</p>
        </div>
        <p className="text-xs text-muted-foreground">{t("Each product has its own page, title and description (Products tab). The sitemap updates itself:")}</p>
        <CopyValue value={`${ov.public_url}/sitemap.xml`} />
      </Block>
    </div>
  );
}

// ── Analytics ─────────────────────────────────────────────────────────

interface Analytics {
  visitors: number; visits: number; product_views: number; add_to_carts: number; order_starts: number; orders: number; completed_orders: number;
  conversion: number; completion: number; sales_value: string | null;
  daily: { day: string; visitors: number; orders: number }[];
  top_products: { id: string; name: string; views: number; add_to_carts: number }[];
}

export function AnalyticsTab() {
  const [period, setPeriod] = useState("30d");
  const { data, isLoading } = useQuery({ queryKey: ["website-analytics", period], queryFn: () => api<Analytics>("/website/analytics", { query: { period } }) });
  const { can } = useSession();
  const max = Math.max(1, ...(data?.daily.map((d) => d.visitors) ?? [1]));
  return (
    <div className="space-y-4">
      <Choice value={period} onChange={setPeriod} options={[["today", "Today"], ["7d", "7 days"], ["30d", "30 days"], ["90d", "90 days"]]} />
      {isLoading || !data ? <Loading /> : (
        <>
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <Stat label={"Visitors"} value={count(data.visitors)} />
            <Stat label={"Product views"} value={count(data.product_views)} />
            <Stat label={"Added to cart"} value={count(data.add_to_carts)} />
            <Stat label={"Checkouts started"} value={count(data.order_starts)} />
            <Stat label={"Orders"} value={count(data.orders)} />
            <Stat label={"Completed orders"} value={count(data.completed_orders)} />
            <Stat label={"Conversion"} value={`${data.conversion}%`} />
            {can("reports.view") && data.sales_value != null && <Stat label={"Completed sales"} value={amount(data.sales_value)} />}
          </div>
          <Block title="Visitors per day">
            <div className="flex h-32 items-end gap-[2px]" role="img" aria-label={t("Visitors per day")}>
              {data.daily.map((d) => (
                <div key={d.day} className="min-w-0 flex-1 rounded-t bg-primary/70" style={{ height: `${Math.max(2, (d.visitors / max) * 100)}%` }} title={`${d.day}: ${d.visitors} ${t("visitors")}, ${d.orders} ${t("orders")}`} />
              ))}
            </div>
          </Block>
          <Block title="Most viewed products">
            {!data.top_products.length ? <p className="text-sm text-muted-foreground">{t("No product views in this period.")}</p> : (
              <ul className="divide-y">
                {data.top_products.map((p) => (
                  <li key={p.id} className="flex items-center justify-between gap-3 py-2 text-sm">
                    <span className="truncate">{p.name}</span>
                    <span className="num shrink-0 text-muted-foreground">{p.views} {t("views")} · {p.add_to_carts} {t("carts")}</span>
                  </li>
                ))}
              </ul>
            )}
          </Block>
          <p className="text-xs text-muted-foreground">{t("Orders and completions come from Orders (source: website), so they always match. Visitors are anonymous.")}</p>
        </>
      )}
    </div>
  );
}
