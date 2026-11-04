/** SEO & Organic Growth (roadmap 157): one business's search settings. Everything here is edited in the website's draft and goes
 * live with Publish, like the rest of the website; nothing is shared between businesses or assumed about what they sell. */
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Check, Sparkles, X } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Field, ToggleRow } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { t } from "@/lib/i18n";
import { blankProduct, useCatalogue, type SiteConfig } from "./data";
import { Block, Grid2, MediaField } from "./kit";
import { ArticlesTab } from "./ArticlesTab";
import { HealthTab, SearchEnginesBlock } from "./SeoHealth";
import type { TabProps } from "./Website";

const TITLE_MAX = 60;
const DESCRIPTION_MAX = 155;

/** How a length compares with what search results show: guidance, not a rule. */
function Count({ value, max }: { value: string; max: number }) {
  const n = value.length;
  const tone = n === 0 ? "text-warning" : n > max ? "text-warning" : "text-muted-foreground";
  return <span className={tone}>{n}/{max}</span>;
}

const lines = (v: string[]) => v.join("\n");
const parseLines = (v: string) => v.split("\n").map((x) => x.trimStart()).slice(0, 30);

function SearchPreview({ url, title, description }: { url: string; title: string; description: string }) {
  return (
    <div className="rounded-lg border p-3">
      <p className="truncate text-xs text-success">{url}</p>
      <p className="truncate text-base text-primary">{title}</p>
      <p className="line-clamp-2 text-sm text-muted-foreground">{description || t("Add a description so people know what you sell.")}</p>
    </div>
  );
}

/** A new website's first visit: three questions, answered from the business's own catalogue where it can help. */
function SetupWizard({ c, set }: TabProps) {
  const { data } = useCatalogue();
  const suggestions = useMemo(() => (data?.categories ?? []).filter((k) => k.is_active).slice(0, 8).map((k) => k.name), [data]);
  return (
    <Block title="Set up in a minute" hint="Answer in your own words. You can change everything later, and nothing goes live until you publish." action={
      <Button size="sm" variant="ghost" onClick={() => set((x) => { x.seo.wizard_done = true; })}><X /> {t("Skip")}</Button>
    }>
      <Field label="What does your business sell or do?" hint="For example, the kind of shop you would tell a customer you are">
        <Input value={c.seo.primary_category} maxLength={80} onChange={(e) => set((x) => { x.seo.primary_category = e.target.value; })} />
      </Field>
      {suggestions.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          <span className="text-xs text-muted-foreground">{t("From your categories:")}</span>
          {suggestions.map((s) => <button key={s} type="button" className="rounded-full border px-2.5 py-0.5 text-xs hover:bg-accent" onClick={() => set((x) => { x.seo.primary_category = s; })}>{s}</button>)}
        </div>
      )}
      <Field label="Which places do you serve?" optional hint="A town, a region, a whole country or online"><Input value={c.seo.service_area} maxLength={200} onChange={(e) => set((x) => { x.seo.service_area = e.target.value; })} /></Field>
      <Field label="Website language" optional hint="A code such as en, sw or fr-CA. Empty means English."><Input value={c.seo.language ?? ""} maxLength={12} onChange={(e) => set((x) => { x.seo.language = e.target.value; })} /></Field>
      <Button size="sm" onClick={() => set((x) => { x.seo.wizard_done = true; })}><Check /> {t("Done")}</Button>
    </Block>
  );
}

function GeneralTab(props: TabProps) {
  const { c, set, ov } = props;
  const title = c.seo.title || c.brand.name;
  const desc = c.seo.description || c.brand.tagline;
  return (
    <div className="space-y-4">
      {!c.seo.wizard_done && <SetupWizard {...props} />}
      <Block title="Search engines & sharing">
        <Field label="Website title" hint={<Count value={c.seo.title} max={TITLE_MAX} />}><Input value={c.seo.title} maxLength={70} onChange={(e) => set((x) => { x.seo.title = e.target.value; })} /></Field>
        <Field label="Description" hint={<Count value={c.seo.description} max={DESCRIPTION_MAX} />}><Textarea value={c.seo.description} maxLength={170} onChange={(e) => set((x) => { x.seo.description = e.target.value; })} /></Field>
        <Field label="About the business" optional hint="Used for search engines"><Textarea value={c.seo.business_description} maxLength={500} onChange={(e) => set((x) => { x.seo.business_description = e.target.value; })} /></Field>
        <MediaField label="Sharing image" kind="banner" value={c.seo.share_image} onChange={(id) => set((x) => { x.seo.share_image = id; })} hint="Shown when your link is shared on WhatsApp or social media (empty = the hero image)." />
      </Block>
      <Block title="Your business" hint="In your own words. These guide your own content; they are not published as a list of keywords.">
        <Grid2>
          <Field label="Primary business category" optional><Input value={c.seo.primary_category} maxLength={80} onChange={(e) => set((x) => { x.seo.primary_category = e.target.value; })} /></Field>
          <Field label="Geographic service area" optional><Input value={c.seo.service_area} maxLength={200} onChange={(e) => set((x) => { x.seo.service_area = e.target.value; })} /></Field>
        </Grid2>
        <Field label="Search phrases and content topics" optional hint="One per line: what customers ask or look for">
          <Textarea rows={4} value={lines(c.seo.topics ?? [])} onChange={(e) => set((x) => { x.seo.topics = parseLines(e.target.value); })} />
        </Field>
      </Block>
      <Block title="Search result preview">
        <SearchPreview url={ov.public_url ?? ""} title={title} description={desc} />
        <p className="text-xs text-muted-foreground">{t("No tool can guarantee a position in search results. Clear, accurate pages give you the best chance.")}</p>
      </Block>
    </div>
  );
}

