/** The one inline "Enter or scan barcode" field (roadmap 98): product form, stock-in, stock take and Record Sale.
 * Enter submits; the outcome shows under the field (never a popup that would catch the next scan, roadmap 153) and the
 * field clears and keeps focus for the next code. The camera
 * button opens the shared scanner when camera scanning is on. */
import { useRef, useState } from "react";
import { AlertTriangle, Camera, CheckCircle2, Info, ScanLine, XCircle } from "lucide-react";
import { useSession } from "@/lib/session";
import { toast } from "@/lib/toast";
import { errorMessage } from "@/lib/api";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { BarcodeScanner, type ScanHandler, type ScanOutcome } from "@/components/BarcodeScanner";
import { SShopLoader } from "@/components/SShopLoader";
import { ScanCounter, type ScanCounterProps } from "@/components/ScanCounter";
import { outcomeSound, scanFeedback } from "@/lib/scanner";
import { DOUBLE_READ_MS, TYPING_OFF, useKeyTiming, useManualBarcodes, type ScanSource } from "@/lib/scan";

export function useBarcodeSettings() {
  const { profile } = useSession();
  const st = profile?.settings.stock;
  return {
    enabled: st?.barcode_requirement !== "disabled",
    manual: st?.manual_barcode_entry ?? true,
    camera: st?.camera_scanning ?? true,
  };
}

export function BarcodeInput({ onCode, placeholder, autoFocus, className, continuousCamera = true, cameraTitle = "Scan barcode", context = "stock", session }: {
  onCode: ScanHandler;
  /** Roadmap 137: "sale" follows the till's barcode rule (typing off by default); "stock" the stock rule. */
  context?: "sale" | "stock";
  placeholder?: string;
  autoFocus?: boolean;
  className?: string;
  continuousCamera?: boolean;
  cameraTitle?: string;
  /** Roadmap 153: the live count of this scanning session, shown under the field and in the camera scanner. */
  session?: Omit<ScanCounterProps, "tone" | "onScanned" | "onDone">;
}) {
  const cfg = { ...useBarcodeSettings(), manual: useManualBarcodes(context) };
  const timing = useKeyTiming();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ScanOutcome | null>(null);
  const [camera, setCamera] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  // Roadmap 153: the field clears the moment a code is submitted and codes are handled strictly in order, so a handheld
  // scanner firing again before the last code is checked neither loses a scan nor runs two into one.
  const chain = useRef<Promise<void>>(Promise.resolve());
  const waiting = useRef(0);
  const last = useRef({ code: "", at: 0 });
  const handle = async (code: string, source: ScanSource) => {
    try {
      const r = (await onCode(code, source)) || null;
      setResult(r);
      scanFeedback(outcomeSound(r ? r.tone : "success"));
    } catch (e) {
      scanFeedback("error");
      setResult({ tone: "error", title: errorMessage(e) });
    } finally {
      waiting.current -= 1;
      if (waiting.current === 0) setBusy(false);
      input.current?.focus();
    }
  };
  const submit = (raw: string) => {
    const code = raw.trim();
    setValue("");
    if (!code) return;
    const source = timing.source(code);
    if (source === "manual" && !cfg.manual) {
      setResult({ tone: "error", title: TYPING_OFF });
      scanFeedback("error");
      return;
    }
    const now = Date.now();
    if (code === last.current.code && now - last.current.at < DOUBLE_READ_MS) return;
    last.current = { code, at: now };
    waiting.current += 1;
    setBusy(true);
    chain.current = chain.current.then(() => handle(code, source));
  };

  const Icon = !result ? null : result.tone === "success" ? CheckCircle2 : result.tone === "error" ? XCircle : result.tone === "warning" ? AlertTriangle : Info;
  return (
    <div className={cn("space-y-1", className)}>
      <div className="flex gap-2">
        <div className="relative flex-1">
          <ScanLine className="pointer-events-none absolute inset-s-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input ref={input} value={value} autoFocus={autoFocus} className="num ps-9" autoComplete="off" enterKeyHint="go"
            placeholder={t(placeholder ?? (cfg.manual ? "Enter or scan barcode" : "Scan barcode"))} aria-label={t("Enter or scan barcode")}
            onPaste={(e) => { timing.onPaste(); if (!cfg.manual) { e.preventDefault(); toast.error(TYPING_OFF); } }}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => { timing.onKeyDown(e); if (e.key === "Enter") { e.preventDefault(); submit(value); } }} />
          {busy && <SShopLoader size="sm" className="absolute inset-e-3 top-1/2 -translate-y-1/2" />}
        </div>
        {cfg.camera && <Button type="button" variant="outline" size="icon" onClick={() => setCamera(true)} aria-label={t("Scan with camera")}><Camera /></Button>}
      </div>
      {result && Icon && !(session?.lines.length && result.tone === "success") && (
        <p role="status" aria-live="polite" className={cn("flex items-start gap-1.5 text-xs", result.tone === "success" ? "text-success" : result.tone === "error" ? "text-destructive" : result.tone === "warning" ? "text-warning" : "text-muted-foreground")}>
          <Icon className="mt-0.5 h-3.5 w-3.5 shrink-0" /> <span>{result.title}{result.detail ? <> · {result.detail}</> : null}</span>
        </p>
      )}
      {session && <ScanCounter {...session} className="mt-2" onScanned={(code) => submit(code)} onDone={() => input.current?.focus()} />}
      {cfg.camera && (
        <BarcodeScanner session={session} open={camera} onOpenChange={setCamera} continuous={continuousCamera} title={cameraTitle} context={context}
          onDetected={async (code, source) => { const r = (await onCode(code, source)) || undefined; setResult(r ?? null); if (!continuousCamera) setCamera(false); return r; }} />
      )}
    </div>
  );
}
