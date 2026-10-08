/** Settings → Website: the Website Add-On (roadmap 51–57). Locked until the platform owner activates it; then the
 * Management Centre edits a draft, previews it on phone / tablet / desktop and publishes it (with history and rollback). */
import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, ExternalLink, Eye, Globe, History, Lock, Monitor, Smartphone, Tablet, Undo2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { date, dateTime, moneyDoc } from "@/lib/format";
import { STATUS_LABEL } from "@/lib/billing";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton, REASONS, isDirty } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ConfirmDialog, Field } from "@/components/Form";
import { ErrorState, Loading } from "@/components/Page";
import { Card, Fact } from "../shared";
import { t } from "@/lib/i18n";
import { useOverview, type Overview, type SiteConfig } from "./data";
import { ContentTab } from "./ContentTab";
import { DesignTab } from "./DesignTab";
import { CategoriesTab, ProductsTab } from "./ProductsTab";
import { ServicesTab, TestimonialsTab } from "./PeopleTabs";
import { MediaTab } from "./MediaTab";
import { AccessTab, AnalyticsTab, DomainTab, SeoTab } from "./ServiceTabs";

export interface TabProps {
  c: SiteConfig;
  set: (fn: (c: SiteConfig) => void) => void;
  ov: Overview;
}

const TABS: { key: string; label: string; perms: string[] }[] = [
  { key: "overview", label: "Overview", perms: [] },
  { key: "content", label: "Content", perms: ["website.content", "website.navigation"] },
  { key: "design", label: "Design", perms: ["website.design"] },
  { key: "products", label: "Products", perms: ["website.products", "website.photos"] },
  { key: "categories", label: "Categories", perms: ["website.categories"] },
  { key: "services", label: "Services", perms: ["website.services"] },
  { key: "testimonials", label: "Testimonials", perms: ["website.testimonials"] },
  { key: "media", label: "Media", perms: ["website.media"] },
  { key: "access", label: "Users & Access", perms: ["users.manage"] },
  { key: "domain", label: "Domain", perms: ["website.domain"] },
  { key: "seo", label: "SEO", perms: ["website.seo"] },
  { key: "analytics", label: "Analytics", perms: ["website.analytics"] },
];

export function WebsiteSettings() {
  const { data: ov, isLoading, error, refetch } = useOverview();
  if (isLoading) return <Loading />;
  if (error || !ov) return <ErrorState error={error} retry={refetch} />;
  return (
    <div className="space-y-5 pb-24">
      <div>
        <h2 className="text-xl font-semibold">{t("Website")}</h2>
        <p className="mt-1 text-sm text-muted-foreground">{t("Your own branded website and online shop, with products straight from S'Shop.")}</p>
      </div>
      {ov.status === "active" ? <Centre ov={ov} /> : <Locked ov={ov} />}
    </div>
  );
}

// ── Not active: request, pending, declined, disabled ──────────────────

function Locked({ ov }: { ov: Overview }) {
  const { can } = useSession();
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [message, setMessage] = useState("");
  const send = useMutation({
    mutationFn: () => api("/website/request", { body: { message } }),
    onSuccess: () => {
      toast.success("Request sent to S'Shop");
      setOpen(false);
      qc.invalidateQueries({ queryKey: ["website"] });
    },
    onError: (e) => toast.error(e),
  });
  const features = [
    "Branded home page, about, services, testimonials and contact pages",
    "Your S'Shop products, prices and photos — always up to date",
    "Online ordering into your Orders, with order tracking",
    "Your own colours, fonts and layout; light and dark themes",
    "Your own domain, search engine settings and visitor analytics",
  ];
  return (
    <Card>
      <div className="space-y-4 py-3">
        <div className="flex items-center gap-3">
          <span className="flex h-11 w-11 items-center justify-center rounded-xl bg-muted"><Lock className="h-5 w-5" /></span>
          <div>
            <p className="font-semibold">{t("Website Add-On")}</p>
            <p className="text-sm text-muted-foreground">
              {ov.status === "requested" ? `${t("Request sent")} ${dateTime(ov.requested_at)} — ${t("S'Shop will review it and contact you.")}`
                : ov.status === "disabled" ? t("The website service is disabled. Your content, products, media and domain are kept.")
                  : t("Locked — request the service and S'Shop will set it up for you.")}
            </p>
          </div>
        </div>
        {ov.status_reason && (ov.status === "declined" || ov.status === "disabled") && (
          <p className="rounded-lg bg-muted/60 p-3 text-sm">{ov.status === "declined" ? t("Declined:") : t("Reason:")} {ov.status_reason}</p>
        )}
        <ul className="grid gap-1.5 text-sm text-muted-foreground sm:grid-cols-2">{features.map((f) => <li key={f}>✓ {t(f)}</li>)}</ul>
        {(ov.status === "none" || ov.status === "declined") && (
          <ActionButton online perm="settings.integrations" onClick={() => setOpen(true)}>{ov.status === "declined" ? "Request again" : "Request Website Service"}</ActionButton>
        )}
        {!can("settings.integrations") && ov.status === "none" && <p className="text-xs text-muted-foreground">{t("Ask your administrator to request the website service.")}</p>}
      </div>
      <ResponsiveDialog open={open} onOpenChange={setOpen} title="Request Website Service" description="S'Shop is notified by email and in the app, and will contact you about set-up and billing."
        footer={<ActionButton online busy={send.isPending} busyLabel="Sending…" onAction={() => send.mutateAsync()}>Send request</ActionButton>}>
        <Field label="Message" optional><Textarea value={message} onChange={(e) => setMessage(e.target.value)} maxLength={1000} placeholder={t("What would you like your website to do?")} /></Field>
      </ResponsiveDialog>
    </Card>
  );
}

