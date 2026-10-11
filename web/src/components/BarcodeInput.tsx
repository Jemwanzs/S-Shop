/** The one inline "Enter or scan barcode" field (roadmap 98): product form, stock-in, stock take and Record Sale.
 * Enter submits; the outcome shows under the field and the field clears and keeps focus for the next code. The camera
 * button opens the shared scanner when camera scanning is on. */
import { useRef, useState } from "react";
import { Camera, CheckCircle2, Info, ScanLine, XCircle } from "lucide-react";
import { useSession } from "@/lib/session";
import { toast } from "@/lib/toast";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { BarcodeScanner, type ScanHandler, type ScanOutcome } from "@/components/BarcodeScanner";
import { SShopLoader } from "@/components/SShopLoader";

export function useBarcodeSettings() {
  const { profile } = useSession();
  const st = profile?.settings.stock;
  return {
    enabled: st?.barcode_requirement !== "disabled",
    manual: st?.manual_barcode_entry ?? true,
    camera: st?.camera_scanning ?? true,
  };
}

export function BarcodeInput({ onCode, placeholder, autoFocus, className, continuousCamera = true, cameraTitle = "Scan barcode" }: {
  onCode: ScanHandler;
  placeholder?: string;
  autoFocus?: boolean;
  className?: string;
  continuousCamera?: boolean;
  cameraTitle?: string;
}) {
  const cfg = useBarcodeSettings();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ScanOutcome | null>(null);
  const [camera, setCamera] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const submit = async (raw: string) => {
    const code = raw.trim();
    if (!code || busy) return;
    setBusy(true);
    try {
      const r = (await onCode(code)) || null;
      setResult(r);
      if (r?.tone === "error") toast.error(r.title);
    } finally {
      setBusy(false);
      setValue("");
      input.current?.focus();
    }
  };

  const Icon = !result ? null : result.tone === "success" ? CheckCircle2 : result.tone === "error" ? XCircle : Info;
  return (
    <div className={cn("space-y-1", className)}>
      <div className="flex gap-2">
        <div className="relative flex-1">
          <ScanLine className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input ref={input} value={value} autoFocus={autoFocus} className="num ps-9" autoComplete="off" enterKeyHint="go"
            placeholder={t(placeholder ?? (cfg.manual ? "Enter or scan barcode" : "Scan barcode"))} aria-label={t("Enter or scan barcode")}
            onPaste={(e) => { if (!cfg.manual) { e.preventDefault(); toast.error("Typing barcodes is switched off — scan the label"); } }}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void submit(value); } }} />
          {busy && <SShopLoader size="sm" className="absolute end-3 top-1/2 -translate-y-1/2" />}
        </div>
        {cfg.camera && <Button type="button" variant="outline" size="icon" onClick={() => setCamera(true)} aria-label={t("Scan with camera")}><Camera /></Button>}
      </div>
      {result && Icon && (
        <p role="status" aria-live="polite" className={cn("flex items-start gap-1.5 text-xs", result.tone === "success" ? "text-success" : result.tone === "error" ? "text-destructive" : "text-muted-foreground")}>
          <Icon className="mt-0.5 h-3.5 w-3.5 shrink-0" /> <span>{result.title}{result.detail ? <> · {result.detail}</> : null}</span>
        </p>
      )}
      {cfg.camera && (
        <BarcodeScanner open={camera} onOpenChange={setCamera} continuous={continuousCamera} title={cameraTitle}
          onDetected={async (code) => { const r = (await onCode(code)) || undefined; setResult(r ?? null); if (!continuousCamera) setCamera(false); return r; }} />
      )}
    </div>
  );
}
