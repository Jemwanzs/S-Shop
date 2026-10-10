/** Website → Holiday & promotions (roadmap 84–86): switch the feature on, create campaigns with ready-written greetings,
 * occasion designs and featured products, preview them on mobile / tablet / desktop, schedule, publish, unpublish,
 * duplicate, archive, and compare their insights. One campaign shows at a time (priority, then the latest start). */
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Archive, CalendarClock, Copy, Eye, Megaphone, Pencil, Plus, Send, Trash2, Undo2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { money } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { CampaignDesign } from "@/site/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Checkbox } from "@/components/ui/checkbox";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ConfirmDialog, Field, Select, ToggleRow } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { Block, Choice, Grid2, MediaField } from "./kit";
import { useCatalogue, type Overview } from "./data";
import { PreviewDialog } from "./Website";

interface Products { mode: "auto" | "manual" | "none"; product_ids: string[]; period_days: number; category_id: string | null; limit: number }
interface Placement { pages: string[]; display: "hero" | "compact" | "strip" | "card"; dismissible: boolean }
interface Insights { views: number; viewers: number; product_clicks: number; cta_clicks: number; orders: number; sales: string }
interface CampaignRow {
  id: string; name: string; occasion: string; status: "draft" | "scheduled" | "live" | "expired" | "archived"; priority: number;
  starts_local: string; ends_local: string; design: CampaignDesign; products: Products; placement: Placement; version: number; insights: Insights;
}
interface ListResponse { enabled: boolean; platform: { enabled: boolean; max_campaigns: number; max_products: number }; items: CampaignRow[]; showing: string | null; timezone: string }

const STATUS: Record<CampaignRow["status"], [string, Tone]> = {
  draft: ["Draft", "neutral"], scheduled: ["Scheduled", "info"], live: ["Live", "success"], expired: ["Expired", "warning"], archived: ["Archived", "neutral"],
};

const OCCASIONS: [string, string, string, string, string][] = [
  // key, label, template, headline, message
  ["christmas", "Christmas", "christmas", "Wishing You a Merry Christmas & a Wonderful New Year!", "Celebrate the season with something special. Discover our festive favourites, carefully selected just for you."],
  ["new_year", "New Year", "new_year", "Happy New Year!", "Thank you for a wonderful year. Start the new one with something you love."],
  ["valentines", "Valentine's Day", "valentines", "Celebrate Love with Something Unforgettable!", "Make every moment memorable with our most-loved gifts and special selections."],
  ["easter", "Easter", "easter", "Happy Easter!", "Wishing you a joyful season with family and friends — and a few treats from us."],
  ["eid", "Eid", "eid", "Eid Mubarak!", "Wishing you and your loved ones peace, happiness and a beautiful celebration."],
  ["diwali", "Diwali", "celebration", "Happy Diwali!", "May the festival of lights bring joy and prosperity to you and your family."],
  ["mothers_day", "Mother's Day", "valentines", "Happy Mother's Day!", "Celebrate the women who mean the world — with gifts as special as they are."],
  ["fathers_day", "Father's Day", "elegant", "Happy Father's Day!", "Thoughtful gifts for the men who are always there for us."],
  ["black_friday", "Black Friday", "sale", "Black Friday Is Here", "Our best offers of the year, for a limited time only."],
  ["cyber_monday", "Cyber Monday", "sale", "Cyber Monday Deals", "Shop online today and grab our favourite picks."],
  ["national", "National holiday", "celebration", "Happy Holiday!", "Celebrating together with you. Wishing you a wonderful day."],
  ["anniversary", "Business anniversary", "elegant", "Thank You for Celebrating With Us!", "Another year of serving you — here's to many more."],
  ["appreciation", "Customer appreciation", "celebration", "Thank You, Valued Customers!", "Your support means everything. Enjoy something special from us."],
  ["back_to_school", "Back to school", "celebration", "Back to School!", "Everything you need for a great start to the new term."],
  ["custom", "Custom occasion", "minimal", "Something Special for You", "Discover our latest selection, picked with care."],
];
const TEMPLATES: [string, string][] = [["christmas", "Christmas"], ["new_year", "New Year"], ["valentines", "Hearts"], ["easter", "Pastel"], ["eid", "Crescent & lanterns"],
  ["celebration", "Celebration"], ["sale", "Sale"], ["elegant", "Elegant gold"], ["minimal", "Minimal"]];
