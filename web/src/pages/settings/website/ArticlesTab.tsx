/** Articles & Content Automation (roadmap 158): one business's articles. Not part of the website draft: each article has its own
 * life: draft → pending review → scheduled or published → archived. Anyone with SEO access writes; publishing needs `website.publish`. */
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Archive, CalendarClock, Copy, Eye, FilePlus2, Save, Send, Trash2, Undo2 } from "lucide-react";
import { api, ApiError, errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { count, date, dateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton } from "@/components/ActionButton";
import { ConfirmDialog, Field } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { useCatalogue, type Overview } from "./data";
import { Block, Grid2, MediaField } from "./kit";

export interface Article {
  id: string; slug: string; title: string; seo_title: string; description: string; body: string; cover_media: string | null; cover_alt: string; category: string; author_name: string;
  product_ids: string[]; status: "draft" | "pending_review" | "scheduled" | "published" | "archived"; scheduled_at: string | null; published_at: string | null; ai_assisted: boolean;
  reviewed_by: string; views: number; created_at: string; updated_at: string;
}

const STATUS: Record<Article["status"], [string, Tone]> = {
  draft: ["Draft", "neutral"], pending_review: ["Pending review", "warning"], scheduled: ["Scheduled", "info"], published: ["Published", "success"], archived: ["Archived", "neutral"],
};
const FILTERS: [Article["status"] | "", string][] = [["", "All"], ["draft", "Drafts"], ["pending_review", "Pending review"], ["scheduled", "Scheduled"], ["published", "Published"], ["archived", "Archived"]];

const fieldsOf = (a: Article) => ({ slug: a.slug, title: a.title, seo_title: a.seo_title, description: a.description, body: a.body, cover_media: a.cover_media, cover_alt: a.cover_alt, category: a.category, author_name: a.author_name, product_ids: a.product_ids });

function Editor({ article, ov, onBack }: { article: Article; ov: Overview; onBack: () => void }) {
  const qc = useQueryClient();
  const { can } = useSession();
  const [f, setF] = useState(fieldsOf(article));
  const [current, setCurrent] = useState(article);
  const [when, setWhen] = useState("");
  const [similar, setSimilar] = useState<{ message: string; action: string } | null>(null);
  const [removing, setRemoving] = useState(false);
  const { data: catalogue } = useCatalogue();
  const [productSearch, setProductSearch] = useState("");
  const set = (patch: Partial<typeof f>) => setF((x) => ({ ...x, ...patch }));
  const dirty = JSON.stringify(f) !== JSON.stringify(fieldsOf(current));
  const canPublish = can("website.publish");
  const live = current.status === "published" || current.status === "scheduled";
  const locked = live && !canPublish;
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["website-articles"] });
  };
  const persist = async () => {
    const saved = await api<Article>(`/website/articles/${current.id}`, { method: "PUT", body: { ...f, base_updated_at: current.updated_at } });
    setCurrent(saved);
    setF(fieldsOf(saved));
    refresh();
    return saved;
  };
  const move = async (action: string, confirm_similar = false) => {
    try {
      if (dirty) await persist();
      const scheduled_at = action === "schedule" && when ? new Date(when).toISOString() : undefined;
      const saved = await api<Article>(`/website/articles/${current.id}/status`, { body: { action, scheduled_at, confirm_similar } });
      setCurrent(saved);
      setF(fieldsOf(saved));
      setSimilar(null);
      refresh();
      toast.success(t(STATUS[saved.status][0]));
    } catch (e) {
      if (e instanceof ApiError && e.title === "Very similar article") setSimilar({ message: e.message, action });
      else toast.error(e);
    }
  };
  const remove = useMutation({
    mutationFn: () => api(`/website/articles/${current.id}`, { method: "DELETE" }),
    onSuccess: () => { refresh(); onBack(); },
    onError: (e) => toast.error(e),
  });
  const products = useMemo(() => (catalogue?.products ?? []).filter((p) => p.is_active), [catalogue]);
  const found = productSearch.trim() ? products.filter((p) => p.name.toLowerCase().includes(productSearch.toLowerCase()) && !f.product_ids.includes(p.id)).slice(0, 6) : [];
  const url = `${ov.public_url}/articles/${current.slug}`;
  return (
    <div className="space-y-4 pb-10">
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="ghost" size="sm" onClick={onBack}><ArrowLeft /> {t("All articles")}</Button>
        <Pill tone={STATUS[current.status][1]}>{t(STATUS[current.status][0])}</Pill>
        {current.ai_assisted && <Pill tone="info">{t("AI-assisted draft: check every fact")}</Pill>}
        {current.status === "scheduled" && current.scheduled_at && <span className="text-xs text-muted-foreground">{dateTime(current.scheduled_at)}</span>}
      </div>
      {locked && <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t("This article is live. Only someone who may publish can change it.")}</p>}
      <Block title="Article">
        <Field label="Title" hint={`${f.title.length}/120`}><Input value={f.title} maxLength={120} disabled={locked} onChange={(e) => set({ title: e.target.value })} /></Field>
        <Field label="Article text" hint={t("Headings start with ## and lists with -. Use **bold** and [text](https://link). Write for your readers; say only what you know to be true.")}>
          <Textarea rows={16} value={f.body} disabled={locked} onChange={(e) => set({ body: e.target.value })} />
        </Field>
        <MediaField label="Cover image" kind="other" value={f.cover_media} onChange={(id) => set({ cover_media: id })} />
        <Field label="Cover image description" optional hint="What the picture shows, for people who cannot see it"><Input value={f.cover_alt} maxLength={200} disabled={locked} onChange={(e) => set({ cover_alt: e.target.value })} /></Field>
        <Grid2>
          <Field label="Article category" optional><Input value={f.category} maxLength={80} disabled={locked} onChange={(e) => set({ category: e.target.value })} /></Field>
          <Field label="Author" optional><Input value={f.author_name} maxLength={120} disabled={locked} onChange={(e) => set({ author_name: e.target.value })} /></Field>
        </Grid2>
      </Block>
      <Block title="Search & sharing">
        <Field label="Description" hint={`${f.description.length}/155`}><Textarea rows={2} value={f.description} maxLength={170} disabled={locked} onChange={(e) => set({ description: e.target.value })} /></Field>
        <Field label="Search title" optional hint={`${f.seo_title.length}/60`}><Input value={f.seo_title} maxLength={70} disabled={locked} onChange={(e) => set({ seo_title: e.target.value })} /></Field>
        <Field label="Address" hint={current.published_at ? t("If you change it, the old address keeps working and forwards here.") : undefined}><Input value={f.slug} maxLength={90} disabled={locked} onChange={(e) => set({ slug: e.target.value })} /></Field>
        <div className="rounded-lg border p-3">
          <p className="truncate text-xs text-success">{url}</p>
          <p className="truncate text-base text-primary">{f.seo_title || f.title}</p>
          <p className="line-clamp-2 text-sm text-muted-foreground">{f.description}</p>
        </div>
      </Block>
      <Block title="Products mentioned" hint="Link only products this article really talks about.">
        <div className="flex flex-wrap gap-1.5">
          {f.product_ids.map((id) => (
            <button key={id} type="button" disabled={locked} className="rounded-full border px-2.5 py-1 text-xs hover:bg-accent" onClick={() => set({ product_ids: f.product_ids.filter((x) => x !== id) })}>
              {products.find((p) => p.id === id)?.name ?? t("Unavailable product")} ×
            </button>
          ))}
        </div>
        <Input value={productSearch} disabled={locked} placeholder={t("Search your products")} onChange={(e) => setProductSearch(e.target.value)} />
        {found.length > 0 && (
          <ul className="divide-y rounded-lg border">
            {found.map((p) => <li key={p.id}><button type="button" className="w-full px-3 py-2 text-start text-sm hover:bg-accent" onClick={() => { set({ product_ids: [...f.product_ids, p.id] }); setProductSearch(""); }}>{p.name}</button></li>)}
          </ul>
        )}
      </Block>
      {similar && (
        <div className="space-y-2 rounded-xl bg-warning/10 p-3 text-sm" role="alert">
          <p>{similar.message}</p>
          <Button size="sm" variant="outline" onClick={() => void move(similar.action, true)}>{t("Publish anyway")}</Button>
        </div>
      )}
      <Block title="What happens next">
        <div className="flex flex-wrap items-end gap-2">
          {!locked && <ActionButton variant="outline" online blockedBy={[!dirty && "Nothing to save"]} quietReason onAction={async () => { try { await persist(); toast.success("Saved"); } catch (e) { toast.error(errorMessage(e)); } }}><Save /> {t("Save")}</ActionButton>}
          {current.status === "draft" && <ActionButton variant="outline" online onAction={() => move("submit")}><Send /> {t("Send for review")}</ActionButton>}
          {canPublish && current.status !== "published" && current.status !== "archived" && <ActionButton online onAction={() => move("publish")}><Eye /> {t("Publish article")}</ActionButton>}
          {(current.status === "pending_review" || live || current.status === "archived") && (!live || canPublish) && <ActionButton variant="outline" online onAction={() => move("draft")}><Undo2 /> {t("Back to draft")}</ActionButton>}
          {canPublish && current.status !== "archived" && <ActionButton variant="ghost" online onAction={() => move("archive")}><Archive /> {t("Archive")}</ActionButton>}
          {!current.published_at && (current.status === "draft" || current.status === "pending_review") && <Button variant="ghost" onClick={() => setRemoving(true)}><Trash2 /> {t("Delete")}</Button>}
        </div>
        {canPublish && current.status !== "archived" && current.status !== "published" && (
          <div className="mt-3 flex flex-wrap items-end gap-2">
            <Field label="Publish later" optional><Input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} /></Field>
            <ActionButton variant="outline" online blockedBy={[!when && "Choose a time"]} onAction={() => move("schedule")}><CalendarClock /> {t("Schedule")}</ActionButton>
          </div>
        )}
        {current.status === "published" && (
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" onClick={() => void navigator.clipboard?.writeText(url).then(() => toast.success("Link copied"))}><Copy /> {t("Copy public link")}</Button>
            <Button size="sm" variant="outline" asChild><a href={`https://wa.me/?text=${encodeURIComponent(`${current.title} ${url}`)}`} target="_blank" rel="noreferrer">WhatsApp</a></Button>
            <Button size="sm" variant="outline" asChild><a href={`https://www.facebook.com/sharer/sharer.php?u=${encodeURIComponent(url)}`} target="_blank" rel="noreferrer">Facebook</a></Button>
            <Button size="sm" variant="outline" asChild><a href={`https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(url)}`} target="_blank" rel="noreferrer">LinkedIn</a></Button>
            <span className="text-xs text-muted-foreground">{count(current.views)} {t("views")}</span>
          </div>
        )}
      </Block>
      <ConfirmDialog open={removing} onOpenChange={setRemoving} title="Delete this draft?" description="It has never been public. This cannot be undone." confirmLabel="Delete" destructive busy={remove.isPending} onConfirm={() => remove.mutate()} />
    </div>
  );
}

