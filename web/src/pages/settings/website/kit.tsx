/** Building blocks shared by the Website Management Centre tabs. */
import { useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, ArrowDown, ArrowUp, CheckCircle2, ImagePlus, Loader2, X, XCircle } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { optimizeImage } from "@/lib/image";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, Select } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { t } from "@/lib/i18n";
import { contrast, mediaUrl, uid, useMedia, type Cta, type Media } from "./data";

export function Block({ title, hint, children, action }: { title: string; hint?: ReactNode; children: ReactNode; action?: ReactNode }) {
  return (
    <section className="surface card-body space-y-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="font-semibold">{t(title)}</h3>
          {hint && <p className="text-xs text-muted-foreground">{typeof hint === "string" ? t(hint) : hint}</p>}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

export function Grid2({ children }: { children: ReactNode }) {
  return <div className="grid gap-3 md:grid-cols-2">{children}</div>;
}

export function Choice<T extends string | number>({ value, onChange, options, label }: { value: T; onChange: (v: T) => void; options: [T, string][]; label?: string }) {
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap gap-1.5">
      {options.map(([v, l]) => (
        <button key={String(v)} type="button" role="radio" aria-checked={value === v} onClick={() => onChange(v)}
          className={cn("h-8 rounded-full border px-3 text-sm", value === v ? "border-primary bg-primary text-primary-foreground" : "bg-card hover:bg-accent/50")}>
          {t(l)}
        </button>
      ))}
    </div>
  );
}

export function Move({ index, count, onMove }: { index: number; count: number; onMove: (from: number, to: number) => void }) {
  return (
    <span className="flex shrink-0">
      <Button type="button" size="icon" variant="ghost" className="h-8 w-8" disabled={index === 0} aria-label={t("Move up")} onClick={() => onMove(index, index - 1)}><ArrowUp /></Button>
      <Button type="button" size="icon" variant="ghost" className="h-8 w-8" disabled={index === count - 1} aria-label={t("Move down")} onClick={() => onMove(index, index + 1)}><ArrowDown /></Button>
    </span>
  );
}

export function moveItem<T>(list: T[], from: number, to: number) {
  const [x] = list.splice(from, 1);
  list.splice(to, 0, x);
}

const PAGES: [string, string][] = [
  ["", "No button"], ["products", "Products page"], ["categories", "Categories"], ["about", "About Us"], ["services", "Services"],
  ["contact", "Contact"], ["order", "Order / cart"], ["testimonials", "Testimonials"], ["home", "Home"], ["link", "Web, phone or email link…"],
];

/** A button: label + a page of the website or an https: / tel: / mailto: link. */
export function CtaEditor({ label, value, onChange }: { label: string; value: Cta; onChange: (c: Cta) => void }) {
  const isPage = PAGES.some(([k]) => k && k !== "link" && k === value.target);
  const mode = !value.target ? "" : isPage ? value.target : "link";
  return (
    <div className="grid gap-2 rounded-lg border p-3 sm:grid-cols-[1fr_1fr]">
      <Field label={`${t(label)} — ${t("text")}`}><Input value={value.label} maxLength={40} onChange={(e) => onChange({ ...value, label: e.target.value })} /></Field>
      <Field label={t("Goes to")}>
        <Select value={mode} onChange={(v) => onChange({ ...value, target: v === "link" ? "https://" : v })}>
          {PAGES.map(([k, l]) => <option key={k} value={k}>{t(l)}</option>)}
        </Select>
      </Field>
      {mode === "link" && (
        <Field label={t("Link")} hint="https://…, tel:0712… or mailto:…" className="sm:col-span-2">
          <Input value={value.target} onChange={(e) => onChange({ ...value, target: e.target.value.trim() })} />
        </Field>
      )}
    </div>
  );
}

export function ColorField({ label, value, onChange, against, min }: { label: string; value: string; onChange: (v: string) => void; against?: string; min?: number }) {
  const ratio = against ? contrast(value, against) : null;
  const ok = ratio === null || ratio >= (min ?? 4.5);
  return (
    <label className="flex items-center gap-2.5 rounded-lg border p-2">
      <input type="color" value={/^#[0-9a-f]{6}$/i.test(value) ? value : "#000000"} onChange={(e) => onChange(e.target.value.toUpperCase())} className="h-9 w-9 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0" />
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{t(label)}</span>
        <Input value={value} onChange={(e) => onChange(e.target.value)} className="mt-1 h-7 px-2 font-mono text-xs" maxLength={7} aria-label={t(label)} />
      </span>
      {ratio !== null && (
        <span className={cn("num shrink-0 text-xs", ok ? "text-success" : "text-destructive")} title={t("Contrast ratio")}>
          {ok ? "✓" : "✕"} {ratio.toFixed(1)}:1
        </span>
      )}
    </label>
  );
}

// ── Media ─────────────────────────────────────────────────────────────

/** Sharpness estimate: variance of the Laplacian on a small greyscale copy (low = blurry). */
async function looksBlurry(file: Blob): Promise<boolean> {
  try {
    const bmp = await createImageBitmap(file);
    const w = 256;
    const h = Math.max(1, Math.round((bmp.height / bmp.width) * w));
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const ctx = c.getContext("2d")!;
    ctx.drawImage(bmp, 0, 0, w, h);
    bmp.close();
    const d = ctx.getImageData(0, 0, w, h).data;
    const g = new Float32Array(w * h);
    for (let i = 0; i < w * h; i++) g[i] = 0.299 * d[i * 4] + 0.587 * d[i * 4 + 1] + 0.114 * d[i * 4 + 2];
    let sum = 0, sq = 0, n = 0;
    for (let y = 1; y < h - 1; y++) {
      for (let x = 1; x < w - 1; x++) {
        const i = y * w + x;
        const v = g[i - w] + g[i + w] + g[i - 1] + g[i + 1] - 4 * g[i];
        sum += v;
        sq += v * v;
        n++;
      }
    }
    const mean = sum / n;
    return sq / n - mean * mean < 60;
  } catch {
    return false;
  }
}

const MAX_SIDE: Record<string, number> = { banner: 2400, promotion: 2000, about: 2000, logo: 640, product: 1600, service: 1400, testimonial: 480, other: 1800 };

export async function uploadMedia(file: File, kind: string): Promise<Media & { warnings: string[] }> {
  if (!/^image\/(jpeg|png|webp)$/.test(file.type)) throw new Error(t("Only JPEG, PNG or WebP images can be used"));
  if (file.size > 25 * 1024 * 1024) throw new Error(t("This image is too large — use one under 25 MB"));
  const [main, thumb, blurry] = await Promise.all([optimizeImage(file, MAX_SIDE[kind] ?? 1800, 0.86), optimizeImage(file, 480, 0.78), looksBlurry(file)]);
  const fd = new FormData();
  fd.append("kind", kind);
  fd.append("name", file.name.replace(/\.[a-z0-9]+$/i, "").slice(0, 120));
  fd.append("blurry", String(blurry));
  fd.append("upload_ref", uid());
  fd.append("file", main, "image");
  fd.append("thumb", thumb, "thumb");
  return api("/website/media", { body: fd });
}

export function QualityBadge({ m }: { m: Pick<Media, "quality" | "warnings"> }) {
  return m.quality === "good"
    ? <span className="inline-flex items-center gap-1 text-xs text-success"><CheckCircle2 className="h-3.5 w-3.5" /> {t("Good")}</span>
    : <span className="inline-flex items-center gap-1 text-xs text-warning" title={m.warnings.join("\n")}><AlertTriangle className="h-3.5 w-3.5" /> {t("Warning")}</span>;
}

/** Picks an image from the business's media library, or uploads a new one (checked and graded first). */
export function MediaField({ label, value, onChange, kind, hint }: { label: string; value: string | null; onChange: (id: string | null) => void; kind: string; hint?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="space-y-1.5">
      <span className="text-[0.85rem] font-medium">{t(label)}</span>
      <div className="flex items-center gap-3">
        <button type="button" onClick={() => setOpen(true)} className="flex h-20 w-28 shrink-0 items-center justify-center overflow-hidden rounded-lg border bg-muted">
          {value ? <img src={mediaUrl(value, true)} alt="" className="h-full w-full object-cover" /> : <ImagePlus className="h-5 w-5 text-muted-foreground" />}
        </button>
        <div className="flex flex-wrap gap-1.5">
          <Button type="button" size="sm" variant="outline" onClick={() => setOpen(true)}>{value ? t("Change") : t("Choose image")}</Button>
          {value && <Button type="button" size="sm" variant="ghost" onClick={() => onChange(null)}><X /> {t("Remove")}</Button>}
        </div>
      </div>
      {hint && <p className="text-xs text-muted-foreground">{t(hint)}</p>}
      <MediaPicker open={open} onOpenChange={setOpen} kind={kind} onPick={(id) => { onChange(id); setOpen(false); }} />
    </div>
  );
}

export function MediaPicker({ open, onOpenChange, kind, onPick }: { open: boolean; onOpenChange: (o: boolean) => void; kind: string; onPick: (id: string) => void }) {
  const { data, isLoading } = useMedia(false);
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const onFile = async (file?: File) => {
    if (!file) return;
    setBusy(true);
    setRefused("");
    try {
      const m = await uploadMedia(file, kind);
      qc.invalidateQueries({ queryKey: ["website-media"] });
      if (m.warnings?.length) toast.warning(`${t("Uploaded with warnings")}: ${m.warnings.join(" · ")}`);
      onPick(m.id);
    } catch (e) {
      setRefused(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };
  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} wide title="Choose an image" description="From your website media library, or upload a new one.">
      <div className="space-y-3">
        <Button type="button" variant="outline" onClick={() => input.current?.click()} disabled={busy}>
          {busy ? <Loader2 className="animate-spin" /> : <ImagePlus />} {busy ? t("Checking & uploading…") : t("Upload image")}
        </Button>
        <input ref={input} type="file" accept="image/jpeg,image/png,image/webp" className="hidden" onChange={(e) => onFile(e.target.files?.[0])} />
        {refused && <p className="flex items-start gap-2 rounded-lg bg-destructive/10 p-3 text-sm text-destructive"><XCircle className="mt-0.5 h-4 w-4 shrink-0" /> {t("Cannot upload")}: {refused}</p>}
        {isLoading ? <Loader2 className="animate-spin" /> : (
          <div className="grid grid-cols-3 gap-2 sm:grid-cols-4">
            {(data?.items ?? []).map((m) => (
              <button key={m.id} type="button" onClick={() => onPick(m.id)} className="group overflow-hidden rounded-lg border text-start hover:ring-2 hover:ring-primary">
                <img src={mediaUrl(m.id, true)} alt="" className="aspect-square w-full object-cover" loading="lazy" />
                <span className="flex items-center justify-between gap-1 px-1.5 py-1"><span className="truncate text-[11px]">{m.name || m.kind}</span><QualityBadge m={m} /></span>
              </button>
            ))}
            {!data?.items.length && <p className="col-span-full py-6 text-center text-sm text-muted-foreground">{t("No images yet — upload your first one.")}</p>}
          </div>
        )}
      </div>
    </ResponsiveDialog>
  );
}
