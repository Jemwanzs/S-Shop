import { useEffect, useRef, useState } from "react";
import { Keyboard, ScanLine } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "./ResponsiveDialog";

interface DetectorLike {
  detect(source: HTMLVideoElement): Promise<{ rawValue: string }[]>;
}
declare global {
  interface Window {
    BarcodeDetector?: new (opts?: { formats?: string[] }) => DetectorLike;
  }
}

function feedback() {
  try {
    navigator.vibrate?.(60);
    const ctx = new AudioContext();
    const osc = ctx.createOscillator();
    osc.frequency.value = 1200;
    osc.connect(ctx.destination);
    osc.start();
    osc.stop(ctx.currentTime + 0.08);
  } catch {
    /* feedback is best-effort */
  }
}

/**
 * Camera scanner: native BarcodeDetector where available, ZXing otherwise.
 * Handheld scanners (keyboard wedge) work through the manual field + Enter.
 * `continuous` keeps scanning (e.g. receiving several tracked items).
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
  onDetected: (code: string) => void;
  title?: string;
  continuous?: boolean;
  hint?: string;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [manual, setManual] = useState("");
  const [last, setLast] = useState<string | null>(null);
  const cb = useRef(onDetected);
  cb.current = onDetected;

  useEffect(() => {
    if (!open) return;
    let stopped = false;
    let stream: MediaStream | null = null;
    let zxingControls: { stop: () => void } | null = null;
    let lastCode = "";
    let lastAt = 0;
    setError(null);

    const handle = (code: string) => {
      const now = Date.now();
      if (code === lastCode && now - lastAt < 2500) return; // ignore repeated frames of the same code
      lastCode = code;
      lastAt = now;
      feedback();
      setLast(code);
      cb.current(code);
      if (!continuous) onOpenChange(false);
    };

    (async () => {
      try {
        const video = videoRef.current;
        if (!video) return;
        if (window.BarcodeDetector) {
          stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" } });
          video.srcObject = stream;
          await video.play();
          const detector = new window.BarcodeDetector({
            formats: ["ean_13", "ean_8", "upc_a", "upc_e", "code_128", "code_39", "code_93", "itf", "qr_code", "data_matrix"],
          });
          const tick = async () => {
            if (stopped) return;
            try {
              const found = await detector.detect(video);
              if (found[0]?.rawValue) handle(found[0].rawValue);
            } catch {
              /* frame not ready */
            }
            setTimeout(tick, 180);
          };
          tick();
        } else {
          const { BrowserMultiFormatReader } = await import("@zxing/browser");
          const reader = new BrowserMultiFormatReader();
          zxingControls = await reader.decodeFromConstraints({ video: { facingMode: "environment" } }, video, (result) => {
            if (result && !stopped) handle(result.getText());
          });
        }
      } catch {
        setError("Camera unavailable. Allow camera access, or type / use a handheld scanner below.");
      }
    })();

    return () => {
      stopped = true;
      zxingControls?.stop();
      stream?.getTracks().forEach((t) => t.stop());
    };
  }, [open, continuous, onOpenChange]);

  const submitManual = () => {
    const code = manual.trim();
    if (!code) return;
    setManual("");
    setLast(code);
    onDetected(code);
    if (!continuous) onOpenChange(false);
  };

  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} title={title} description={hint}>
      <div className="space-y-4">
        <div className="relative aspect-[4/3] overflow-hidden rounded-xl bg-black">
          <video ref={videoRef} className="h-full w-full object-cover" playsInline muted />
          <div className="pointer-events-none absolute inset-x-8 top-1/2 h-24 -translate-y-1/2 rounded-lg border-2 border-white/80 shadow-[0_0_0_9999px_rgba(0,0,0,0.35)]">
            <ScanLine className="absolute left-1/2 top-1/2 h-8 w-8 -translate-x-1/2 -translate-y-1/2 animate-pulse text-white/80" />
          </div>
          {error && <p className="absolute inset-x-0 bottom-0 bg-black/70 p-3 text-center text-sm text-white">{error}</p>}
        </div>
        {last && (
          <p className="text-center text-sm">
            Last scanned: <span className="num font-semibold">{last}</span>
          </p>
        )}
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            submitManual();
          }}
        >
          <div className="relative flex-1">
            <Keyboard className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input value={manual} onChange={(e) => setManual(e.target.value)} placeholder="Type or use a handheld scanner" className="ps-9" />
          </div>
          <Button type="submit" variant="ink">
            Add
          </Button>
        </form>
        {continuous && (
          <Button variant="outline" className="w-full" onClick={() => onOpenChange(false)}>
            Done scanning
          </Button>
        )}
      </div>
    </ResponsiveDialog>
  );
}
