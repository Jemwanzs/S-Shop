/**
 * Camera barcode engine shared by every scanning flow (sales, stock, transfers, counts, returns, orders).
 * Uses the browser's native BarcodeDetector when it supports retail 1D formats, ZXing otherwise (iOS Safari,
 * Firefox, desktop Chrome on Windows). Both decode continuously from one MediaStream that we own, so torch and
 * camera switching work the same way on either path.
 */

/** Retail formats we decode: product packaging (EAN/UPC), shelf and item labels (Code 128/39, ITF) and QR. */
const NATIVE_FORMATS = ["ean_13", "ean_8", "upc_a", "upc_e", "code_128", "code_39", "code_93", "itf", "qr_code"];

interface NativeDetector {
  detect(source: HTMLVideoElement): Promise<{ rawValue: string }[]>;
}
interface NativeDetectorCtor {
  new (opts?: { formats?: string[] }): NativeDetector;
  getSupportedFormats?: () => Promise<string[]>;
}

export type CameraProblem = "insecure" | "denied" | "no-camera" | "busy" | "unsupported" | "failed";

export function cameraProblemText(p: CameraProblem): string {
  switch (p) {
    case "insecure":
      return "The camera needs a secure (https) connection.";
    case "denied":
      return "Camera access was blocked. Allow the camera for this site in your browser settings — or use a handheld scanner or type the code below.";
    case "no-camera":
      return "No camera found on this device. Use a handheld scanner or type the code below.";
    case "busy":
      return "The camera is being used by another app. Close it and try again.";
    case "unsupported":
      return "This browser cannot use the camera. Use a handheld scanner or type the code below.";
    default:
      return "The camera could not start. Try again, or use a handheld scanner or type the code below.";
  }
}

function classify(err: unknown): CameraProblem {
  const name = (err as { name?: string })?.name ?? "";
  if (name === "NotAllowedError" || name === "SecurityError") return "denied";
  if (name === "NotFoundError" || name === "OverconstrainedError" || name === "DevicesNotFoundError") return "no-camera";
  if (name === "NotReadableError" || name === "TrackStartError") return "busy";
  if (name === "NotSupportedError" || name === "TypeError") return "unsupported";
  return "failed";
}

export interface CameraSession {
  stop: () => void;
  torchSupported: boolean;
  setTorch: (on: boolean) => Promise<void>;
  /** Other cameras available to switch to (after permission was granted). */
  cameras: MediaDeviceInfo[];
  deviceId: string | undefined;
  engine: "native" | "zxing";
}

async function nativeDetector(): Promise<NativeDetector | null> {
  const Ctor = (window as unknown as { BarcodeDetector?: NativeDetectorCtor }).BarcodeDetector;
  if (!Ctor) return null;
  try {
    const supported = (await Ctor.getSupportedFormats?.()) ?? [];
    const formats = NATIVE_FORMATS.filter((f) => supported.includes(f));
    // Some platforms expose the API with QR only — product barcodes need EAN/Code 128, so fall back to ZXing.
    if (!formats.includes("ean_13") || !formats.includes("code_128")) return null;
    return new Ctor({ formats });
  } catch {
    return null;
  }
}

/**
 * Starts the camera on `video` and calls `onCode` for every decoded barcode until `stop()`.
 * Throws a CameraProblem string when the camera cannot be used.
 */
