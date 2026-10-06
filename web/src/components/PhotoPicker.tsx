import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ImagePlus, Loader2, X } from "lucide-react";
import { api, ApiError } from "@/lib/api";
import { optimizeImage } from "@/lib/image";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { toast } from "@/lib/toast";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

/** A photo chosen but not yet saved. `ref` makes its upload idempotent (a retry never stores it twice). */
export interface PendingPhoto {
  ref: string;
  file: File;
  url: string;
  /** Why it cannot be saved (not an image, unreadable). */
  problem?: string;
}

const newRef = () => crypto.randomUUID();

/** Pending photos with their preview URLs released when removed or unmounted. */
export function usePendingPhotos() {
  const [items, setItems] = useState<PendingPhoto[]>([]);
  const live = useRef(items);
  live.current = items;
  useEffect(() => () => live.current.forEach((p) => URL.revokeObjectURL(p.url)), []);
  return {
    items,
    add: (files: FileList | File[] | null) => {
      if (!files) return;
      const next = Array.from(files).map((file) => ({
        ref: newRef(),
        file,
        url: URL.createObjectURL(file),
        problem: file.type.startsWith("image/") ? undefined : t("Not a photo"),
      }));
      setItems((cur) => [...cur, ...next]);
    },
    remove: (ref: string) =>
      setItems((cur) => {
        const gone = cur.find((p) => p.ref === ref);
        if (gone) URL.revokeObjectURL(gone.url);
        return cur.filter((p) => p.ref !== ref);
      }),
    markUnreadable: (ref: string) =>
      setItems((cur) => cur.map((p) => (p.ref === ref && !p.problem ? { ...p, problem: t("Can't read this photo — use JPEG or PNG") } : p))),
    clear: () =>
      setItems((cur) => {
        cur.forEach((p) => URL.revokeObjectURL(p.url));
        return [];
      }),
  };
}
export type PendingPhotos = ReturnType<typeof usePendingPhotos>;

/** Whether the pending photos can be saved: within the limit and all readable. */
export function photosValid(items: PendingPhoto[], room: number) {
  return items.length <= room && items.every((p) => !p.problem);
}

/**
 * Select → preview → add/remove → validate. Nothing is uploaded here; the owning form saves with `uploadPhotos`.
 * Over the limit, every photo stays visible, the extras are marked and saving is blocked until some are removed.
 */
export function PhotoPicker({ photos, room, max, existing = 0, disabled }: { photos: PendingPhotos; room: number; max: number; existing?: number; disabled?: boolean }) {
  const over = photos.items.length - room;
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-2.5">
        {photos.items.map((p, i) => {
          const extra = i >= room;
          return (
            <div
              key={p.ref}
              className={cn("relative h-20 w-20 overflow-hidden rounded-xl border sm:h-24 sm:w-24", (extra || p.problem) && "border-2 border-destructive")}
            >
              <img src={p.url} alt="" className={cn("h-full w-full object-cover", (extra || p.problem) && "opacity-50")} onError={() => photos.markUnreadable(p.ref)} />
              {i === 0 && existing === 0 && !p.problem && <span className="absolute bottom-1 start-1 rounded bg-black/60 px-1.5 text-[10px] text-white">{t("Primary")}</span>}
              {(extra || p.problem) && (
                <span className="absolute inset-x-0 bottom-0 bg-destructive px-1 py-0.5 text-center text-[10px] font-medium leading-tight text-destructive-foreground">
                  {p.problem ?? t("Over limit")}
                </span>
              )}
              <button
                type="button"
                disabled={disabled}
                className="absolute end-1 top-1 rounded-full bg-black/65 p-1 text-white hover:bg-black/80"
                onClick={() => photos.remove(p.ref)}
                aria-label={t("Remove photo")}
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          );
        })}
        <label
          className={cn(
            "flex h-20 w-20 cursor-pointer flex-col items-center justify-center gap-1 rounded-xl border-2 border-dashed text-xs text-muted-foreground hover:border-primary/50 sm:h-24 sm:w-24",
            disabled && "pointer-events-none opacity-50",
          )}
        >
          <ImagePlus className="h-5 w-5" /> {t("Add")}
          <input
            type="file"
            accept="image/jpeg,image/png,image/webp,image/heic,image/heif,image/*"
            multiple
            className="hidden"
            onChange={(e) => {
              photos.add(e.target.files);
              e.target.value = ""; // the same file can be picked again after removing it
            }}
          />
        </label>
      </div>
      <p className={cn("text-xs", over > 0 ? "font-medium text-destructive" : "text-muted-foreground")}>
        {over > 0 ? (
          <span className="inline-flex items-center gap-1">
            <AlertTriangle className="h-3.5 w-3.5" />
            {photos.items.length} {t("selected")} · {t("Maximum")} {room} — {t("remove")} {over} {t("to save")}
          </span>
        ) : (
          <>
            {existing + photos.items.length}/{max} {t("photos")}
          </>
        )}
      </p>
    </div>
  );
}