function LocalTab({ c, set, ov }: TabProps) {
  const l = c.seo.local;
  const edit = (fn: (x: SiteConfig["seo"]["local"]) => void) => set((x) => fn(x.seo.local));
  const branch = (id: string) => l.branches.find((b) => b.branch_id === id);
  const editBranch = (id: string, fn: (b: SiteConfig["seo"]["local"]["branches"][number]) => void) => edit((x) => {
    let b = x.branches.find((y) => y.branch_id === id);
    if (!b) {
      b = { branch_id: id, show: false, name: "", street_address: "", city: "", region: "", country: "", phone: "", maps_url: "" };
      x.branches.push(b);
    }
    fn(b);
  });
  return (
    <div className="space-y-4">
      <Block title="Where customers can find you">
        <ToggleRow label="Customers can visit" hint="Publishes this address on your Contact page and to search engines. Leave off if you only sell online." checked={l.enabled} onChange={(v) => edit((x) => { x.enabled = v; })} />
        <Field label="Address" optional={!l.enabled}><Input value={l.street_address} maxLength={200} onChange={(e) => edit((x) => { x.street_address = e.target.value; })} /></Field>
        <Grid2>
          <Field label="City or town" optional={!l.enabled}><Input value={l.city} maxLength={80} onChange={(e) => edit((x) => { x.city = e.target.value; })} /></Field>
          <Field label="County or region" optional><Input value={l.region} maxLength={80} onChange={(e) => edit((x) => { x.region = e.target.value; })} /></Field>
          <Field label="Country" optional><Input value={l.country} maxLength={80} onChange={(e) => edit((x) => { x.country = e.target.value; })} /></Field>
          <Field label="Postal code" optional><Input value={l.postal_code} maxLength={20} onChange={(e) => edit((x) => { x.postal_code = e.target.value; })} /></Field>
          <Field label="Public phone" optional><Input value={l.phone} maxLength={40} onChange={(e) => edit((x) => { x.phone = e.target.value; })} /></Field>
        </Grid2>
        <Field label="Service areas" optional hint="One per line: only places you really serve"><Textarea rows={3} value={lines(l.service_areas ?? [])} onChange={(e) => edit((x) => { x.service_areas = parseLines(e.target.value); })} /></Field>
        <Field label="Google Maps link" optional><Input value={l.maps_url} placeholder="https://" onChange={(e) => edit((x) => { x.maps_url = e.target.value.trim(); })} /></Field>
        <Field label="Google Business Profile link" optional><Input value={l.profile_url} placeholder="https://" onChange={(e) => edit((x) => { x.profile_url = e.target.value.trim(); })} /></Field>
        <p className="text-xs text-muted-foreground">{t("WhatsApp, opening hours text and social links are set in Content → Contact and Social.")}</p>
      </Block>
      {(ov.branches?.length ?? 0) > 0 && (
        <Block title="Branch locations" hint="Only your real branches can be listed. Switched-on branches appear on your Contact page and to search engines, with no separate page for each.">
          <ul className="divide-y">
            {ov.branches!.map((r) => {
              const b = branch(r.id);
              return (
                <li key={r.id} className="py-2">
                  <ToggleRow label={r.name} hint={r.location || t("No address on the branch yet")} checked={!!b?.show} disabled={!l.enabled} onChange={(v) => editBranch(r.id, (x) => { x.show = v; })} />
                  {b?.show && (
                    <Grid2>
                      <Field label="Address shown" optional hint="Empty uses the branch's own"><Input value={b.street_address} maxLength={200} onChange={(e) => editBranch(r.id, (x) => { x.street_address = e.target.value; })} /></Field>
                      <Field label="City or town" optional><Input value={b.city} maxLength={80} onChange={(e) => editBranch(r.id, (x) => { x.city = e.target.value; })} /></Field>
                      <Field label="Phone shown" optional hint="Empty uses the branch's own"><Input value={b.phone} maxLength={40} onChange={(e) => editBranch(r.id, (x) => { x.phone = e.target.value; })} /></Field>
                      <Field label="Google Maps link" optional><Input value={b.maps_url} placeholder="https://" onChange={(e) => editBranch(r.id, (x) => { x.maps_url = e.target.value.trim(); })} /></Field>
                    </Grid2>
                  )}
                </li>
              );
            })}
          </ul>
          {!l.enabled && <p className="text-xs text-warning">{t("Switch on Customers can visit to list branches.")}</p>}
        </Block>
      )}
      <Block title="Google Business Profile checklist" hint="Done by you, on Google. S'Shop does not claim or change a listing for you.">
        <ol className="list-decimal space-y-1 ps-5 text-sm">
          {[
            "Search for your business on business.google.com and claim it, or add it if it is not listed.",
            "Verify it the way Google asks. Only the real owner of a real business can do this.",
            "Use the same name, address and phone as on this page.",
            "Add your opening hours, photos and this website's address.",
            "Paste your profile's link above, and keep the details the same in both places when anything changes.",
          ].map((s) => <li key={s}>{t(s)}</li>)}
        </ol>
      </Block>
    </div>
  );
}