export async function startScanning(video: HTMLVideoElement, onCode: (code: string) => void, deviceId?: string): Promise<CameraSession> {
  if (!window.isSecureContext) throw "insecure" satisfies CameraProblem;
  if (!navigator.mediaDevices?.getUserMedia) throw "unsupported" satisfies CameraProblem;

  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: {
        ...(deviceId ? { deviceId: { exact: deviceId } } : { facingMode: { ideal: "environment" } }),
        // Higher resolution makes thin 1D bars decodable from a normal holding distance.
        width: { ideal: 1920 },
        height: { ideal: 1080 },
      },
    });
  } catch (e) {
    throw classify(e);
  }
  const track = stream.getVideoTracks()[0];
  try {
    await track.applyConstraints({ advanced: [{ focusMode: "continuous" } as MediaTrackConstraintSet] });
  } catch {
    /* continuous focus is optional */
  }
  video.srcObject = stream;
  video.setAttribute("playsinline", "true");
  video.muted = true;
  await video.play().catch(() => undefined);

  let stopped = false;
  let stopDecoder = () => {};
  const native = await nativeDetector();
  let engine: CameraSession["engine"] = "native";
  if (native) {
    let timer = 0;
    const tick = async () => {
      if (stopped) return;
      try {
        if (video.readyState >= 2) {
          const found = await native.detect(video);
          const code = found.find((f) => f.rawValue)?.rawValue;
          if (code && !stopped) onCode(code);
        }
      } catch {
        /* frame not ready */
      }
      timer = window.setTimeout(tick, 120);
    };
    tick();
    stopDecoder = () => window.clearTimeout(timer);
  } else {
    // ZXing on frames we grab ourselves: no second stream attachment, and only the central band (where the
    // on-screen frame is) is decoded, which is faster and ignores clutter around the product.
    engine = "zxing";
    const [{ BrowserMultiFormatReader }, { BarcodeFormat, DecodeHintType }] = await Promise.all([import("@zxing/browser"), import("@zxing/library")]);
    const hints = new Map();
    hints.set(DecodeHintType.POSSIBLE_FORMATS, [
      BarcodeFormat.EAN_13, BarcodeFormat.EAN_8, BarcodeFormat.UPC_A, BarcodeFormat.UPC_E,
      BarcodeFormat.CODE_128, BarcodeFormat.CODE_39, BarcodeFormat.CODE_93, BarcodeFormat.ITF, BarcodeFormat.QR_CODE,
    ]);
    hints.set(DecodeHintType.TRY_HARDER, true);
    const reader = new BrowserMultiFormatReader(hints);
    const canvas = document.createElement("canvas");
    const g = canvas.getContext("2d", { willReadFrequently: true });
    let timer = 0;
    const tick = () => {
      if (stopped) return;
      const w = video.videoWidth;
      const h = video.videoHeight;
      if (g && w && h) {
        // Central 90% × 60% of the picture, scaled down to at most 1280 px wide.
        const sw = Math.round(w * 0.9);
        const sh = Math.round(h * 0.6);
        const scale = Math.min(1, 1280 / sw);
        canvas.width = Math.round(sw * scale);
        canvas.height = Math.round(sh * scale);
        g.drawImage(video, Math.round((w - sw) / 2), Math.round((h - sh) / 2), sw, sh, 0, 0, canvas.width, canvas.height);
        try {
          const code = reader.decodeFromCanvas(canvas).getText();
          if (code && !stopped) onCode(code);
        } catch {
          /* no barcode in this frame */
        }
      }
      timer = window.setTimeout(tick, 110);
    };
    tick();
    stopDecoder = () => window.clearTimeout(timer);
  }

  const caps = (track.getCapabilities?.() ?? {}) as MediaTrackCapabilities & { torch?: boolean };
  const cameras = (await navigator.mediaDevices.enumerateDevices().catch(() => [])).filter((d) => d.kind === "videoinput");

  return {
    engine,
    cameras,
    deviceId: track.getSettings().deviceId,
    torchSupported: !!caps.torch,
    setTorch: async (on) => {
      await track.applyConstraints({ advanced: [{ torch: on } as MediaTrackConstraintSet] });
    },
    stop: () => {
      if (stopped) return;
      stopped = true;
      stopDecoder();
      stream.getTracks().forEach((t) => t.stop());
      video.srcObject = null;
    },
  };
}

/** Short beep + vibration on a successful read (best effort; respects devices without either). */
export function scanFeedback(ok = true) {
  try {
    navigator.vibrate?.(ok ? 60 : [40, 60, 40]);
    const ctx = new AudioContext();
    const osc = ctx.createOscillator();
    osc.frequency.value = ok ? 1200 : 300;
    osc.connect(ctx.destination);
    osc.start();
    osc.stop(ctx.currentTime + (ok ? 0.08 : 0.18));
    osc.onended = () => ctx.close();
  } catch {
    /* feedback is optional */
  }
}