export interface UploadResult {
  saved: number;
  failed: { ref: string; name: string; reason: string }[];
}

/** Saves pending photos one by one. Never throws for a single photo: each succeeds or fails on its own. */
export async function uploadPhotos(productId: string, items: PendingPhoto[], onProgress?: (done: number) => void): Promise<UploadResult> {
  const result: UploadResult = { saved: 0, failed: [] };
  for (const [i, p] of items.entries()) {
    try {
      const blob = await optimizeImage(p.file);
      const fd = new FormData();
      fd.append("upload_ref", p.ref);
      fd.append("file", blob, blob.type === "image/webp" ? "photo.webp" : "photo.jpg");
      await api(`/products/${productId}/photos`, { body: fd });
      result.saved++;
    } catch (e) {
      const reason = e instanceof ApiError ? (e.title ? `${e.title}: ${e.message}` : e.message) : t("Can't read this photo — use JPEG or PNG");
      result.failed.push({ ref: p.ref, name: p.file.name, reason });
    }
    onProgress?.(i + 1);
  }
  return result;
}

/**
 * "Add photos" for an existing product (product page, Receive Stock): pick → review → save. Saved photos leave the
 * list; failed ones stay with the same upload id, so retrying never stores a photo twice.
 */
export function AddPhotosDialog({ productId, existing, max, open, onOpenChange, onSaved }: {
  productId: string;
  existing: number;
  max: number;
  open: boolean;
  onOpenChange: (o: boolean) => void;
  onSaved: () => void;
}) {
  const pending = usePendingPhotos();
  const [busy, setBusy] = useState(false);
  const room = Math.max(max - existing, 0);
  const save = async () => {
    setBusy(true);
    const result = await uploadPhotos(productId, pending.items);
    const failed = new Set(result.failed.map((f) => f.ref));
    pending.items.filter((p) => !failed.has(p.ref)).forEach((p) => pending.remove(p.ref));
    setBusy(false);
    onSaved();
    if (result.failed.length) {
      toast.error(`${result.failed.length} ${t("photo(s) not saved")}`, {
        description: result.failed.map((f) => `${f.name}: ${f.reason}`).join(" · ") + (result.saved ? ` · ${result.saved} ${t("saved")}` : ""),
      });
    } else {
      toast.success(`${result.saved} ${t("photo(s) added")}`);
      onOpenChange(false);
    }
  };
  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={(o) => {
        if (busy) return;
        if (!o) pending.clear();
        onOpenChange(o);
      }}
      title="Add photos"
      description={`${t("Up to")} ${max} ${t("photos per product. Review them before saving.")}`}
      footer={
        <Button className="w-full md:w-auto" disabled={busy || !pending.items.length || !photosValid(pending.items, room)} onClick={save}>
          {busy ? <Loader2 className="animate-spin" /> : <ImagePlus />} {t("Save photos")}
        </Button>
      }
    >
      <PhotoPicker photos={pending} room={room} max={max} existing={existing} disabled={busy} />
    </ResponsiveDialog>
  );
}