const PAGES: [string, string][] = [["home", "Home"], ["products", "Products"], ["categories", "Categories"], ["all", "All pages"]];
const TARGETS: [string, string][] = [["products", "Products"], ["home", "Home"], ["categories", "Categories"], ["services", "Services"], ["contact", "Contact"], ["order", "Order"]];

const pad = (n: number) => String(n).padStart(2, "0");
const localInput = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;

function blank(): Omit<CampaignRow, "id" | "status" | "version" | "insights"> {
  const [key, , template, headline, message] = OCCASIONS[0];
  const start = new Date();
  start.setMinutes(0, 0, 0);
  const end = new Date(start.getTime() + 14 * 86_400_000);
  return {
    name: "", occasion: key, priority: 0, starts_local: localInput(start), ends_local: localInput(end),
    design: { template, headline, message, promo: "", cta_label: "Shop now", cta_target: "products", align: "center", headline_size: "lg", text_color: "", background: "",
      background_image: null, decorations: true, animation: "subtle", height: "standard", button_style: "solid" },
    products: { mode: "auto", product_ids: [], period_days: 90, category_id: null, limit: 5 },
    placement: { pages: ["home", "products"], display: "hero", dismissible: true },
  };
}

export function CampaignsTab({ ov }: { ov: Overview }) {
  const qc = useQueryClient();
  const { can } = useSession();
  const q = useQuery({ queryKey: ["website-campaigns"], queryFn: () => api<ListResponse>("/website/campaigns") });
  const [edit, setEdit] = useState<(Omit<CampaignRow, "id" | "status" | "version" | "insights"> & { id?: string; version?: number }) | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [remove, setRemove] = useState<CampaignRow | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["website-campaigns"] });
  const act = useMutation({
    mutationFn: ({ id, action }: { id: string; action: string }) => api(`/website/campaigns/${id}/${action}`, { method: "POST" }),
    onSuccess: (_r, v) => { toast.success({ publish: "Campaign published", unpublish: "Campaign unpublished", archive: "Campaign archived", duplicate: "Copy created as a draft" }[v.action] ?? "Saved"); refresh(); },
    onError: (e) => toast.error(e),
  });
  const toggle = useMutation({
    mutationFn: (enabled: boolean) => api("/website/campaigns/settings", { method: "PUT", body: { enabled } }),
    onSuccess: refresh,
    onError: (e) => toast.error(e),
  });
  const del = useMutation({
    mutationFn: (id: string) => api(`/website/campaigns/${id}`, { method: "DELETE" }),
    onSuccess: () => { setRemove(null); toast.success("Draft deleted"); refresh(); },
    onError: (e) => toast.error(e),
  });
  if (q.isLoading || !q.data) return <Loading />;
  const d = q.data;
  const canEdit = can("website.content");
  const canPublish = can("website.publish");
  const active = d.items.filter((c) => c.status !== "archived");
  const archived = d.items.filter((c) => c.status === "archived");
  return (
    <div className="space-y-4">
      <Block title="Holiday & promotional banners" hint="Seasonal greetings and featured products on your website. One campaign shows at a time.">
        {!d.platform.enabled ? (
          <p className="text-sm text-muted-foreground">{t("Holiday & promotional banners are switched off on S'Shop")}</p>
        ) : (
          <ToggleRow label="Show campaigns on my website" hint={d.enabled ? (d.showing ? `${t("Showing now")}: ${d.items.find((c) => c.id === d.showing)?.name ?? ""}` : t("No campaign is live right now")) : t("Off: nothing shows, but you can prepare campaigns as drafts.")}
            checked={d.enabled} disabled={!canPublish || toggle.isPending} onChange={(v) => toggle.mutate(v)} />
        )}
      </Block>

      <Block title="Campaigns" hint={`${t("Times are in your business timezone")} (${d.timezone}). ${active.length}/${d.platform.max_campaigns}`}
        action={canEdit && <Button size="sm" onClick={() => setEdit(blank())}><Plus /> {t("New campaign")}</Button>}>
        {active.length === 0 && <p className="py-2 text-sm text-muted-foreground">{t("No campaigns yet. Start with a ready-written greeting for the next holiday.")}</p>}
        <div className="divide-y">
          {active.map((c) => (
            <CampaignItem key={c.id} c={c} showing={d.showing === c.id} canEdit={canEdit} canPublish={canPublish} busy={act.isPending}
              onEdit={() => setEdit({ ...c })} onPreview={() => setPreview(c.id)} onAction={(action) => act.mutate({ id: c.id, action })} onDelete={() => setRemove(c)} />
          ))}
        </div>
      </Block>

      {archived.length > 0 && (
        <Block title="Archived" hint="Kept with their results; duplicate one to reuse it.">
          <div className="divide-y">
            {archived.map((c) => (
              <CampaignItem key={c.id} c={c} showing={false} canEdit={canEdit} canPublish={false} busy={act.isPending}
                onEdit={() => undefined} onPreview={() => setPreview(c.id)} onAction={(action) => act.mutate({ id: c.id, action })} onDelete={() => undefined} />
            ))}
          </div>
        </Block>
      )}

      {edit && <Editor value={edit} maxProducts={d.platform.max_products} onClose={() => setEdit(null)} onSaved={(id) => { setEdit(null); refresh(); if (id) setPreview(id); }} />}
      {ov.slug && <PreviewDialog open={!!preview} onOpenChange={(o) => !o && setPreview(null)} slug={ov.slug} query={preview ? `campaign=${preview}` : undefined} />}
      <ConfirmDialog open={!!remove} onOpenChange={(o) => !o && setRemove(null)} title="Delete this draft?" description="It has never been published, so nothing else is affected."
        confirmLabel="Delete" destructive busy={del.isPending} onConfirm={() => remove && del.mutate(remove.id)} />
    </div>
  );
}