export function ArticlesTab({ ov }: { ov: Overview }) {
  const qc = useQueryClient();
  const [filter, setFilter] = useState<Article["status"] | "">("");
  const [open, setOpen] = useState<Article | null>(null);
  const { data, isLoading } = useQuery({ queryKey: ["website-articles", filter], queryFn: () => api<{ items: Article[]; counts: Record<string, number> }>("/website/articles", { query: { status: filter || undefined } }) });
  const create = useMutation({
    mutationFn: () => api<Article>("/website/articles", { body: { title: t("New article"), slug: `new-article-${Date.now().toString(36)}` } }),
    onSuccess: (a) => { qc.invalidateQueries({ queryKey: ["website-articles"] }); setOpen(a); },
    onError: (e) => toast.error(e),
  });
  if (open) return <Editor key={open.id} article={open} ov={ov} onBack={() => setOpen(null)} />;
  if (isLoading || !data) return <Loading />;
  const total = Object.values(data.counts).reduce((a, n) => a + n, 0);
  return (
    <Block title="Articles" hint="Helpful articles bring people to your website. Nothing is public until someone with publishing rights publishes it." action={
      <ActionButton size="sm" online busy={create.isPending} onAction={() => create.mutateAsync()}><FilePlus2 /> {t("Create article")}</ActionButton>
    }>
      <div className="flex gap-1.5 overflow-x-auto pb-1">
        {FILTERS.map(([k, label]) => (
          <button key={k} type="button" onClick={() => setFilter(k)} className={cn("shrink-0 rounded-full border px-3 py-1 text-xs", filter === k && "border-primary bg-primary/10 font-medium")}>
            {t(label)} ({k ? data.counts[k] ?? 0 : total})
          </button>
        ))}
      </div>
      {data.items.length === 0 ? <p className="py-4 text-sm text-muted-foreground">{t("No articles here yet.")}</p> : (
        <ul className="divide-y">
          {data.items.map((a) => (
            <li key={a.id}>
              <button type="button" className="flex w-full items-center gap-3 py-2.5 text-start hover:bg-accent/50" onClick={() => setOpen(a)}>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{a.title}</span>
                  <span className="block truncate text-xs text-muted-foreground">{a.published_at ? date(a.published_at) : t("Not published")}{a.status === "published" ? ` · ${count(a.views)} ${t("views")}` : ""}{a.ai_assisted ? ` · ${t("AI-assisted")}` : ""}</span>
                </span>
                <Pill tone={STATUS[a.status][1]}>{t(STATUS[a.status][0])}</Pill>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Block>
  );
}