interface Suggestions {
  products: { id: string; name: string; category: string | null; has_photo: boolean; has_description: boolean; seo_title: string; seo_description: string; suggested_title: string; suggested_description: string }[];
  categories: { id: string; name: string; seo_title: string; seo_description: string; suggested_title: string; suggested_description: string }[];
}

function ProductsCategoriesTab({ c, set }: TabProps) {
  const { can } = useSession();
  const { data, isLoading } = useQuery({ queryKey: ["website-seo-suggestions"], queryFn: () => api<Suggestions>("/website/seo/suggestions") });
  const [filter, setFilter] = useState<"missing" | "all">("missing");
  const editProducts = can("website.products");
  const editCategories = can("website.categories");
  if (isLoading || !data) return <Loading />;
  // What the draft holds now (typing here must show at once), over the suggestions' snapshot.
  const own = (id: string) => c.products.items.find((p) => p.product_id === id);
  const setProduct = (id: string, title: string | null, description: string | null) => set((x) => {
    let p = x.products.items.find((y) => y.product_id === id);
    if (!p) {
      p = { ...blankProduct(id), published: x.products.auto_publish_new, sort: x.products.items.length };
      x.products.items.push(p);
    }
    if (title !== null) p.seo_title = title;
    if (description !== null) p.seo_description = description;
  });
  const setCategory = (id: string, title: string | null, description: string | null) => set((x) => {
    let k = x.categories.items.find((y) => y.category_id === id);
    if (!k) {
      k = { category_id: id, visible: true, sort: x.categories.items.length, image: null, seo_title: "", seo_description: "" };
      x.categories.items.push(k);
    }
    if (title !== null) k.seo_title = title;
    if (description !== null) k.seo_description = description;
  });
  const products = data.products.map((p) => ({ ...p, title: own(p.id)?.seo_title ?? "", description: own(p.id)?.seo_description ?? "" }));
  const open = products.filter((p) => !p.title || !p.description);
  const shown = filter === "missing" ? open : products;
  return (
    <div className="space-y-4">
      <Block title="Products" hint="Suggestions use only what you have on record: the name, category and description. Check them before publishing." action={
        editProducts && open.length > 0 ? <Button size="sm" variant="outline" onClick={() => set((x) => {
          for (const p of open) {
            let e = x.products.items.find((y) => y.product_id === p.id);
            if (!e) {
              e = { ...blankProduct(p.id), published: x.products.auto_publish_new, sort: x.products.items.length };
              x.products.items.push(e);
            }
            if (!e.seo_title) e.seo_title = p.suggested_title;
            if (!e.seo_description) e.seo_description = p.suggested_description;
          }
        })}><Sparkles /> {t("Use suggestions for all missing")} ({open.length})</Button> : undefined
      }>
        <div className="flex gap-1.5">
          {(["missing", "all"] as const).map((k) => (
            <button key={k} type="button" onClick={() => setFilter(k)} className={cn("rounded-full border px-3 py-1 text-xs", filter === k && "border-primary bg-primary/10 font-medium")}>
              {t(k === "missing" ? "Missing" : "All")} ({k === "missing" ? open.length : products.length})
            </button>
          ))}
        </div>
        {shown.length === 0 ? <p className="text-sm text-muted-foreground">{t("Every product has a search title and description.")}</p> : (
          <ul className="divide-y">
            {shown.slice(0, 50).map((p) => (
              <li key={p.id} className="space-y-2 py-3">
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="text-sm font-medium">{p.name}</span>
                  {!p.has_photo && <Pill tone="warning">{t("No photo")}</Pill>}
                  {!p.has_description && <Pill tone="warning">{t("No description")}</Pill>}
                </div>
                <Field label="Search title" hint={<Count value={p.title} max={TITLE_MAX} />}><Input value={p.title} maxLength={70} placeholder={p.suggested_title} disabled={!editProducts} onChange={(e) => setProduct(p.id, e.target.value, null)} /></Field>
                <Field label="Search description" hint={<Count value={p.description} max={DESCRIPTION_MAX} />}><Textarea rows={2} value={p.description} maxLength={170} placeholder={p.suggested_description} disabled={!editProducts} onChange={(e) => setProduct(p.id, null, e.target.value)} /></Field>
                {editProducts && (!p.title || !p.description) && <Button size="sm" variant="ghost" onClick={() => setProduct(p.id, p.title || p.suggested_title, p.description || p.suggested_description)}><Sparkles /> {t("Use suggestion")}</Button>}
              </li>
            ))}
          </ul>
        )}
        {shown.length > 50 && <p className="text-xs text-muted-foreground">{t("Showing the first 50. Apply or finish these to see more.")}</p>}
      </Block>
      <Block title="Categories">
        <ul className="divide-y">
          {data.categories.map((k) => {
            const e = c.categories.items.find((x) => x.category_id === k.id);
            return (
              <li key={k.id} className="space-y-2 py-3">
                <span className="text-sm font-medium">{k.name}</span>
                <Field label="Search title" optional hint={<Count value={e?.seo_title ?? ""} max={TITLE_MAX} />}><Input value={e?.seo_title ?? ""} maxLength={70} placeholder={k.suggested_title} disabled={!editCategories} onChange={(v) => setCategory(k.id, v.target.value, null)} /></Field>
                <Field label="Search description" optional hint={<Count value={e?.seo_description ?? ""} max={DESCRIPTION_MAX} />}><Input value={e?.seo_description ?? ""} maxLength={170} placeholder={k.suggested_description} disabled={!editCategories} onChange={(v) => setCategory(k.id, null, v.target.value)} /></Field>
              </li>
            );
          })}
        </ul>
      </Block>
    </div>
  );
}