function CampaignItem({ c, showing, canEdit, canPublish, busy, onEdit, onPreview, onAction, onDelete }: {
  c: CampaignRow; showing: boolean; canEdit: boolean; canPublish: boolean; busy: boolean;
  onEdit: () => void; onPreview: () => void; onAction: (a: string) => void; onDelete: () => void;
}) {
  const [label, tone] = STATUS[c.status];
  const occasion = OCCASIONS.find((o) => o[0] === c.occasion)?.[1] ?? c.occasion;
  const i = c.insights;
  const archived = c.status === "archived";
  return (
    <div className="space-y-2 py-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <Megaphone className="h-4 w-4 text-primary" />
        <span className="font-medium">{c.name}</span>
        <Pill tone={tone}>{t(label)}</Pill>
        {showing && <Pill tone="primary">{t("Showing now")}</Pill>}
        <span className="text-xs text-muted-foreground">{t(occasion)}{c.priority ? ` · ${t("priority")} ${c.priority}` : ""}</span>
      </div>
      <p className="flex items-center gap-1.5 text-xs text-muted-foreground"><CalendarClock className="h-3.5 w-3.5" /> <span className="num">{c.starts_local.replace("T", " ")} → {c.ends_local.replace("T", " ")}</span></p>
      <div className="num grid grid-cols-3 gap-2 text-xs sm:grid-cols-6">
        {([["Views", i.views], ["Visitors", i.viewers], ["Product clicks", i.product_clicks], ["Button clicks", i.cta_clicks], ["Orders", i.orders], ["Sales", null]] as const).map(([l, v]) => (
          <div key={l} className="rounded-lg bg-muted/50 px-2 py-1.5"><p className="text-muted-foreground">{t(l)}</p><p className="font-semibold">{v === null ? money(i.sales) : v}</p></div>
        ))}
      </div>
      <div className="flex flex-wrap gap-1.5">
        <Button size="sm" variant="outline" onClick={onPreview}><Eye /> {t("Preview")}</Button>
        {!archived && canEdit && (c.status === "draft" || canPublish) && <Button size="sm" variant="outline" onClick={onEdit}><Pencil /> {t("Edit")}</Button>}
        {!archived && canPublish && c.status === "draft" && <Button size="sm" disabled={busy} onClick={() => onAction("publish")}><Send /> {t("Publish")}</Button>}
        {!archived && canPublish && (c.status === "scheduled" || c.status === "live") && <Button size="sm" variant="outline" disabled={busy} onClick={() => onAction("unpublish")}><Undo2 /> {t("Unpublish")}</Button>}
        {canEdit && <Button size="sm" variant="ghost" disabled={busy} onClick={() => onAction("duplicate")}><Copy /> {t("Duplicate")}</Button>}
        {!archived && canPublish && c.status !== "draft" && <Button size="sm" variant="ghost" disabled={busy} onClick={() => onAction("archive")}><Archive /> {t("Archive")}</Button>}
        {!archived && canEdit && c.status === "draft" && <Button size="sm" variant="ghost" className="text-destructive" onClick={onDelete}><Trash2 /> {t("Delete")}</Button>}
      </div>
    </div>
  );
}

