/** Roadmap 190: the product's own photos managed from Settings → Website → Products → (product).
 *
 * The photos are shared with the catalogue, product pages, the till, stock, the website and the ordering link (one
 * source of truth). Add (up to the business's photo limit), replace, preview, reorder (drag, or the arrows on touch
 * screens), remove and choose the primary photo (the first). New photos are uploaded to a holding area and shown
 * nowhere until the change is saved — at once for people who may publish, otherwise after approval. Hiding a photo on
 * the website only (the eye) stays part of the website draft, as before. */
import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ArrowRight, Eye, EyeOff, ImagePlus, Loader2, RefreshCw, Star, Trash2, Undo2 } from "lucide-react";
import { api, ApiError } from "@/lib/api";
import { optimizeImage } from "@/lib/image";
import { toast } from "@/lib/toast";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { dateTime } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { ConfirmDialog, Field } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

interface Tile { id: string; url: string; staged: boolean }
interface PhotoState {
  product: { id: string; name: string };
  photos: { id: string; is_primary: boolean; url: string }[];
  max: number;
  applies_directly: boolean;
  pending: null | { id: string; photos: Tile[]; requested_by: string | null; requested_at: string; approval_id: string | null; summary: Record<string, unknown> };
}

const ACCEPT = /^image\/(jpeg|png|webp)$/;

async function stage(productId: string, file: File): Promise<Tile> {
  if (!ACCEPT.test(file.type) && !/^image\/(heic|heif)$/.test(file.type)) throw new Error(t("Only JPEG, PNG or WebP images can be added as product photos."));
  const main = await optimizeImage(file);
  const thumb = await optimizeImage(file, 480, 0.78).catch(() => null);
  const fd = new FormData();
  fd.append("upload_ref", crypto.randomUUID());
  fd.append("file", main, main.type === "image/webp" ? "photo.webp" : "photo.jpg");
  if (thumb) fd.append("thumb", thumb, thumb.type === "image/webp" ? "thumb.webp" : "thumb.jpg");
  const r = await api<{ id: string; url: string }>(`/website/products/${productId}/photos/stage`, { body: fd });
  return { id: r.id, url: r.url, staged: true };
}