// ── Active: the Management Centre ─────────────────────────────────────

function Centre({ ov }: { ov: Overview }) {
  const { can } = useSession();
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const tabs = TABS.filter((x) => !x.perms.length || x.perms.some((p) => can(p)));
  const tab = tabs.find((x) => x.key === params.get("tab"))?.key ?? "overview";
  const [draft, setDraft] = useState<SiteConfig | null>(null);
  const [base, setBase] = useState<string | null | undefined>(null);
  // Load the saved draft; never overwrite local edits in progress.
  useEffect(() => {
    if (!ov.draft) return;
    setDraft((d) => (d && base === ov.draft_updated_at ? d : structuredClone(ov.draft!)));
    setBase(ov.draft_updated_at);
  }, [ov.draft, ov.draft_updated_at]); // eslint-disable-line react-hooks/exhaustive-deps
  const dirty = !!draft && !!ov.draft && isDirty(ov.draft, draft);
  const set = (fn: (c: SiteConfig) => void) => setDraft((d) => {
    if (!d) return d;
    const next = structuredClone(d);
    fn(next);
    return next;
  });
  const refresh = () => qc.invalidateQueries({ queryKey: ["website"] });

  const save = useMutation({
    mutationFn: () => api<{ ok: boolean; draft_updated_at: string }>("/website/draft", { method: "PUT", body: { config: draft, base_updated_at: base } }),
    onSuccess: (r) => {
      setBase(r.draft_updated_at);
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const [publishOpen, setPublishOpen] = useState(false);
  const [note, setNote] = useState("");
  const publish = useMutation({
    mutationFn: async () => {
      if (dirty) await save.mutateAsync();
      return api<{ version: number }>("/website/publish", { body: { note } });
    },
    onSuccess: (r) => {
      toast.success(`${t("Published")} — v${r.version}`);
      setPublishOpen(false);
      setNote("");
      refresh();
      qc.invalidateQueries({ queryKey: ["website-versions"] });
    },
    onError: (e) => toast.error(e),
  });
  const [discardOpen, setDiscardOpen] = useState(false);
  const discard = useMutation({
    mutationFn: () => api("/website/discard", { body: {} }),
    onSuccess: () => {
      setDraft(null);
      setDiscardOpen(false);
      toast.success("Draft discarded");
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const [preview, setPreview] = useState(false);
  const [history, setHistory] = useState(false);
  const openPreview = async () => {
    if (dirty) {
      try {
        await save.mutateAsync();
      } catch {
        return;
      }
    }
    setPreview(true);
  };
  const pending = dirty || !!ov.has_unpublished;
  const canPublish = can("website.publish");

  if (!draft) return <Loading />;
  const props: TabProps = { c: draft, set, ov };
  return (
    <>
      <nav className="scrollbar-none -mx-1 flex gap-1.5 overflow-x-auto px-1 pb-1" aria-label={t("Website sections")}>
        {tabs.map((x) => (
          <button key={x.key} type="button" onClick={() => setParams(x.key === "overview" ? {} : { tab: x.key }, { replace: true })}
            className={cn("h-8 shrink-0 rounded-full border px-3 text-sm", tab === x.key ? "border-primary bg-primary text-primary-foreground" : "bg-card hover:bg-accent/50")}>
            {t(x.label)}
          </button>
        ))}
      </nav>

      {tab === "overview" && <OverviewTab ov={ov} pending={pending} onPreview={openPreview} onHistory={() => setHistory(true)} />}
      {tab === "content" && <ContentTab {...props} />}
      {tab === "design" && <DesignTab {...props} />}
      {tab === "products" && <ProductsTab {...props} />}
      {tab === "categories" && <CategoriesTab {...props} />}
      {tab === "services" && <ServicesTab {...props} />}
      {tab === "testimonials" && <TestimonialsTab {...props} />}
      {tab === "media" && <MediaTab />}
      {tab === "access" && <AccessTab />}
      {tab === "domain" && <DomainTab ov={ov} />}
      {tab === "seo" && <SeoTab {...props} />}
      {tab === "analytics" && <AnalyticsTab />}

      {/* Draft → Preview → Publish */}
      <div className="fixed inset-x-0 bottom-above-nav z-20 border-t bg-background/95 p-2.5 backdrop-blur lg:bottom-0 lg:start-sidebar">
        <div className="mx-auto flex max-w-[1680px] flex-wrap items-center justify-end gap-2 px-1 md:px-3 lg:px-5">
          <span className="me-auto text-sm text-muted-foreground">
            {dirty ? t("Unsaved changes") : ov.has_unpublished ? t("Draft saved — not published yet") : t("Everything is published")}
          </span>
          {(dirty || ov.has_unpublished) && (
            <Button variant="ghost" size="sm" onClick={() => (dirty && !ov.has_unpublished ? setDraft(structuredClone(ov.draft!)) : setDiscardOpen(true))}><Undo2 /> {t("Discard")}</Button>
          )}
          <Button variant="outline" size="sm" onClick={openPreview} disabled={save.isPending}><Eye /> {t("Preview")}</Button>
          {dirty && <ActionButton size="sm" variant="outline" online busy={save.isPending} busyLabel="Saving…" doneLabel="Saved" onAction={() => save.mutateAsync()}>Save draft</ActionButton>}
          {canPublish && <ActionButton size="sm" online blockedBy={[!pending && REASONS.noChanges]} onClick={() => setPublishOpen(true)}>Publish</ActionButton>}
        </div>
      </div>

      <PreviewDialog open={preview} onOpenChange={setPreview} slug={ov.slug!} />
      <HistoryDialog open={history} onOpenChange={setHistory} canRestore={canPublish} onRestored={() => { setDraft(null); refresh(); }} />
      <ResponsiveDialog open={publishOpen} onOpenChange={setPublishOpen} title="Publish website" description="Visitors see the new version straight away. Earlier versions stay in the history."
        footer={<ActionButton online busy={publish.isPending} busyLabel="Publishing…" onAction={() => publish.mutateAsync()}>Publish now</ActionButton>}>
        <Field label="What changed?" optional><Input value={note} onChange={(e) => setNote(e.target.value)} maxLength={200} placeholder={t("New arrivals and updated prices")} /></Field>
      </ResponsiveDialog>
      <ConfirmDialog open={discardOpen} onOpenChange={setDiscardOpen} title="Discard the draft?" description="All changes since the last publication are removed. The published website is not affected."
        confirmLabel="Discard draft" destructive busy={discard.isPending} onConfirm={() => discard.mutate()} />
    </>
  );
}

function OverviewTab({ ov, pending, onPreview, onHistory }: { ov: Overview; pending: boolean; onPreview: () => void; onHistory: () => void }) {
  const url = ov.public_url ?? "";
  const status = ov.billing_suspended ? { label: "Billing suspended", cls: "bg-destructive/10 text-destructive" }
    : ov.live ? { label: "Live", cls: "bg-success/15 text-success" } : { label: "Not published", cls: "bg-muted text-muted-foreground" };
  return (
    <>
      <Card title="Your website" action={<span className={cn("rounded-full px-2.5 py-0.5 text-xs font-medium", status.cls)}>{t(status.label)}</span>}>
        <div className="space-y-3 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <Globe className="h-4 w-4 text-muted-foreground" />
            <code className="num min-w-0 flex-1 truncate rounded-lg bg-muted px-3 py-2 text-sm">{url}</code>
            <Button variant="outline" size="sm" onClick={() => { navigator.clipboard.writeText(url); toast.success("Link copied"); }}><Copy /> {t("Copy")}</Button>
            {ov.live && <Button variant="outline" size="sm" asChild><a href={url} target="_blank" rel="noreferrer"><ExternalLink /> {t("Open")}</a></Button>}
          </div>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Fact label={t("Version")}>{ov.version ? `v${ov.version}` : "—"}</Fact>
            <Fact label={t("Published")}>{ov.published_at ? dateTime(ov.published_at) : t("Never")}</Fact>
            <Fact label={t("Published by")}>{ov.published_by ?? "—"}</Fact>
            <Fact label={t("Draft saved")}>{ov.draft_updated_at ? `${dateTime(ov.draft_updated_at)}${ov.draft_updated_by ? ` · ${ov.draft_updated_by}` : ""}` : "—"}</Fact>
          </div>
          {ov.billing_suspended && <p className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive">{t("The website shows “temporarily unavailable” until the website invoice is settled. Your POS and everything else keep working.")}</p>}
          {pending && <p className="rounded-lg bg-warning/10 p-3 text-sm">{t("You have changes that are not published yet — preview them, then publish.")}</p>}
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" onClick={onPreview}><Eye /> {t("Preview")}</Button>
            <Button variant="outline" size="sm" onClick={onHistory}><History /> {t("Version history")}</Button>
          </div>
        </div>
      </Card>
      <Card title="Billing">
        <div className="grid grid-cols-2 gap-3 py-3 sm:grid-cols-3">
          <Fact label={t("Status")}>{t(STATUS_LABEL[ov.billing.status] ?? ov.billing.status)}</Fact>
          {ov.billing.status !== "platform_owned" && <Fact label={t("Outstanding")}>{moneyDoc(ov.billing.outstanding, ov.billing.currency)}</Fact>}
          {ov.billing.next_due && <Fact label={t("Next due")}>{date(ov.billing.next_due)}</Fact>}
        </div>
        <p className="py-2 text-xs text-muted-foreground">{t("Website invoices appear in Settings → Billing.")}</p>
      </Card>
    </>
  );
}

const DEVICES = [
  { key: "mobile", icon: Smartphone, w: 390, label: "Phone" },
  { key: "tablet", icon: Tablet, w: 820, label: "Tablet" },
  { key: "desktop", icon: Monitor, w: 1280, label: "Desktop" },
] as const;

function PreviewDialog({ open, onOpenChange, slug }: { open: boolean; onOpenChange: (o: boolean) => void; slug: string }) {
  const [device, setDevice] = useState<(typeof DEVICES)[number]["key"]>("mobile");
  const [stamp, setStamp] = useState(0);
  useEffect(() => {
    if (open) setStamp(Date.now());
  }, [open]);
  const d = DEVICES.find((x) => x.key === device)!;
  const [box, setBox] = useState(600);
  const scale = Math.min(1, box / d.w);
  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} wide title="Preview" description="Your saved draft, exactly as visitors will see it after publishing.">
      <div className="space-y-3">
        <div className="flex justify-center gap-1.5">
          {DEVICES.map((x) => (
            <Button key={x.key} size="sm" variant={device === x.key ? "default" : "outline"} onClick={() => setDevice(x.key)} aria-pressed={device === x.key}><x.icon /> {t(x.label)}</Button>
          ))}
        </div>
        <div ref={(el) => { if (el && el.clientWidth !== box) setBox(el.clientWidth); }} className="overflow-hidden rounded-lg border bg-muted" style={{ height: 620 }}>
          {open && (
            <iframe title={t("Website preview")} src={`/s/${slug}?preview=1&t=${stamp}`} className="origin-top-left border-0 bg-white"
              style={{ width: d.w, height: 620 / scale, transform: `scale(${scale})`, marginInline: scale === 1 ? "auto" : undefined, display: "block" }} />
          )}
        </div>
      </div>
    </ResponsiveDialog>
  );
}

function HistoryDialog({ open, onOpenChange, canRestore, onRestored }: { open: boolean; onOpenChange: (o: boolean) => void; canRestore: boolean; onRestored: () => void }) {
  const { data } = useQuery({
    queryKey: ["website-versions"],
    queryFn: () => api<{ items: { version: number; published_at: string; published_by: string | null; note: string }[] }>("/website/versions"),
    enabled: open,
  });
  const [confirm, setConfirm] = useState<number | null>(null);
  const restore = useMutation({
    mutationFn: (v: number) => api(`/website/versions/${v}/restore`, { body: {} }),
    onSuccess: () => {
      toast.success("Version restored and published");
      setConfirm(null);
      onRestored();
    },
    onError: (e) => toast.error(e),
  });
  const items = useMemo(() => data?.items ?? [], [data]);
  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} title="Version history">
      {!items.length ? <p className="py-6 text-center text-sm text-muted-foreground">{t("Nothing published yet.")}</p> : (
        <ul className="divide-y">
          {items.map((v, i) => (
            <li key={v.version} className="flex items-center gap-3 py-2.5">
              <span className="num w-10 text-sm font-semibold">v{v.version}</span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm">{v.note || t("Published")}</p>
                <p className="text-xs text-muted-foreground">{dateTime(v.published_at)}{v.published_by && ` · ${v.published_by}`}</p>
              </div>
              {i === 0 ? <span className="text-xs text-success">{t("Live")}</span> : canRestore && <Button size="sm" variant="outline" onClick={() => setConfirm(v.version)}>{t("Restore")}</Button>}
            </li>
          ))}
        </ul>
      )}
      <ConfirmDialog open={confirm !== null} onOpenChange={(o) => !o && setConfirm(null)} title={`${t("Restore version")} ${confirm ?? ""}?`}
        description="It is published again as a new version, so nothing is lost. Unpublished draft changes are replaced."
        confirmLabel="Restore & publish" busy={restore.isPending} onConfirm={() => confirm !== null && restore.mutate(confirm)} />
    </ResponsiveDialog>
  );
}