type EditValue = Omit<CampaignRow, "id" | "status" | "version" | "insights"> & { id?: string; version?: number };

function Editor({ value, maxProducts, onClose, onSaved }: { value: EditValue; maxProducts: number; onClose: () => void; onSaved: (previewId?: string) => void }) {
  const [c, setC] = useState<EditValue>(value);
  const [search, setSearch] = useState("");
  const cat = useCatalogue();
  const set = (fn: (x: EditValue) => void) => setC((prev) => { const next = structuredClone(prev); fn(next); return next; });
  const d = c.design;
  const chooseOccasion = (key: string) => set((x) => {
    const was = OCCASIONS.find((o) => o[0] === x.occasion);
    const o = OCCASIONS.find((k) => k[0] === key)!;
    // Replace the sample texts only if the business has not written its own.
    if (!x.design.headline || x.design.headline === was?.[3]) x.design.headline = o[3];
    if (!x.design.message || x.design.message === was?.[4]) x.design.message = o[4];
    if (!x.name || x.name === was?.[1]) x.name = o[1];
    x.design.template = o[2];
    x.occasion = key;
  });
  const products = useMemo(() => (cat.data?.products ?? []).filter((p) => p.is_active && (!search || p.name.toLowerCase().includes(search.toLowerCase()))).slice(0, 40), [cat.data, search]);
  const save = async (andPreview: boolean) => {
    const body = { ...c, version: c.version };
    const r = c.id
      ? await api<{ ok: boolean }>(`/website/campaigns/${c.id}`, { method: "PUT", body }).then(() => ({ id: c.id! }))
      : await api<{ id: string }>("/website/campaigns", { body });
    toast.success("Campaign saved");
    onSaved(andPreview ? r.id : undefined);
  };
  const problems = [!c.name.trim() && "Name the campaign", !d.headline.trim() && "Write the greeting", c.ends_local <= c.starts_local && "The end must be after the start",
    c.products.mode === "manual" && !c.products.product_ids.length && "Choose products", !c.placement.pages.length && "Choose where it shows"];
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} wide title={c.id ? "Edit campaign" : "New campaign"}
      description="Saved campaigns are drafts until someone who can publish publishes them."
      footer={<div className="flex flex-wrap justify-end gap-2">
        <ActionButton variant="outline" online blockedBy={problems} onAction={() => save(true)}><Eye /> {t("Save & preview")}</ActionButton>
        <ActionButton online blockedBy={problems} onAction={() => save(false)}>{t("Save draft")}</ActionButton>
      </div>}>
      <div className="space-y-4">
        <Grid2>
          <Field label="Occasion">
            <Select value={c.occasion} onChange={chooseOccasion}>{OCCASIONS.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}</Select>
          </Field>
          <Field label="Campaign name"><Input value={c.name} maxLength={80} onChange={(e) => set((x) => { x.name = e.target.value; })} /></Field>
          <Field label="Starts"><Input type="datetime-local" value={c.starts_local} onChange={(e) => set((x) => { x.starts_local = e.target.value; })} /></Field>
          <Field label="Ends"><Input type="datetime-local" value={c.ends_local} onChange={(e) => set((x) => { x.ends_local = e.target.value; })} /></Field>
        </Grid2>

        <Block title="Greeting">
          <Field label="Headline"><Input value={d.headline} maxLength={90} onChange={(e) => set((x) => { x.design.headline = e.target.value; })} /></Field>
          <Field label="Message" optional><Textarea rows={2} value={d.message} maxLength={260} onChange={(e) => set((x) => { x.design.message = e.target.value; })} /></Field>
          <Field label="Promotional line" optional hint="e.g. Up to 20% off selected gifts — only what you actually offer"><Input value={d.promo} maxLength={120} onChange={(e) => set((x) => { x.design.promo = e.target.value; })} /></Field>
          <Grid2>
            <Field label="Button text" optional><Input value={d.cta_label} maxLength={30} onChange={(e) => set((x) => { x.design.cta_label = e.target.value; })} /></Field>
            <Field label="Button goes to">
              <Select value={TARGETS.some(([k]) => k === d.cta_target) ? d.cta_target : "products"} onChange={(v) => set((x) => { x.design.cta_target = v; })}>
                {TARGETS.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}
              </Select>
            </Field>
          </Grid2>
        </Block>

        <Block title="Design">
          <Field label="Banner design"><Choice value={d.template} onChange={(v) => set((x) => { x.design.template = v; })} options={TEMPLATES} /></Field>
          <Grid2>
            <Field label="Text"><Choice value={d.align} onChange={(v) => set((x) => { x.design.align = v; })} options={[["center", "Centred"], ["left", "Left"]]} /></Field>
            <Field label="Headline size"><Choice value={d.headline_size} onChange={(v) => set((x) => { x.design.headline_size = v; })} options={[["md", "Medium"], ["lg", "Large"], ["xl", "Extra large"]]} /></Field>
            <Field label="Height"><Choice value={d.height} onChange={(v) => set((x) => { x.design.height = v; })} options={[["compact", "Compact"], ["standard", "Standard"], ["tall", "Tall"]]} /></Field>
            <Field label="Button"><Choice value={d.button_style} onChange={(v) => set((x) => { x.design.button_style = v; })} options={[["solid", "Solid"], ["outline", "Outline"]]} /></Field>
          </Grid2>
          <Grid2>
            <ColourField label="Background colour" value={d.background} onChange={(v) => set((x) => { x.design.background = v; })} />
            <ColourField label="Text colour" value={d.text_color} onChange={(v) => set((x) => { x.design.text_color = v; })} />
          </Grid2>
          <MediaField label="Background image" kind="banner" value={d.background_image} onChange={(id) => set((x) => { x.design.background_image = id; })} hint="Optional; darkened slightly so the greeting stays readable." />
          <ToggleRow label="Decorations" hint="Quiet occasion motifs (snowflakes, hearts, crescents, confetti …)" checked={d.decorations} onChange={(v) => set((x) => { x.design.decorations = v; })} />
          <ToggleRow label="Gentle animation" hint="Always still for visitors who ask for reduced motion" checked={d.animation === "subtle"} onChange={(v) => set((x) => { x.design.animation = v ? "subtle" : "off"; })} />
        </Block>

        <Block title="Featured products" hint="Prices, stock and ordering follow your website settings.">
          <Choice value={c.products.mode} onChange={(v) => set((x) => { x.products.mode = v; })} options={[["auto", "Best sellers"], ["manual", "I choose"], ["none", "None"]]} />
          {c.products.mode === "auto" && (
            <Grid2>
              <Field label="Best sellers over"><Choice value={c.products.period_days} onChange={(v) => set((x) => { x.products.period_days = v; })} options={[[30, "30 days"], [90, "90 days"], [365, "A year"], [0, "All time"]]} /></Field>
              <Field label="Category" optional>
                <Select value={c.products.category_id ?? ""} onChange={(v) => set((x) => { x.products.category_id = v || null; })}>
                  <option value="">{t("All categories")}</option>
                  {cat.data?.categories.filter((k) => k.is_active).map((k) => <option key={k.id} value={k.id}>{k.name}</option>)}
                </Select>
              </Field>
            </Grid2>
          )}
          {c.products.mode !== "none" && (
            <Field label="How many" hint={`1–${maxProducts}`}>
              <Input type="number" min={1} max={maxProducts} className="w-24" value={c.products.limit} onChange={(e) => set((x) => { x.products.limit = Math.min(maxProducts, Math.max(1, Number(e.target.value) || 1)); })} />
            </Field>
          )}
          {c.products.mode === "auto" && <p className="text-xs text-muted-foreground">{t("Ranked by units actually sold (returns deducted). No sales in the period: all-time sales; no sales at all: the banner shows the greeting only.")}</p>}
          {c.products.mode === "manual" && (
            <div className="space-y-2">
              <Input placeholder={t("Search products")} value={search} onChange={(e) => setSearch(e.target.value)} />
              <div className="max-h-56 space-y-1 overflow-y-auto rounded-lg border p-2">
                {products.map((p) => {
                  const on = c.products.product_ids.includes(p.id);
                  return (
                    <label key={p.id} className={cn("flex items-center gap-2 rounded-md px-1.5 py-1 text-sm", !on && c.products.product_ids.length >= c.products.limit && "opacity-50")}>
                      <Checkbox checked={on} disabled={!on && c.products.product_ids.length >= c.products.limit}
                        onCheckedChange={(v) => set((x) => { x.products.product_ids = v ? [...x.products.product_ids, p.id] : x.products.product_ids.filter((id) => id !== p.id); })} />
                      <span className="min-w-0 flex-1 truncate">{p.name}</span>
                      <span className="num text-xs text-muted-foreground">{money(p.price)}</span>
                    </label>
                  );
                })}
              </div>
              <p className="text-xs text-muted-foreground">{c.products.product_ids.length}/{c.products.limit} {t("chosen — only products published on your website appear.")}</p>
            </div>
          )}
        </Block>

        <Block title="Where it shows">
          <div className="flex flex-wrap gap-3">
            {PAGES.map(([k, l]) => (
              <label key={k} className="flex items-center gap-2 text-sm">
                <Checkbox checked={c.placement.pages.includes(k)} onCheckedChange={(v) => set((x) => { x.placement.pages = v ? [...x.placement.pages, k] : x.placement.pages.filter((p) => p !== k); })} />
                {t(l)}
              </label>
            ))}
          </div>
          <Field label="Display"><Choice value={c.placement.display} onChange={(v) => set((x) => { x.placement.display = v; })}
            options={[["hero", "Hero banner"], ["compact", "Compact banner"], ["strip", "Top strip"], ["card", "Floating card"]]} /></Field>
          <ToggleRow label="Visitors can close it" checked={c.placement.dismissible} onChange={(v) => set((x) => { x.placement.dismissible = v; })} />
          <Field label="Priority" hint="When campaigns overlap, the higher one shows"><Input type="number" min={-100} max={100} className="w-24" value={c.priority} onChange={(e) => set((x) => { x.priority = Math.max(-100, Math.min(100, Number(e.target.value) || 0)); })} /></Field>
        </Block>
      </div>
    </ResponsiveDialog>
  );
}