export function ProductPhotoManager({ productId, hidden, onToggleHidden, websiteMax }: {
  productId: string;
  /** Photos hidden on the website only (website draft). */
  hidden: string[];
  onToggleHidden: (id: string) => void;
  /** How many photos the website shows per product. */
  websiteMax: number;
}) {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["website-product-photos", productId], queryFn: () => api<PhotoState>(`/website/products/${productId}/photos`) });
  const base = useMemo(() => (q.data?.photos ?? []).map((p) => ({ id: p.id, url: p.url, staged: false })), [q.data]);
  const [tiles, setTiles] = useState<Tile[]>([]);
  const [busy, setBusy] = useState(0);
  const [drag, setDrag] = useState<number | null>(null);
  const [preview, setPreview] = useState<Tile | null>(null);
  const [removing, setRemoving] = useState<number | null>(null);
  const [comments, setComments] = useState("");
  const add = useRef<HTMLInputElement>(null);
  const replaceAt = useRef<number | null>(null);
  const replace = useRef<HTMLInputElement>(null);
  useEffect(() => setTiles(base), [base]);

  const max = q.data?.max ?? 5;
  const changed = tiles.length !== base.length || tiles.some((x, i) => x.id !== base[i]?.id);
  const pending = q.data?.pending ?? null;
  const direct = !!q.data?.applies_directly;

  const save = useMutation({
    mutationFn: () => api<{ applied: boolean; pending_approval?: boolean }>(`/website/products/${productId}/photos`, {
      method: "PUT",
      body: { photos: tiles.map((x) => ({ id: x.id, staged: x.staged })), base: base.map((x) => x.id), comments },
    }),
    onSuccess: (r) => {
      toast.success(r.applied ? "Photos saved — updated everywhere" : "Photo change sent for approval — nothing changes until it is approved");
      setComments("");
      qc.invalidateQueries({ queryKey: ["website-product-photos", productId] });
      qc.invalidateQueries({ queryKey: ["website-catalogue"] });
      qc.invalidateQueries({ queryKey: ["products"] });
      qc.invalidateQueries({ queryKey: ["product", productId] });
    },
    onError: (e) => toast.error(e),
  });

  const upload = async (files: File[], at: number | null) => {
    const room = at === null ? Math.max(max - tiles.length, 0) : 1;
    const list = files.slice(0, room);
    if (files.length > room) toast.error(`${t("A product can have at most")} ${max} ${t("photos")}`);
    setBusy((n) => n + list.length);
    for (const f of list) {
      try {
        const tile = await stage(productId, f);
        setTiles((cur) => {
          if (at !== null) return cur.map((x, i) => (i === at ? tile : x));
          return [...cur, tile];
        });
      } catch (e) {
        toast.error(e instanceof ApiError ? e : e instanceof Error ? e.message : String(e), { description: f.name });
      } finally {
        setBusy((n) => n - 1);
      }
    }
  };
  const move = (from: number, to: number) => setTiles((cur) => {
    if (to < 0 || to >= cur.length || from === to) return cur;
    const next = [...cur];
    const [x] = next.splice(from, 1);
    next.splice(to, 0, x);
    return next;
  });

  if (q.isLoading) return <p className="text-xs text-muted-foreground">{t("Loading photos…")}</p>;
  if (q.error) return <p className="text-xs text-destructive">{t("Photos could not be loaded")}</p>;

  if (pending) {
    return (
      <div className="space-y-2 rounded-lg border border-warning/40 bg-warning/5 p-2.5" data-testid="photo-change-pending">
        <p className="text-xs font-medium text-warning">{t("Photo change awaiting approval")}{pending.requested_by && ` · ${pending.requested_by}`} · {dateTime(pending.requested_at)}</p>
        <div className="flex flex-wrap gap-2">
          {pending.photos.map((p, i) => (
            <button key={p.id} type="button" onClick={() => setPreview(p)} className="relative h-16 w-16 overflow-hidden rounded-md border">
              <img src={p.url} alt="" className="h-full w-full object-cover" />
              {i === 0 && <span className="absolute bottom-0.5 inset-s-0.5 rounded bg-black/60 px-1 text-[9px] text-white">{t("Primary")}</span>}
              {p.staged && <span className="absolute top-0.5 inset-e-0.5 rounded bg-primary px-1 text-[9px] text-primary-foreground">{t("New")}</span>}
            </button>
          ))}
        </div>
        <p className="text-xs text-muted-foreground">{t("The current photos stay everywhere until the change is approved.")}</p>
        <PreviewDialog tile={preview} onClose={() => setPreview(null)} />
      </div>
    );
  }

  return (
    <div className="space-y-2" data-testid="product-photo-manager">
      <div className="flex flex-wrap gap-2">
        {tiles.map((p, i) => {
          const off = !p.staged && hidden.includes(p.id);
          return (
            <div
              key={p.id}
              draggable
              onDragStart={() => setDrag(i)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => { if (drag !== null) move(drag, i); setDrag(null); }}
              onDragEnd={() => setDrag(null)}
              className={cn("group relative h-24 w-24 overflow-hidden rounded-lg border bg-muted sm:h-28 sm:w-28", drag === i && "opacity-50", i === 0 && "ring-2 ring-primary")}
            >
              <button type="button" className="h-full w-full" onClick={() => setPreview(p)} aria-label={t("Preview photo")}>
                <img src={p.url} alt="" className={cn("h-full w-full object-cover", off && "opacity-40")} />
              </button>
              {i === 0 && <span className="absolute top-1 inset-s-1 rounded bg-primary px-1.5 text-[10px] font-medium text-primary-foreground">{t("Primary")}</span>}
              {p.staged && <span className="absolute top-1 inset-e-1 rounded bg-black/65 px-1.5 text-[10px] text-white">{t("New")}</span>}
              <div className="absolute inset-x-0 bottom-0 flex items-center justify-between bg-black/60 px-0.5 py-0.5 text-white">
                <button type="button" className="rounded p-1 hover:bg-white/20 disabled:opacity-30" disabled={i === 0} onClick={() => move(i, i - 1)} aria-label={t("Move left")}><ArrowLeft className="h-3 w-3" /></button>
                {i !== 0 && <button type="button" className="rounded p-1 hover:bg-white/20" onClick={() => move(i, 0)} aria-label={t("Make primary photo")}><Star className="h-3 w-3" /></button>}
                <button type="button" className="rounded p-1 hover:bg-white/20" onClick={() => { replaceAt.current = i; replace.current?.click(); }} aria-label={t("Replace photo")}><RefreshCw className="h-3 w-3" /></button>
                {!p.staged && (
                  <button type="button" className="rounded p-1 hover:bg-white/20" onClick={() => onToggleHidden(p.id)} aria-pressed={!off} aria-label={off ? t("Show on the website") : t("Hide on the website")}>
                    {off ? <EyeOff className="h-3 w-3" /> : <Eye className="h-3 w-3" />}
                  </button>
                )}
                <button type="button" className="rounded p-1 hover:bg-white/20" onClick={() => setRemoving(i)} aria-label={t("Remove photo")}><Trash2 className="h-3 w-3" /></button>
                <button type="button" className="rounded p-1 hover:bg-white/20 disabled:opacity-30" disabled={i === tiles.length - 1} onClick={() => move(i, i + 1)} aria-label={t("Move right")}><ArrowRight className="h-3 w-3" /></button>
              </div>
            </div>
          );
        })}
        {busy > 0 && <div className="flex h-24 w-24 items-center justify-center rounded-lg border border-dashed sm:h-28 sm:w-28"><Loader2 className="h-5 w-5 animate-spin text-muted-foreground" /></div>}
        {tiles.length + busy < max && (
          <button type="button" onClick={() => add.current?.click()} className="flex h-24 w-24 flex-col items-center justify-center gap-1 rounded-lg border-2 border-dashed text-xs text-muted-foreground hover:border-primary/50 sm:h-28 sm:w-28">
            <ImagePlus className="h-5 w-5" /> {t("Add photos")}
          </button>
        )}
      </div>
      <input ref={add} type="file" accept="image/jpeg,image/png,image/webp,image/heic,image/heif" multiple className="hidden"
        onChange={(e) => { void upload(Array.from(e.target.files ?? []), null); e.target.value = ""; }} />
      <input ref={replace} type="file" accept="image/jpeg,image/png,image/webp,image/heic,image/heif" className="hidden"
        onChange={(e) => { const f = e.target.files?.[0]; if (f) void upload([f], replaceAt.current); e.target.value = ""; }} />
      <p className="text-xs text-muted-foreground">
        <span className="num font-medium text-foreground">{tiles.length}/{max}</span> {t("photos")} · {t("the first is the primary photo")} · {t("drag or use the arrows to reorder")}
        {tiles.length > websiteMax && ` · ${t("the website shows the first")} ${websiteMax}`}
      </p>
      {changed && (
        <div className="space-y-2 rounded-lg border bg-background p-2.5">
          <p className="text-xs text-muted-foreground">
            {direct
              ? t("Saving updates the photos everywhere: catalogue, product pages, the till, stock, the website and the ordering link.")
              : t("Your change is sent for approval. The current photos stay everywhere — including the live website — until it is approved.")}
          </p>
          {!direct && <Field label="Comments" optional><Textarea rows={2} maxLength={500} value={comments} onChange={(e) => setComments(e.target.value)} /></Field>}
          <div className="flex flex-wrap justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={() => setTiles(base)}><Undo2 /> {t("Discard")}</Button>
            <ActionButton size="sm" online busy={save.isPending} busyLabel="Saving…" blockedBy={[!changed && REASONS.nothingToSave, busy > 0 && "Wait for the upload to finish"]} onAction={() => save.mutateAsync()}>
              {direct ? "Save photos" : "Submit for approval"}
            </ActionButton>
          </div>
        </div>
      )}
      <PreviewDialog tile={preview} onClose={() => setPreview(null)} />
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(o) => !o && setRemoving(null)}
        title="Remove this photo?"
        description="It is removed from the product everywhere (catalogue, till, website, ordering link) once the change is saved or approved. Hide it with the eye to remove it from the website only."
        confirmLabel="Remove"
        destructive
        onConfirm={async () => { setTiles((cur) => cur.filter((_, i) => i !== removing)); setRemoving(null); }}
      />
    </div>
  );
}