function IndexingTab({ c, set, ov }: TabProps) {
  return (
    <div className="space-y-4">
      <Block title="Indexing & language">
        <ToggleRow label="Allow search engines to list this website" hint="Off: every page asks search engines to stay away and the sitemap is empty. People with the link can still visit."
          checked={!c.seo.noindex} onChange={(v) => set((x) => { x.seo.noindex = !v; })} />
        <Field label="Website language" optional hint="A code such as en, sw or fr-CA. Empty means English."><Input value={c.seo.language ?? ""} maxLength={12} onChange={(e) => set((x) => { x.seo.language = e.target.value; })} /></Field>
      </Block>
      <Block title="Your website's address and sitemap" hint="One address for search engines. Connect your own domain in the Domain tab and it becomes this address, with the S'Shop address forwarding to it.">
        <dl className="space-y-1 text-sm">
          <div><dt className="label-caps">{t("Main address")}</dt><dd className="break-all">{ov.public_url}</dd></div>
          <div><dt className="label-caps">{t("Sitemap (updates itself)")}</dt><dd className="break-all">{ov.public_url}/sitemap.xml</dd></div>
        </dl>
      </Block>
      <SearchEnginesBlock c={c} set={set} ov={ov} />
    </div>
  );
}

const TABS = [["general", "General SEO"], ["local", "Local SEO"], ["products", "Products & Categories"], ["articles", "Articles & Content Automation"], ["indexing", "Search Engines & Indexing"], ["health", "Analytics & SEO Health"]] as const;

export function SeoTab(props: TabProps) {
  const [tab, setTab] = useState<(typeof TABS)[number][0]>("general");
  return (
    <div className="space-y-4">
      <div className="flex gap-1.5 overflow-x-auto pb-1" role="tablist" aria-label={t("SEO & Organic Growth")}>
        {TABS.map(([k, label]) => (
          <button key={k} type="button" role="tab" aria-selected={tab === k} onClick={() => setTab(k)}
            className={cn("shrink-0 rounded-full border px-3 py-1.5 text-sm", tab === k ? "border-primary bg-primary/10 font-medium" : "text-muted-foreground hover:bg-accent")}>{t(label)}</button>
        ))}
      </div>
      {tab === "general" && <GeneralTab {...props} />}
      {tab === "local" && <LocalTab {...props} />}
      {tab === "products" && <ProductsCategoriesTab {...props} />}
      {tab === "articles" && <ArticlesTab ov={props.ov} />}
      {tab === "indexing" && <IndexingTab {...props} />}
      {tab === "health" && <HealthTab onOpen={(k) => setTab(k as (typeof TABS)[number][0])} />}
    </div>
  );
}