function ColourField({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <Field label={label} optional hint="Empty = the design's own">
      <div className="flex items-center gap-2">
        <input type="color" aria-label={t(label)} value={value || "#c2410c"} onChange={(e) => onChange(e.target.value)} className="h-9 w-12 cursor-pointer rounded border bg-card" />
        <Input value={value} placeholder="#RRGGBB" maxLength={7} onChange={(e) => onChange(e.target.value.trim())} className="w-28" />
        {value && <Button type="button" size="sm" variant="ghost" onClick={() => onChange("")}>{t("Reset")}</Button>}
      </div>
    </Field>
  );
}

/** Platform → Website campaigns: the feature for every business, and its limits (roadmap 84). */
export function PlatformCampaignSettings() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["platform-campaigns"], queryFn: () => api<{ enabled: boolean; max_campaigns: number; max_products: number }>("/platform/campaigns") });
  const [d, setD] = useState<{ enabled: boolean; max_campaigns: number; max_products: number } | null>(null);
  const v = d ?? q.data ?? null;
  const save = useMutation({
    mutationFn: (b: { enabled: boolean; max_campaigns: number; max_products: number }) => api("/platform/campaigns", { method: "PUT", body: b }),
    onSuccess: () => { toast.success("Campaign settings saved"); setD(null); qc.invalidateQueries({ queryKey: ["platform-campaigns"] }); },
    onError: (e) => toast.error(e),
  });
  if (!v) return <Loading />;
  return (
    <Block title="Website campaigns" hint="Holiday & promotional banners on businesses' websites. Off here hides every campaign at once.">
      <ToggleRow label="Available to businesses" checked={v.enabled} onChange={(x) => setD({ ...v, enabled: x })} />
      <Grid2>
        <Field label="Campaigns per business" hint="1–100"><Input type="number" min={1} max={100} value={v.max_campaigns} onChange={(e) => setD({ ...v, max_campaigns: Number(e.target.value) || 1 })} /></Field>
        <Field label="Featured products per campaign" hint="1–24"><Input type="number" min={1} max={24} value={v.max_products} onChange={(e) => setD({ ...v, max_products: Number(e.target.value) || 1 })} /></Field>
      </Grid2>
      <div className="flex justify-end">
        <ActionButton online blockedBy={[!d && "Nothing to save"]} onAction={() => save.mutateAsync(v)}>{t("Save")}</ActionButton>
      </div>
    </Block>
  );
}
