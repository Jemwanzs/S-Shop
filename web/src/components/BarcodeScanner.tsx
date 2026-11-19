import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { AlertTriangle, CameraOff, Check, Flashlight, FlashlightOff, ImageUp, Keyboard, RefreshCw, SwitchCamera, X, ZoomIn } from "lucide-react";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { ApiError } from "@/lib/api";
import { cameraProblemText, decodePhoto, outcomeSound, scanFeedback, startScanning, type CameraProblem, type CameraSession, type CameraStats } from "@/lib/scanner";
import { useIsDesktop } from "@/lib/hooks";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SShopLoader } from "@/components/SShopLoader";
import { ScanCounter, type ScanCounterProps } from "@/components/ScanCounter";
import { DOUBLE_READ_MS, TYPING_OFF, useKeyTiming, useManualBarcodes, type ScanSource } from "@/lib/scan";
import { RepeatGuard } from "@/lib/scanRepeat";

/** What a scan led to, shown inside the scanner so the user never has to leave it. */
export interface ScanOutcome {
  /** `warning` (roadmap 153): read fine, but nothing was added — e.g. an individual barcode already scanned. */
  tone: "success" | "error" | "warning" | "info";
  title: string;
  detail?: ReactNode;
  /** Extra actions for errors (e.g. "Search product", "Assign barcode"). "Scan again" is always offered. */
  actions?: { label: string; onClick: () => void }[];
}
/** `source` (roadmap 137): how the code arrived — handheld scanner, camera or typed. */
export type ScanHandler = (code: string, source?: ScanSource) => void | ScanOutcome | Promise<void | ScanOutcome>;

/** Roadmap 163: the same barcode seen again too soon. Counts down by itself, so the scanner does not re-render each tick. */
function RepeatWait({ until }: { until: number }) {
  const [left, setLeft] = useState(() => until - performance.now());
  useEffect(() => {
    const id = window.setInterval(() => {
      const l = until - performance.now();
      setLeft(l);
      if (l < -2000) window.clearInterval(id);
    }, 100);
    return () => window.clearInterval(id);
  }, [until]);
  if (left < -2000) return null;
  return left > 0
    ? <span className="num text-white/85">{t("Please wait — same barcode ready in")} {(left / 1000).toFixed(1)}s</span>
    : <span className="text-emerald-300">{t("Ready to scan again")}</span>;
}

/** Roadmap 177: guidance when nothing has been read for a while — never during normal scanning. */
const NO_READ_MS = 6000;
export const SCAN_TIPS = {
  none: "Barcode not detected. Try adjusting the angle, moving closer or reducing reflections.",
  glare: "Strong reflection — tilt the pack slightly so the light does not shine straight back. Shiny gold or silver labels read best at a small angle.",
  dark: "Too dark to read — turn on the torch or move to better light.",
  darkNoTorch: "Too dark to read — move to better light.",
  far: "Still not read — move closer so the barcode fills the frame, or zoom in.",
} as const;

/**
 * The one barcode scanner used across S'Shop: phone camera (full screen on phones), handheld scanners
 * (keyboard wedge into the code field + Enter) and manual entry all go through `onDetected`.
 *
 * - `continuous`: stay open after successful scans (selling, receiving, transfers, counts); errors pause
 *   scanning until "Scan again".
 * - Otherwise the scanner closes after a successful scan; an error outcome keeps it open.
 * The camera starts only when the scanner opens and stops on close, unmount, or when the app goes to the
 * background.
 */