function PreviewDialog({ tile, onClose }: { tile: Tile | null; onClose: () => void }) {
  return (
    <ResponsiveDialog open={!!tile} onOpenChange={(o) => !o && onClose()} title="Photo">
      {tile && <img src={tile.url} alt="" className="mx-auto max-h-[70vh] w-auto rounded-lg object-contain" />}
    </ResponsiveDialog>
  );
}

/** Approvals: the photos before and as proposed (roadmap 190). */
export function PhotoRequestLines({ id }: { id: string }) {
  const q = useQuery({ queryKey: ["photo-request", id], queryFn: () => api<{ product_name: string; before: Tile[]; proposed: Tile[]; summary: Record<string, unknown> }>(`/website/photo-requests/${id}`) });
  if (!q.data) return null;
  const row = (label: string, list: Tile[]) => (
    <div>
      <div className="mb-1 text-xs text-muted-foreground">{t(label)} · {list.length}</div>
      <div className="flex flex-wrap gap-1.5">
        {list.map((p, i) => (
          <a key={p.id} href={p.url} target="_blank" rel="noreferrer" className={cn("relative h-14 w-14 overflow-hidden rounded-md border", i === 0 && "ring-2 ring-primary")}>
            <img src={p.url} alt="" className="h-full w-full object-cover" />
            {p.staged && <span className="absolute bottom-0 inset-x-0 bg-primary text-center text-[9px] text-primary-foreground">{t("New")}</span>}
          </a>
        ))}
        {!list.length && <span className="text-xs text-muted-foreground">{t("No photos")}</span>}
      </div>
    </div>
  );
  return (
    <div className="space-y-2 rounded-lg bg-muted/40 p-2.5" data-testid="photo-request-lines">
      {row("Current photos", q.data.before)}
      {row("Proposed photos", q.data.proposed)}
    </div>
  );
}
