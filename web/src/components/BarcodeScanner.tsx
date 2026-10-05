import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { CameraOff, Check, Flashlight, FlashlightOff, Keyboard, Loader2, RefreshCw, SwitchCamera, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { cameraProblemText, scanFeedback, startScanning, type CameraProblem, type CameraSession } from "@/lib/scanner";
import { useIsDesktop } from "@/lib/hooks";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** What a scan led to, shown inside the scanner so the user never has to leave it. */
export interface ScanOutcome {
  tone: "success" | "error" | "info";
  title: string;
  detail?: ReactNode;
  /** Extra actions for errors (e.g. "Search product", "Assign barcode"). "Scan again" is always offered. */
  actions?: { label: string; onClick: () => void }[];
}
export type ScanHandler = (code: string) => void | ScanOutcome | Promise<void | ScanOutcome>;

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
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  onDetected: ScanHandler;
  title?: string;
  continuous?: boolean;
  hint?: string;
}) {
  const desktop = useIsDesktop();
  // The dialog mounts its content after opening, so the video element arrives as state, not a ref read too early.
  const [video, setVideo] = useState<HTMLVideoElement | null>(null);
  const session = useRef<CameraSession | null>(null);
  const [run, setRun] = useState(0); // bump to restart the camera
  const [deviceId, setDeviceId] = useState<string | undefined>();
  const [camera, setCamera] = useState<{ state: "starting" | "live" | "off"; problem?: CameraProblem; torch?: boolean; canSwitch?: boolean }>({ state: "starting" });
  const [torchOn, setTorchOn] = useState(false);
  const [busy, setBusy] = useState(false);
  const [detected, setDetected] = useState(false);
  const [outcome, setOutcome] = useState<(ScanOutcome & { code: string }) | null>(null);
  const [manual, setManual] = useState("");
  const paused = useRef(false);
  const last = useRef({ code: "", at: 0 });
  const handler = useRef(onDetected);
  handler.current = onDetected;

  const close = useCallback(() => onOpenChange(false), [onOpenChange]);

  // One path for camera, handheld and typed codes.
  const process = useCallback(
    async (raw: string, fromCamera: boolean) => {
      const code = raw.trim();
      if (!code || paused.current) return;
      const now = Date.now();
      // The camera sees the same label many times a second: count it once until it leaves view for a moment.
      if (fromCamera && code === last.current.code && now - last.current.at < 1500) {
        last.current.at = now;
        return;
      }
      last.current = { code, at: now };
      paused.current = true;
      setBusy(true);
      setDetected(true);
      scanFeedback(true);
      let result: void | ScanOutcome;
      try {
        result = await handler.current(code);
      } catch (e) {
        result = { tone: "error", title: e instanceof Error ? e.message : String(e) };
      }
      setBusy(false);
      if (result && result.tone === "error") {
        scanFeedback(false);
        setOutcome({ ...result, code });
        return; // stays paused until "Scan again"
      }
      if (!continuous) {
        close();
        return;
      }
      setOutcome(result ? { ...result, code } : { tone: "success", title: t("Scanned"), code });
      window.setTimeout(() => {
        setDetected(false);
        paused.current = false;
      }, 700);
    },
    [continuous, close],
  );

  const scanAgain = () => {
    setOutcome(null);
    setDetected(false);
    last.current = { code: "", at: 0 };
    paused.current = false;
  };

  // Reset each time the scanner opens.
  useEffect(() => {
    if (open) {
      setOutcome(null);
      setDetected(false);
      setManual("");
      setTorchOn(false);
      paused.current = false;
      last.current = { code: "", at: 0 };
    }
  }, [open]);

  // Camera lifecycle.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setCamera({ state: "starting" });
    if (!video) return;
    startScanning(video, (code) => process(code, true), deviceId)
      .then((s) => {
        if (cancelled) return s.stop();
        session.current = s;
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
  }, [open, video, deviceId, run, process]);

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
    last.current = { code: "", at: 0 };
    paused.current = false;
    setOutcome(null);
    void process(code, false);
  };

  const frameState = outcome?.tone === "error" ? "error" : detected ? "ok" : "scanning";

  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-[hsl(var(--overlay)/0.6)] backdrop-blur-[2px] data-[state=open]:animate-in data-[state=open]:fade-in-0" />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          onOpenAutoFocus={(e) => !desktop && e.preventDefault()}
          className={cn(
            "fixed z-50 flex flex-col overflow-hidden bg-black text-white outline-none",
            // Phones: the camera takes the whole screen. Larger screens: a compact centred scanner.
            "inset-0 md:inset-auto md:left-1/2 md:top-1/2 md:w-[min(30rem,92vw)] md:-translate-x-1/2 md:-translate-y-1/2 md:rounded-2xl md:border md:border-white/10",
          )}
        >
          {/* Top bar */}
          <div className="absolute inset-x-0 top-0 z-20 flex items-center gap-2 bg-gradient-to-b from-black/70 to-transparent px-3 pb-6 pt-[max(0.75rem,env(safe-area-inset-top))]">
            <DialogPrimitive.Close className="flex h-10 w-10 items-center justify-center rounded-full bg-white/10 backdrop-blur" aria-label={t("Close")}>
              <X className="h-5 w-5" />
            </DialogPrimitive.Close>
            <div className="min-w-0 flex-1 text-center">
              <DialogPrimitive.Title className="truncate text-sm font-semibold">{t(title)}</DialogPrimitive.Title>
              {hint && <p className="truncate text-xs text-white/70">{t(hint)}</p>}
            </div>
            {camera.torch ? (
              <button onClick={toggleTorch} className={cn("flex h-10 w-10 items-center justify-center rounded-full backdrop-blur", torchOn ? "bg-white text-black" : "bg-white/10")} aria-label={t("Torch")}>
                {torchOn ? <Flashlight className="h-5 w-5" /> : <FlashlightOff className="h-5 w-5" />}
              </button>
            ) : (
              <span className="h-10 w-10" />
            )}
            {camera.canSwitch && (
              <button onClick={switchCamera} className="flex h-10 w-10 items-center justify-center rounded-full bg-white/10 backdrop-blur" aria-label={t("Switch camera")}>
                <SwitchCamera className="h-5 w-5" />
              </button>
            )}
          </div>

          {/* Camera */}
          <div className="relative min-h-0 flex-1 md:aspect-[4/3] md:flex-none">
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
                <p className="mt-4 rounded-full bg-black/55 px-3 py-1 text-xs font-medium backdrop-blur">
                  {busy ? <span className="inline-flex items-center gap-1.5"><Loader2 className="h-3.5 w-3.5 animate-spin" />{t("Checking…")}</span>
                    : frameState === "ok" ? <span className="inline-flex items-center gap-1.5 text-emerald-300"><Check className="h-3.5 w-3.5" />{t("Barcode detected")}</span>
                    : frameState === "error" ? t("Scanning paused")
                    : t("Position the barcode inside the frame")}
                </p>
              </div>
            )}
            {camera.state === "starting" && (
              <div className="absolute inset-0 flex items-center justify-center text-sm text-white/80"><Loader2 className="me-2 h-4 w-4 animate-spin" />{t("Starting camera…")}</div>
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
          <div className="relative z-20 space-y-2.5 bg-black px-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] pt-3">
            {outcome && (
              <div
                role="status"
                className={cn(
                  "rounded-xl px-3 py-2.5 text-sm animate-fade-up",
                  outcome.tone === "success" ? "bg-emerald-500/15 text-emerald-100" : outcome.tone === "error" ? "bg-red-500/15 text-red-100" : "bg-white/10 text-white",
                )}
              >
                <p className="flex items-start gap-2 font-semibold">
                  {outcome.tone === "success" ? <Check className="mt-0.5 h-4 w-4 shrink-0 text-emerald-300" /> : outcome.tone === "error" ? <X className="mt-0.5 h-4 w-4 shrink-0 text-red-300" /> : null}
                  <span className="min-w-0">{t(outcome.title)}</span>
                </p>
                <p className="num mt-0.5 ps-6 text-xs opacity-70">{outcome.code}</p>
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
                <Keyboard className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-white/50" />
                <Input
                  value={manual}
                  onChange={(e) => setManual(e.target.value)}
                  autoFocus={desktop}
                  placeholder={t("Type or use a handheld scanner")}
                  aria-label={t("Barcode")}
                  className="num border-white/15 bg-white/10 ps-9 text-white placeholder:text-white/45 focus-visible:ring-offset-0"
                />
              </div>
              <Button type="submit" disabled={!manual.trim()}>{t("Add")}</Button>
            </form>
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