export function BarcodeScanner({
  open,
  onOpenChange,
  onDetected,
  title = "Scan barcode",
  continuous = false,
  hint,
  context = "stock",
  session: tally,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  onDetected: ScanHandler;
  title?: string;
  continuous?: boolean;
  hint?: string;
  /** Roadmap 137: "sale" follows the till's barcode rule (typing off by default); "stock" the stock rule. */
  context?: "sale" | "stock";
  /** Roadmap 153: the live count of this scanning session, with its corrections. */
  session?: Omit<ScanCounterProps, "tone" | "onScanned" | "onDone">;
}) {
  const desktop = useIsDesktop();
  // The dialog mounts its content after opening, so the video element arrives as state, not a ref read too early.
  const [video, setVideo] = useState<HTMLVideoElement | null>(null);
  const session = useRef<CameraSession | null>(null);
  const [run, setRun] = useState(0); // bump to restart the camera
  const [deviceId, setDeviceId] = useState<string | undefined>();
  const [camera, setCamera] = useState<{ state: "starting" | "live" | "off"; problem?: CameraProblem; torch?: boolean; canSwitch?: boolean }>({ state: "starting" });
  const [torchOn, setTorchOn] = useState(false);
  // Roadmap 177: zoom (where the camera has it), guidance after a while without a read, and reading from a photo.
  const [zoom, setZoom] = useState<{ min: number; max: number; at: number } | null>(null);
  const [tip, setTip] = useState<keyof typeof SCAN_TIPS | null>(null);
  const stats = useRef<CameraStats | null>(null);
  const liveSince = useRef(0);
  const lastRead = useRef(0);
  const photoInput = useRef<HTMLInputElement>(null);
  const [readingPhoto, setReadingPhoto] = useState(false);
  const [busy, setBusy] = useState(false);
  const [detected, setDetected] = useState(false);
  const [outcome, setOutcome] = useState<(ScanOutcome & { code: string }) | null>(null);
  const [manual, setManual] = useState("");
  const manualInput = useRef<HTMLInputElement>(null);
  // Roadmap 98 / 137: the business may switch typed / pasted barcodes off (handheld scanners still work in this field).
  const manualAllowed = useManualBarcodes(context);
  const timing = useKeyTiming();
  const paused = useRef(false);
  const last = useRef({ code: "", at: 0 });
  // Roadmap 163: camera reads of the same barcode, 3 s apart; other barcodes are never held back.
  const repeat = useRef(new RepeatGuard());
  const [wait, setWait] = useState<{ code: string; until: number } | null>(null);
  const waitRef = useRef(wait);
  waitRef.current = wait;
  const handler = useRef(onDetected);
  handler.current = onDetected;

  const close = useCallback(() => onOpenChange(false), [onOpenChange]);

  // One path for camera, handheld and typed codes. Scans run strictly one after another (roadmap 153): a handheld scanner
  // can fire again before the previous code is checked, and each must see the count the previous one left.
  const chain = useRef<Promise<void>>(Promise.resolve());
  const resume = useRef(0);
  const runScan = useCallback(
    async (code: string, source: ScanSource) => {
      let result: void | ScanOutcome;
      try {
        result = await handler.current(code, source);
      } catch (e) {
        result =
          e instanceof ApiError && e.title
            ? { tone: "error", title: e.title, detail: e.message }
            : { tone: "error", title: e instanceof Error ? e.message : String(e) };
      }
      setBusy(false);
      // One sound per scan, once its result is known.
      scanFeedback(outcomeSound(result ? result.tone : "success"));
      if (result && result.tone === "error") {
        // Refused, so not counted: after "Scan again" the same barcode may be tried at once.
        repeat.current.release(code);
        setOutcome({ ...result, code });
        return; // stays paused until "Scan again"
      }
      if (!continuous) {
        close();
        return;
      }
      setOutcome(result ? { ...result, code } : { tone: "success", title: t("Scanned"), code });
      // Ready for the next barcode at once; the green frame stays a moment as confirmation.
      paused.current = false;
      window.clearTimeout(resume.current);
      resume.current = window.setTimeout(() => setDetected(false), 700);
    },
    [continuous, close],
  );
  const process = useCallback(
    (raw: string, source: ScanSource) => {
      const fromCamera = source === "camera";
      const code = raw.trim();
      if (!code || (fromCamera && paused.current)) return;
      const now = Date.now();
      lastRead.current = now;
      setTip(null);
      if (fromCamera) {
        // The camera sees a label many times a second: the same barcode counts again only 3 s after it was accepted.
        const c = repeat.current.claim(code);
        if (!c.ok) {
          if (waitRef.current?.code !== code || waitRef.current.until !== c.until) setWait({ code, until: c.until });
          return; // no count, no sound
        }
        if (waitRef.current) setWait(null);
      } else {
        // A handheld trigger press can reach the page twice; the same code again this fast is one scan.
        if (code === last.current.code && now - last.current.at < DOUBLE_READ_MS) return;
        last.current = { code, at: now };
      }
      paused.current = true;
      setBusy(true);
      setDetected(true);
      chain.current = chain.current.then(() => runScan(code, source));
    },
    [runScan],
  );

  // The camera calls the latest handler without restarting: a quantity tap re-renders the screen around the scanner, and
  // restarting the camera on each one would make it flicker and miss reads (roadmap 165).
  const processRef = useRef(process);
  processRef.current = process;

  const scanAgain = () => {
    setOutcome(null);
    setDetected(false);
    setWait(null);
    last.current = { code: "", at: 0 };
    paused.current = false;
  };

  // A fresh start each time the scanner opens; nothing is kept once it closes.
  useEffect(() => {
    if (open) {
      setOutcome(null);
      setDetected(false);
      setWait(null);
      setManual("");
      setTorchOn(false);
      setTip(null);
      lastRead.current = 0;
      paused.current = false;
      last.current = { code: "", at: 0 };
      repeat.current.clear();
    }
    const guard = repeat.current;
    return () => {
      guard.clear();
      window.clearTimeout(resume.current);
    };
  }, [open]);

  // Camera lifecycle.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setCamera({ state: "starting" });
    if (!video) return;
    startScanning(video, (code) => processRef.current(code, "camera"), deviceId, (s) => { stats.current = s; })
      .then((s) => {
        if (cancelled) return s.stop();
        session.current = s;
        liveSince.current = Date.now();
        setZoom(s.zoom ? { ...s.zoom, at: s.zoom.min } : null);
        setCamera({ state: "live", torch: s.torchSupported, canSwitch: s.cameras.length > 1 });
      })
      .catch((problem: CameraProblem) => {
        if (!cancelled) setCamera({ state: "off", problem: typeof problem === "string" ? problem : "failed" });
      });
    // Never keep the camera running in the background.
    const onVisibility = () => {
      if (document.hidden) {
        session.current?.stop();
        session.current = null;
        setCamera({ state: "off", problem: undefined });
      } else {
        setRun((r) => r + 1);
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", onVisibility);
      session.current?.stop();
      session.current = null;
    };
  }, [open, video, deviceId, run]);

  // Roadmap 177: after a while without any read, one short tip — chosen from what the camera sees.
  const idle = open && camera.state === "live" && !busy && !outcome && !wait;
  useEffect(() => {
    if (!idle) return;
    const id = window.setInterval(() => {
      const since = Math.max(liveSince.current, lastRead.current);
      const quiet = Date.now() - since;
      if (quiet < NO_READ_MS) return;
      const s = stats.current;
      const next: keyof typeof SCAN_TIPS = s && s.mean < 45 ? (camera.torch && !torchOn ? "dark" : "darkNoTorch")
        : s && s.glare > 0.06 ? "glare"
        : quiet > NO_READ_MS * 2 ? "far" : "none";
      setTip((t0) => (t0 === next ? t0 : next));
    }, 1000);
    return () => window.clearInterval(id);
  }, [idle, camera.torch, torchOn]);

  const changeZoom = async () => {
    if (!zoom || !session.current) return;
    // 1× → 2× (or the most it has) → back.
    const next = zoom.at < Math.min(2, zoom.max) ? Math.min(2, zoom.max) : zoom.min;
    try {
      await session.current.setZoom(next);
      setZoom({ ...zoom, at: next });
      setTip(null);
    } catch {
      setZoom(null);
    }
  };

  const readPhoto = async (file: File | undefined) => {
    if (!file) return;
    setReadingPhoto(true);
    setOutcome(null);
    try {
      const code = await decodePhoto(file);
      if (code) {
        paused.current = false;
        process(code, "camera");
      } else {
        paused.current = true;
        scanFeedback("error");
        // A picture without a readable barcode is not a barcode mismatch or an unknown product.
        setOutcome({ tone: "error", title: t("No barcode could be read from this photo"), detail: t("Take it again closer and straight on, without reflections — or type the code."), code: "" });
      }
    } catch {
      setOutcome({ tone: "error", title: t("This photo could not be opened"), code: "" });
    } finally {
      setReadingPhoto(false);
      if (photoInput.current) photoInput.current.value = "";
    }
  };

  const toggleTorch = async () => {
    try {
      await session.current?.setTorch(!torchOn);
      setTorchOn(!torchOn);
    } catch {
      setCamera((c) => ({ ...c, torch: false }));
    }
  };
  const switchCamera = () => {
    const s = session.current;
    if (!s || s.cameras.length < 2) return;
    const i = s.cameras.findIndex((c) => c.deviceId === s.deviceId);
    setTorchOn(false);
    setDeviceId(s.cameras[(i + 1) % s.cameras.length].deviceId);
  };
  const submitManual = (e: React.FormEvent) => {
    e.preventDefault();
    const code = manual;
    setManual("");
    setOutcome(null);
    const source = timing.source(code.trim());
    if (source === "manual" && !manualAllowed) {
      scanFeedback("error");
      setOutcome({ tone: "error", title: t(TYPING_OFF), code: code.trim() });
      return;
    }
    process(code, source);
  };

  const frameState = outcome?.tone === "error" ? "error" : detected ? "ok" : "scanning";
  // The counter already shows an accepted scan (product, barcode, quantity, tick).
  const counted = !!tally?.lines.length;
  const refocus = () => { if (desktop) manualInput.current?.focus(); };

  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-[hsl(var(--overlay)/0.6)] backdrop-blur-[2px] data-[state=open]:animate-in data-[state=open]:fade-in-0" />
        <DialogPrimitive.Content
          onPointerDownOutside={(e) => e.preventDefault()}
          onInteractOutside={(e) => e.preventDefault()}
          aria-describedby={undefined}
          onOpenAutoFocus={(e) => !desktop && e.preventDefault()}
          className={cn(
            "fixed z-50 flex flex-col overflow-hidden bg-black text-white outline-hidden",
            // Phones: the camera takes the whole screen. Larger screens: a compact centred scanner.
            "inset-0 md:inset-auto md:left-1/2 md:top-1/2 md:w-[min(30rem,92vw)] md:-translate-x-1/2 md:-translate-y-1/2 md:rounded-2xl md:border md:border-white/10",
          )}
        >
          {/* Top bar */}
          <div className="absolute inset-x-0 top-0 z-20 flex items-center gap-2 bg-linear-to-b from-black/70 to-transparent px-3 pb-6 pt-[max(0.75rem,env(safe-area-inset-top))]">
            <DialogPrimitive.Close className="flex h-10 w-10 items-center justify-center rounded-full bg-white/10 backdrop-blur-sm" aria-label={t("Close")}>
              <X className="h-5 w-5" />
            </DialogPrimitive.Close>
            <div className="min-w-0 flex-1 text-center">
              <DialogPrimitive.Title className="truncate text-sm font-semibold">{t(title)}</DialogPrimitive.Title>
              {hint && <p className="truncate text-xs text-white/70">{t(hint)}</p>}
            </div>
            {camera.torch ? (
              <button onClick={toggleTorch} className={cn("flex h-10 w-10 items-center justify-center rounded-full backdrop-blur-sm", torchOn ? "bg-white text-black" : "bg-white/10")} aria-label={t("Torch")}>
                {torchOn ? <Flashlight className="h-5 w-5" /> : <FlashlightOff className="h-5 w-5" />}
              </button>
            ) : (
              <span className="h-10 w-10" />
            )}
            {zoom && camera.state === "live" && (
              <button onClick={changeZoom} className={cn("flex h-10 min-w-10 items-center justify-center gap-0.5 rounded-full px-2 text-xs font-semibold backdrop-blur-sm", zoom.at > zoom.min ? "bg-white text-black" : "bg-white/10")} aria-label={t("Zoom")}>
                <ZoomIn className="h-4 w-4" />{zoom.at > zoom.min ? `${Math.round(zoom.at * 10) / 10}×` : ""}
              </button>
            )}
            {camera.canSwitch && (
              <button onClick={switchCamera} className="flex h-10 w-10 items-center justify-center rounded-full bg-white/10 backdrop-blur-sm" aria-label={t("Switch camera")}>
                <SwitchCamera className="h-5 w-5" />
              </button>
            )}
          </div>

          {/* Camera */}
          <div className="relative min-h-0 flex-1 md:h-[min(22.5rem,40vh)] md:flex-none">
            <video ref={setVideo} className="absolute inset-0 h-full w-full object-cover" playsInline muted />
            {camera.state === "live" && (
              <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
                <div
                  className={cn(
                    "relative h-[min(34vw,9.5rem)] w-[min(80vw,21rem)] rounded-2xl shadow-[0_0_0_9999px_rgba(0,0,0,0.45)] transition-colors md:h-36 md:w-[80%]",
                    frameState === "ok" ? "ring-[3px] ring-emerald-400" : frameState === "error" ? "ring-[3px] ring-red-400" : "ring-2 ring-white/85",
                  )}
                >
                  {frameState === "scanning" && <span className="absolute inset-x-4 top-1/2 h-0.5 -translate-y-1/2 animate-pulse rounded bg-[hsl(var(--primary))] shadow-[0_0_12px_hsl(var(--primary))]" />}
                </div>
                <p className="mt-4 rounded-full bg-black/55 px-3 py-1 text-xs font-medium backdrop-blur-sm">
                  {busy ? <span className="inline-flex items-center gap-1.5"><SShopLoader size="sm" />{t("Checking…")}</span>
                    : frameState === "ok" ? <span className="inline-flex items-center gap-1.5 text-emerald-300"><Check className="h-3.5 w-3.5" />{t(outcome?.tone === "success" ? "Scan accepted" : "Barcode detected")}</span>
                    : frameState === "error" ? t("Scanning paused")
                    : wait ? <RepeatWait key={`${wait.code}-${wait.until}`} until={wait.until} />
                    : t(counted ? "Scan the next barcode. The same one counts again after 3 seconds" : "Position the barcode inside the frame")}
                </p>
                {tip && frameState === "scanning" && !wait && (
                  <p role="status" data-testid="scan-tip" className="mx-6 mt-2 max-w-sm rounded-xl bg-black/65 px-3 py-1.5 text-center text-xs text-amber-100 backdrop-blur-sm">{t(SCAN_TIPS[tip])}</p>
                )}
              </div>
            )}
            {camera.state === "starting" && (
              <div className="absolute inset-0 flex items-center justify-center text-sm text-white/80"><SShopLoader size="sm" className="me-2" />{t("Starting camera…")}</div>
            )}
            {camera.state === "off" && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 px-8 text-center">
                <CameraOff className="h-8 w-8 text-white/60" />
                <p className="text-sm text-white/85">{camera.problem ? t(cameraProblemText(camera.problem)) : t("Camera paused")}</p>
                {camera.problem !== "insecure" && camera.problem !== "unsupported" && camera.problem !== "no-camera" && (
                  <Button size="sm" variant="outline" className="border-white/30 bg-transparent text-white hover:bg-white/10 hover:text-white" onClick={() => setRun((r) => r + 1)}>
                    <RefreshCw /> {t("Try again")}
                  </Button>
                )}
              </div>
            )}
          </div>

          {/* Result + handheld / manual entry */}
          <div className="relative z-20 max-h-[58dvh] space-y-2.5 overflow-y-auto bg-black px-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] pt-3">
            {tally && <ScanCounter {...tally} tone="dark" onScanned={(code) => process(code, "scanner")} onDone={refocus} />}
            {outcome && !(counted && outcome.tone === "success") && (
              <div
                role="status"
                className={cn(
                  "rounded-xl px-3 py-2.5 text-sm animate-fade-up",
                  outcome.tone === "success" ? "bg-emerald-500/15 text-emerald-100" : outcome.tone === "error" ? "bg-red-500/15 text-red-100" : outcome.tone === "warning" ? "bg-amber-500/15 text-amber-100" : "bg-white/10 text-white",
                )}
              >
                <p className="flex items-start gap-2 font-semibold">
                  {outcome.tone === "success" ? <Check className="mt-0.5 h-4 w-4 shrink-0 text-emerald-300" /> : outcome.tone === "error" ? <X className="mt-0.5 h-4 w-4 shrink-0 text-red-300" /> : outcome.tone === "warning" ? <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-300" /> : null}
                  <span className="min-w-0">{t(outcome.title)}</span>
                </p>
                {outcome.code && <p className="num mt-0.5 ps-6 text-xs opacity-70">{outcome.code}</p>}
                {outcome.detail && <div className="mt-1.5 ps-6 text-xs opacity-90">{typeof outcome.detail === "string" ? t(outcome.detail) : outcome.detail}</div>}
                {outcome.tone === "error" && (
                  <div className="mt-2.5 flex flex-wrap gap-2 ps-6">
                    <Button size="sm" onClick={scanAgain}>{t("Scan again")}</Button>
                    {outcome.actions?.map((a) => (
                      <Button key={a.label} size="sm" variant="outline" className="border-white/25 bg-transparent text-white hover:bg-white/10 hover:text-white" onClick={a.onClick}>
                        {t(a.label)}
                      </Button>
                    ))}
                  </div>
                )}
              </div>
            )}
            <form onSubmit={submitManual} className="flex gap-2">
              <div className="relative flex-1">
                <Keyboard className="pointer-events-none absolute inset-s-3 top-1/2 h-4 w-4 -translate-y-1/2 text-white/50" />
                <Input
                  ref={manualInput}
                  value={manual}
                  onChange={(e) => setManual(e.target.value)}
                  onPaste={(e) => { timing.onPaste(); if (!manualAllowed) e.preventDefault(); }}
                  onKeyDown={timing.onKeyDown}
                  autoFocus={desktop}
                  placeholder={t(manualAllowed ? "Type or use a handheld scanner" : "Use a handheld scanner")}
                  aria-label={t("Barcode")}
                  className="num border-white/15 bg-white/10 ps-9 text-white placeholder:text-white/45 focus-visible:ring-offset-0"
                />
              </div>
              <Button type="submit" disabled={!manual.trim()}>{t("Add")}</Button>
            </form>
            {/* Roadmap 177: the phone's camera app takes a sharper still of shiny or curved labels than the live preview. */}
            <input ref={photoInput} type="file" accept="image/*" capture="environment" className="hidden" data-testid="scan-photo-input" onChange={(e) => readPhoto(e.target.files?.[0])} />
            <Button type="button" variant="outline" disabled={readingPhoto || busy} onClick={() => photoInput.current?.click()}
              className="w-full border-white/25 bg-transparent text-white hover:bg-white/10 hover:text-white">
              {readingPhoto ? <SShopLoader size="sm" /> : <ImageUp />} {t(readingPhoto ? "Reading the photo…" : "Scan from photo")}
            </Button>
            {continuous && (
              <Button variant="outline" className="w-full border-white/25 bg-transparent text-white hover:bg-white/10 hover:text-white" onClick={close}>
                {t("Done scanning")}
              </Button>
            )}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
