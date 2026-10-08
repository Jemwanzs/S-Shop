/** Website media library: this business's images only, graded on upload (✓ good · ⚠ warning · ✕ cannot upload). */
import { useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Archive, ArchiveRestore, ImagePlus, Loader2, Trash2, XCircle } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, Select, ConfirmDialog } from "@/components/Form";
import { Loading } from "@/components/Page";
import { t } from "@/lib/i18n";
import { mediaUrl, useMedia, type Media } from "./data";
import { Block, Choice, QualityBadge, uploadMedia } from "./kit";

const KINDS: [string, string][] = [["product", "Product"], ["banner", "Banner"], ["promotion", "Promotion"], ["about", "About"], ["service", "Service"], ["testimonial", "Testimonial"], ["logo", "Logo"], ["other", "Other"]];

export function MediaTab() {
  const { can } = useSession();
  const [archived, setArchived] = useState(false);
  const { data, isLoading } = useMedia(archived);
  const qc = useQueryClient();
  const [kind, setKind] = useState("product");
  const [busy, setBusy] = useState(0);
  const [results, setResults] = useState<{ name: string; ok: boolean; text: string }[]>([]);
  const input = useRef<HTMLInputElement>(null);
  const [del, setDel] = useState<Media | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["website-media"] });
  const patch = useMutation({
    mutationFn: ({ id, body }: { id: string; body: { name?: string; archived?: boolean } }) => api(`/website/media/${id}`, { method: "PATCH", body }),
    onSuccess: refresh,
    onError: (e) => toast.error(e),
  });
  const remove = useMutation({
    mutationFn: (id: string) => api(`/website/media/${id}`, { method: "DELETE" }),
    onSuccess: () => { setDel(null); toast.success("Image deleted"); refresh(); },
    onError: (e) => toast.error(e),
  });
  const onFiles = async (files: FileList | null) => {
    if (!files?.length) return;
    const list = [...files].slice(0, 20);
    setBusy(list.length);
    const out: typeof results = [];
    for (const f of list) {
      try {
        const m = await uploadMedia(f, kind);
        out.push({ name: f.name, ok: true, text: m.warnings?.length ? `⚠ ${m.warnings.join(" · ")}` : `✓ ${t("Good quality")}` });
      } catch (e) {
        out.push({ name: f.name, ok: false, text: `✕ ${e instanceof Error ? e.message : String(e)}` });
      }
      setBusy((n) => n - 1);
    }
    setResults(out);
    if (input.current) input.current.value = "";
    refresh();
  };
  const canUpload = can("website.media") || can("website.photos");
  return (
    <div className="space-y-4">
      {canUpload && (
        <Block title="Upload images" hint="Images are resized in your browser and checked for size, resolution, proportions and sharpness.">
          <div className="flex flex-wrap items-end gap-2">
            <Field label="Used for" className="min-w-40"><Select value={kind} onChange={setKind}>{KINDS.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}</Select></Field>
            <Button onClick={() => input.current?.click()} disabled={busy > 0}>{busy ? <Loader2 className="animate-spin" /> : <ImagePlus />} {busy ? `${t("Uploading…")} ${busy}` : t("Choose images")}</Button>
            <input ref={input} type="file" multiple accept="image/jpeg,image/png,image/webp" className="hidden" onChange={(e) => onFiles(e.target.files)} />
          </div>
          {results.length > 0 && (
            <ul className="space-y-1 text-sm">
              {results.map((r, i) => <li key={i} className={r.ok ? (r.text.startsWith("⚠") ? "text-warning" : "text-success") : "text-destructive"}><span className="font-medium">{r.name}</span> — {r.text}</li>)}
            </ul>
          )}
        </Block>
      )}
      <Block title="Library" action={<Choice value={archived ? "archived" : "active"} onChange={(v) => setArchived(v === "archived")} options={[["active", "In use"], ["archived", "Archived"]]} />}>
        {isLoading ? <Loading /> : !data?.items.length ? <p className="py-6 text-center text-sm text-muted-foreground">{t("No images here.")}</p> : (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4">
            {data.items.map((m) => (
              <div key={m.id} className="overflow-hidden rounded-lg border">
                <img src={mediaUrl(m.id, true)} alt="" className="aspect-square w-full object-cover" loading="lazy" />
                <div className="space-y-1 p-2">
                  <Input defaultValue={m.name} className="h-7 px-2 text-xs" aria-label={t("Name")} disabled={!canUpload}
                    onBlur={(e) => e.target.value.trim() !== m.name && patch.mutate({ id: m.id, body: { name: e.target.value.trim() } })} />
                  <div className="flex items-center justify-between gap-1">
                    <QualityBadge m={m} />
                    <span className="num text-[11px] text-muted-foreground">{m.width}×{m.height}</span>
                  </div>
                  {m.warnings.length > 0 && <p className="text-[11px] leading-snug text-muted-foreground">{m.warnings.join(" · ")}</p>}
                  {canUpload && (
                    <div className="flex justify-end gap-0.5">
                      <Button size="icon" variant="ghost" className="h-7 w-7" aria-label={archived ? t("Restore") : t("Archive")} onClick={() => patch.mutate({ id: m.id, body: { archived: !archived } })}>
                        {archived ? <ArchiveRestore /> : <Archive />}
                      </Button>
                      <Button size="icon" variant="ghost" className="h-7 w-7" aria-label={t("Delete")} onClick={() => setDel(m)}><Trash2 /></Button>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground"><XCircle className="h-3.5 w-3.5" /> {t("Images used on the website (draft, published or history) cannot be deleted or archived.")}</p>
      </Block>
      <ConfirmDialog open={!!del} onOpenChange={(o) => !o && setDel(null)} title="Delete this image?" description="It is removed from the library permanently." confirmLabel="Delete" destructive busy={remove.isPending} onConfirm={() => del && remove.mutate(del.id)} />
    </div>
  );
}
