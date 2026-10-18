/** Barcode stickers (roadmap 134): preview, size, layout, then Print / PDF / PNG — or a file for ZPL / TSPL label
 * printers. Receives the full codes from an audited server action; nothing here is kept after closing. */
import { useMemo, useState } from "react";
import { Download, FileImage, FileText, Printer } from "lucide-react";
import { useSession } from "@/lib/session";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { a4Grid, download, labelsPdf, labelsPng, labelsTspl, labelsZpl, labelSvg, PRESETS, printLabels, type Label, type LabelSize, type Layout } from "@/lib/labels";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";

const PREVIEW = 6;

export function LabelsDialog({ business, labels, title = "Print labels", note, onClose, onPrinted }: {
  business: string;
  labels: Label[];
  title?: string;
  /** Shown above the preview (e.g. "Attach one label to each box"). */
  note?: string;
  onClose: () => void;
  /** After Print / PDF / PNG / printer file — the receiving flow then asks to confirm the labels are attached. */
  onPrinted?: () => void;
}) {
  const { profile } = useSession();
  const cfg = profile?.settings.stock.labels ?? { width_mm: 50, height_mm: 30, printer: "system" as const };
  const [size, setSize] = useState<LabelSize>({ w: cfg.width_mm, h: cfg.height_mm });
  const [layout, setLayout] = useState<Layout>("roll");
  const [busy, setBusy] = useState(false);
  const preset = PRESETS.find((p) => p.w === size.w && p.h === size.h);
  const [custom, setCustom] = useState(!preset);
  const valid = size.w >= 25 && size.w <= 100 && size.h >= 15 && size.h <= 80 && size.h <= size.w;
  const previews = useMemo(() => (valid ? labels.slice(0, PREVIEW).map((l) => labelSvg(business, l, size)) : []), [business, labels, size, valid]);
  const base = labels.length === 1 ? `label-${labels[0].code}` : `labels-${labels.length}`;

  const run = async (what: () => Promise<void>) => {
    if (!valid || busy) return;
    setBusy(true);
    try {
      await what();
      onPrinted?.();
    } catch (e) {
      toast.error(e);
    } finally {
      setBusy(false);
    }
  };
  const printerFile = (kind: "zpl" | "tspl") => run(async () => {
    const text = kind === "zpl" ? labelsZpl(business, labels, size) : labelsTspl(business, labels, size);
    download(new Blob([text], { type: "text/plain" }), `${base}.${kind === "zpl" ? "zpl" : "prn"}`);
  });

  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title={title} confirmDiscard={false} wide
      description={`${labels.length} ${t(labels.length === 1 ? "label" : "labels")} · ${t("business name, barcode, number and product")}`}
      footer={
        <div className="flex w-full flex-wrap justify-end gap-2">
          <Button variant="outline" disabled={!valid || busy} onClick={() => run(async () => download(await labelsPng(business, labels, size), `${base}.png`))}><FileImage /> PNG</Button>
          <Button variant="outline" disabled={!valid || busy} onClick={() => run(async () => download(await labelsPdf(business, labels, size, layout), `${base}.pdf`))}><FileText /> PDF</Button>
          <Button disabled={!valid || busy} onClick={() => run(() => printLabels(business, labels, size, layout))}><Printer /> {t("Print")}</Button>
        </div>
      }>
      <div className="space-y-4">
        {note && <p className="rounded-lg bg-primary/5 p-2.5 text-xs text-primary">{t(note)}</p>}
        <Field group label="Label size">
          <div className="flex flex-wrap gap-2">
            {PRESETS.map((p) => (
              <button key={p.label} type="button" onClick={() => { setSize({ w: p.w, h: p.h }); setCustom(false); }}
                className={cn("h-9 rounded-lg border px-3 text-sm", !custom && preset?.label === p.label ? "border-primary bg-primary/5 font-medium" : "bg-card")}>{p.label}</button>
            ))}
            <button type="button" onClick={() => setCustom(true)}
              className={cn("h-9 rounded-lg border px-3 text-sm", custom ? "border-primary bg-primary/5 font-medium" : "bg-card")}>{t("Custom")}</button>
          </div>
          {custom && (
            <div className="mt-2 grid max-w-xs grid-cols-2 gap-2">
              <Field label="Width (mm)" hint="25–100"><Input type="number" inputMode="numeric" value={size.w} onChange={(e) => setSize({ ...size, w: Number(e.target.value) || 0 })} /></Field>
              <Field label="Height (mm)" hint="15–80"><Input type="number" inputMode="numeric" value={size.h} onChange={(e) => setSize({ ...size, h: Number(e.target.value) || 0 })} /></Field>
            </div>
          )}
          {!valid && <p className="mt-1 text-xs text-destructive">{t("Label size: 25–100 mm wide, 15–80 mm high, no taller than it is wide")}</p>}
        </Field>
        <Field group label="Layout">
          <div className="flex flex-wrap gap-2">
            {([["roll", "Label roll (one per label)"], ["a4", `A4 sheet (${a4Grid(size).perPage} ${t("per page")})`]] as const).map(([k, l]) => (
              <button key={k} type="button" onClick={() => setLayout(k)}
                className={cn("h-9 rounded-lg border px-3 text-sm", layout === k ? "border-primary bg-primary/5 font-medium" : "bg-card")}>{t(l)}</button>
            ))}
          </div>
        </Field>
        <div className="rounded-xl bg-muted/60 p-3">
          <p className="label-caps mb-2 text-center">{t("Preview")}</p>
          <div className="flex flex-wrap justify-center gap-3">
            {previews.map((svg, i) => (
              <img key={i} alt={`${t("Label")} ${i + 1}`} className="rounded-sm bg-white shadow-sm ring-1 ring-black/10"
                style={{ width: `${Math.min(220, size.w * 4)}px` }} src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`} />
            ))}
          </div>
          {labels.length > PREVIEW && <p className="mt-2 text-center text-xs text-muted-foreground">+{labels.length - PREVIEW} {t("more")}</p>}
        </div>
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <Download className="h-3.5 w-3.5" /> {t("Label printer file")}:
          <Button size="sm" variant={cfg.printer === "zpl" ? "secondary" : "ghost"} disabled={!valid || busy} onClick={() => printerFile("zpl")}>ZPL (Zebra)</Button>
          <Button size="sm" variant={cfg.printer === "tspl" ? "secondary" : "ghost"} disabled={!valid || busy} onClick={() => printerFile("tspl")}>TSPL (TSC)</Button>
        </div>
      </div>
    </ResponsiveDialog>
  );
}
