/** Settings → Stock → Barcode hardware (roadmap 138): the sticker size, how labels reach the printer, a test label, a
 * scanner test and honest connection guidance. Web pages cannot drive USB / Bluetooth printers directly on most
 * browsers (never on iPhone): printing goes through the system print dialog, a PDF / PNG, or a ZPL / TSPL file. */
import { useState } from "react";
import { Printer, ScanLine } from "lucide-react";
import { useSession } from "@/lib/session";
import { maskCode } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { PRESETS } from "@/lib/labels";
import type { ScanSource } from "@/lib/scan";
import type { Settings } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, Select } from "@/components/Form";
import { BarcodeInput } from "@/components/BarcodeInput";
import { LabelsDialog } from "@/components/LabelsDialog";
import { Card } from "./shared";

const SOURCE: Record<ScanSource, string> = {
  scanner: "Handheld scanner (keyboard-style)",
  camera: "Camera",
  manual: "Typed by hand",
};

export function BarcodeHardwareCard({ s, update }: { s: Settings; update: (f: (x: Settings) => void) => void }) {
  const { profile } = useSession();
  const l = s.stock.labels ?? { width_mm: 50, height_mm: 30, printer: "system" };
  const preset = PRESETS.find((p) => p.w === l.width_mm && p.h === l.height_mm);
  const [custom, setCustom] = useState(!preset);
  const [testLabel, setTestLabel] = useState(false);
  const [lastScan, setLastScan] = useState<{ code: string; source?: ScanSource } | null>(null);
  const setLabels = (patch: Partial<typeof l>) => update((x) => { x.stock.labels = { ...l, ...patch }; });
  return (
    <Card title="Barcode hardware">
      <div className="space-y-4 py-2 text-sm">
        <Field group label="Standard label size" hint="Used for every sticker unless changed when printing">
          <div className="flex flex-wrap gap-2">
            {PRESETS.map((p) => (
              <button key={p.label} type="button" onClick={() => { setCustom(false); setLabels({ width_mm: p.w, height_mm: p.h }); }}
                className={cn("h-9 rounded-lg border px-3 text-sm", !custom && preset?.label === p.label ? "border-primary bg-primary/5 font-medium" : "bg-card")}>{p.label}</button>
            ))}
            <button type="button" onClick={() => setCustom(true)} className={cn("h-9 rounded-lg border px-3 text-sm", custom ? "border-primary bg-primary/5 font-medium" : "bg-card")}>{t("Custom")}</button>
          </div>
          {custom && (
            <div className="mt-2 grid max-w-xs grid-cols-2 gap-2">
              <Field label="Width (mm)" hint="25–100"><Input type="number" inputMode="numeric" value={l.width_mm} onChange={(e) => setLabels({ width_mm: Number(e.target.value) || 0 })} /></Field>
              <Field label="Height (mm)" hint="15–80"><Input type="number" inputMode="numeric" value={l.height_mm} onChange={(e) => setLabels({ height_mm: Number(e.target.value) || 0 })} /></Field>
            </div>
          )}
        </Field>
        <Field label="Label printer" hint="System printing works with any printer installed on the phone or computer">
          <Select value={l.printer} onChange={(v) => setLabels({ printer: v as "system" })}>
            <option value="system">{t("System / browser printing (PDF, PNG, any printer)")}</option>
            <option value="zpl">{t("Zebra-language printer (ZPL file)")}</option>
            <option value="tspl">{t("TSC-language printer (TSPL file)")}</option>
          </Select>
        </Field>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={() => setTestLabel(true)}><Printer /> {t("Print a test label")}</Button>
        </div>

        <div className="space-y-2 border-t pt-3">
          <p className="font-medium">{t("Test your scanner")}</p>
          <p className="text-xs text-muted-foreground">{t("Click the field and scan any barcode, or use the camera. S'Shop shows what arrived and how.")}</p>
          <BarcodeInput context="stock" continuousCamera={false} placeholder="Scan a barcode to test"
            onCode={(code, source) => { setLastScan({ code, source }); return { tone: "success", title: t("Barcode received") }; }} />
          {lastScan && (
            <p className="flex items-center gap-2 rounded-lg bg-muted/60 p-2.5 text-xs">
              <ScanLine className="h-4 w-4 shrink-0 text-primary" />
              <span><span className="num font-semibold">{lastScan.code.length >= 7 ? maskCode(lastScan.code) : lastScan.code}</span> · {lastScan.code.length} {t("characters")} · {t(SOURCE[lastScan.source ?? "scanner"])}</span>
            </p>
          )}
        </div>

        <details className="rounded-lg bg-muted/40 p-3 text-xs text-muted-foreground">
          <summary className="cursor-pointer font-medium text-foreground">{t("Connecting printers and scanners")}</summary>
          <ul className="mt-2 list-disc space-y-1.5 ps-4">
            <li>{t("USB or Bluetooth handheld scanners: pair or plug in like a keyboard; they work in every barcode field. Set the scanner to send Enter after each code.")}</li>
            <li>{t("Phone camera: allow camera access when asked; scanning works on S'Shop's secure address.")}</li>
            <li>{t("Label printers (USB, Bluetooth, Wi-Fi): install the printer on the computer or phone and print the labels from the print dialog, choosing the label size. Use a PDF when printing from another app.")}</li>
            <li>{t("Zebra (ZPL) or TSC (TSPL) printers: download the label file and send it with the printer's own utility or app.")}</li>
            <li>{t("iPhone and most browsers do not let web pages control printers directly — S'Shop always prints through the system print dialog or a file. Labels must print at 100% scale (no 'fit to page').")}</li>
            <li>{t("A keyboard-style scanner cannot be told apart from very fast typing with certainty: S'Shop records how each barcode arrived as a strong hint, not proof. Camera scanning is the stronger option.")}</li>
          </ul>
        </details>
      </div>
      {testLabel && profile && (
        <LabelsDialog business={profile.tenant.name} title="Test label" onClose={() => setTestLabel(false)}
          labels={[{ code: "5831047", product: t("Test label — not a real item") }]} />
      )}
    </Card>
  );
}
