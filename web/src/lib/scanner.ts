/**
 * Camera barcode engine shared by every scanning flow (sales, stock, transfers, counts, returns, orders).
 * Uses the browser's native BarcodeDetector when it supports retail 1D formats, ZXing otherwise (iOS Safari,
 * Firefox, desktop Chrome on Windows). Both decode continuously from one MediaStream that we own, so torch and
 * camera switching work the same way on either path.
 *
 * Roadmap 177: every other frame is read as an enhanced picture (blue channel, contrast stretch, inverted — see
 * scanPreprocess) so gold, bronze, silver, low-contrast and light-on-dark barcodes are read too; a read from an enhanced
 * picture of a format without a check digit counts only when seen twice. Zoom is offered where the camera has it, and
 * the lighting of the picture is reported for the scanner's guidance.
 */
import { CAMERA_CYCLE, Confirmations, PHOTO_VARIANTS, greyToRgba, lighting, toGrey, type Variant } from "@/lib/scanPreprocess";

/** Retail formats we decode: product packaging (EAN/UPC), shelf and item labels (Code 128/39, ITF) and QR. */
const NATIVE_FORMATS = ["ean_13", "ean_8", "upc_a", "upc_e", "code_128", "code_39", "code_93", "itf", "qr_code"];

interface NativeDetector {
  detect(source: HTMLVideoElement | ImageData | ImageBitmap | HTMLCanvasElement): Promise<{ rawValue: string; format?: string }[]>;
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
  /** Roadmap 177: the camera's zoom range when it has one (phones with several lenses or digital zoom). */
  zoom: { min: number; max: number } | null;
  setZoom: (z: number) => Promise<void>;
}

/** Roadmap 177: how the picture looks, about once a second (for guidance: too dark, glare). */
export type CameraStats = { mean: number; glare: number };

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
export async function startScanning(video: HTMLVideoElement, onCode: (code: string) => void, deviceId?: string, onStats?: (s: CameraStats) => void): Promise<CameraSession> {
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
  // Continuous focus and exposure where the camera offers them (each is optional; iPhones manage their own).
  for (const c of [{ focusMode: "continuous" }, { exposureMode: "continuous" }]) {
    try {
      await track.applyConstraints({ advanced: [c as MediaTrackConstraintSet] });
    } catch {
      /* not offered by this camera */
    }
  }
  video.srcObject = stream;
  video.setAttribute("playsinline", "true");
  video.muted = true;
  await video.play().catch(() => undefined);

  let stopped = false;
  let stopDecoder = () => {};
  const native = await nativeDetector();
  let engine: CameraSession["engine"] = "native";
  // Roadmap 177: the frame's central band (where the on-screen frame is) as pixels, for enhanced reads and lighting.
  const band = document.createElement("canvas");
  const bg = band.getContext("2d", { willReadFrequently: true });
  const grab = (): ImageData | null => {
    const w = video.videoWidth, h = video.videoHeight;
    if (!bg || !w || !h) return null;
    const sw = Math.round(w * 0.9), sh = Math.round(h * 0.6);
    const scale = Math.min(1, 1280 / sw);
    band.width = Math.round(sw * scale);
    band.height = Math.round(sh * scale);
    bg.drawImage(video, Math.round((w - sw) / 2), Math.round((h - sh) / 2), sw, sh, 0, 0, band.width, band.height);
    return bg.getImageData(0, 0, band.width, band.height);
  };
  const enhanced = (img: ImageData, v: Variant) => new ImageData(greyToRgba(toGrey(img.data, img.width, img.height, v)), img.width, img.height);
  const confirm = new Confirmations();
  let frame = 0;
  let statsAt = 0;
  const report = (img: ImageData | null) => {
    const now = performance.now();
    if (img && onStats && now - statsAt > 1000) {
      statsAt = now;
      onStats(lighting(img.data));
    }
  };
  if (native) {
    let timer = 0;
    const tick = async () => {
      if (stopped) return;
      try {
        if (video.readyState >= 2) {
          const v = CAMERA_CYCLE[frame++ % CAMERA_CYCLE.length];
          let found: { rawValue: string; format?: string }[] = [];
          if (v === "luma") {
            found = await native.detect(video);
            if (onStats && performance.now() - statsAt > 1000) report(grab());
          } else {
            const img = grab();
            report(img);
            if (img) found = await native.detect(enhanced(img, v));
          }
          const hit = found.find((f) => f.rawValue);
          if (hit && !stopped && confirm.accept(hit.rawValue, hit.format, v)) onCode(hit.rawValue);
        }
      } catch {
        /* frame not ready */
      }
      timer = window.setTimeout(tick, 110);
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
    // The enhanced picture goes through a canvas too, so ZXing can still try it turned 90°.
    const work = document.createElement("canvas");
    const wg = work.getContext("2d", { willReadFrequently: true });
    let timer = 0;
    const tick = () => {
      if (stopped) return;
      // Central 90% × 60% of the picture, scaled down to at most 1280 px wide.
      const img = grab();
      if (img && wg) {
        const v = CAMERA_CYCLE[frame++ % CAMERA_CYCLE.length];
        report(img);
        try {
          let source = band;
          if (v !== "luma") {
            work.width = img.width;
            work.height = img.height;
            wg.putImageData(enhanced(img, v), 0, 0);
            source = work;
          }
          const r = reader.decodeFromCanvas(source);
          const code = r.getText();
          if (code && !stopped && confirm.accept(code, BarcodeFormat[r.getBarcodeFormat()], v)) onCode(code);
        } catch {
          /* no barcode in this frame */
        }
      }
      timer = window.setTimeout(tick, 110);
    };
    tick();
    stopDecoder = () => window.clearTimeout(timer);
  }

  const caps = (track.getCapabilities?.() ?? {}) as MediaTrackCapabilities & { torch?: boolean; zoom?: { min: number; max: number } };
  const cameras = (await navigator.mediaDevices.enumerateDevices().catch(() => [])).filter((d) => d.kind === "videoinput");
  const zoom = caps.zoom && caps.zoom.max > caps.zoom.min ? { min: caps.zoom.min, max: caps.zoom.max } : null;

  return {
    engine,
    cameras,
    deviceId: track.getSettings().deviceId,
    torchSupported: !!caps.torch,
    setTorch: async (on) => {
      await track.applyConstraints({ advanced: [{ torch: on } as MediaTrackConstraintSet] });
    },
    zoom,
    setZoom: async (z) => {
      if (!zoom) return;
      await track.applyConstraints({ advanced: [{ zoom: Math.min(zoom.max, Math.max(zoom.min, z)) } as MediaTrackConstraintSet] });
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

/**
 * Roadmap 177: reads a barcode from a photo (the phone's own camera app focuses and exposes a still picture better than a
 * live preview — useful for shiny or curved labels). Every enhancement is tried; a format without a check digit is
 * accepted only when two of them read the same code. Never guesses: no read is `null`.
 */
export async function decodePhoto(file: Blob): Promise<string | null> {
  const bitmap = await loadImage(file);
  const max = 1600;
  const scale = Math.min(1, max / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(bitmap.width * scale));
  canvas.height = Math.max(1, Math.round(bitmap.height * scale));
  const g = canvas.getContext("2d", { willReadFrequently: true });
  if (!g) return null;
  g.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  const img = g.getImageData(0, 0, canvas.width, canvas.height);
  const native = await nativeDetector();
  const reads: { code: string; format: string | undefined; variant: Variant }[] = [];
  const accept = (code: string, format: string | undefined, variant: Variant) => {
    reads.push({ code, format, variant });
    const conf = new Confirmations(Number.MAX_SAFE_INTEGER);
    return reads.filter((r) => r.code === code).some((r) => conf.accept(r.code, r.format, r.variant));
  };
  const [{ BrowserMultiFormatReader }, { BarcodeFormat, DecodeHintType }] = await Promise.all([import("@zxing/browser"), import("@zxing/library")]);
  const hints = new Map();
  hints.set(DecodeHintType.POSSIBLE_FORMATS, [
    BarcodeFormat.EAN_13, BarcodeFormat.EAN_8, BarcodeFormat.UPC_A, BarcodeFormat.UPC_E,
    BarcodeFormat.CODE_128, BarcodeFormat.CODE_39, BarcodeFormat.CODE_93, BarcodeFormat.ITF, BarcodeFormat.QR_CODE,
  ]);
  hints.set(DecodeHintType.TRY_HARDER, true);
  const reader = new BrowserMultiFormatReader(hints);
  const work = document.createElement("canvas");
  work.width = img.width;
  work.height = img.height;
  const wg = work.getContext("2d", { willReadFrequently: true });
  for (const v of PHOTO_VARIANTS) {
    const pic = v === "luma" ? img : new ImageData(greyToRgba(toGrey(img.data, img.width, img.height, v)), img.width, img.height);
    if (native) {
      try {
        const hit = (await native.detect(pic)).find((f) => f.rawValue);
        if (hit && accept(hit.rawValue, hit.format, v)) return hit.rawValue;
      } catch {
        /* try ZXing */
      }
    }
    if (wg) {
      wg.putImageData(pic, 0, 0);
      try {
        const r = reader.decodeFromCanvas(work);
        if (accept(r.getText(), BarcodeFormat[r.getBarcodeFormat()], v)) return r.getText();
      } catch {
        /* next enhancement */
      }
    }
  }
  return null;
}

async function loadImage(file: Blob): Promise<ImageBitmap | HTMLImageElement> {
  if (typeof createImageBitmap === "function") {
    try {
      return await createImageBitmap(file, { imageOrientation: "from-image" } as ImageBitmapOptions);
    } catch {
      /* older Safari: an <img> below */
    }
  }
  const url = URL.createObjectURL(file);
  try {
    const el = new Image();
    el.src = url;
    await el.decode();
    return el;
  } finally {
    URL.revokeObjectURL(url);
  }
}

export type ScanSound = "ok" | "error" | "warning";

// One audio context for the app (browsers limit how many may exist). Phones only let it make sound after a tap or key
// press, and a camera read is neither — so the first touch of the page unlocks it.
let audio: AudioContext | null = null;
function audioContext() {
  audio ??= new AudioContext();
  if (audio.state === "suspended") void audio.resume();
  return audio;
}
if (typeof document !== "undefined") {
  const unlock = () => {
    try {
      audioContext();
    } catch {
      /* no sound on this device */
    }
    document.removeEventListener("pointerdown", unlock, true);
    document.removeEventListener("keydown", unlock, true);
  };
  document.addEventListener("pointerdown", unlock, true);
  document.addEventListener("keydown", unlock, true);
}

/** [frequency Hz, start s, length s] of each tone. */
const SOUNDS: Record<ScanSound, { wave: OscillatorType; tones: [number, number, number][]; buzz: number | number[] }> = {
  ok: { wave: "sine", tones: [[1900, 0, 0.09]], buzz: 60 }, // the supermarket beep
  warning: { wave: "triangle", tones: [[880, 0, 0.07], [880, 0.12, 0.07]], buzz: [30, 40, 30] }, // already scanned
  error: { wave: "square", tones: [[220, 0, 0.24]], buzz: [40, 60, 40] }, // not accepted
};

/** The sound and vibration of one finished scan (best effort; respects devices without either). Call once per scan. */
export function scanFeedback(sound: ScanSound = "ok") {
  try {
    const s = SOUNDS[sound];
    navigator.vibrate?.(s.buzz);
    const ctx = audioContext();
    for (const [frequency, at, length] of s.tones) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = s.wave;
      osc.frequency.value = frequency;
      gain.gain.value = sound === "error" ? 0.12 : 0.25;
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(ctx.currentTime + at);
      osc.stop(ctx.currentTime + at + length);
    }
  } catch {
    /* feedback is optional */
  }
}

/** The sound of a scan's result. */
export function outcomeSound(tone?: "success" | "error" | "warning" | "info"): ScanSound {
  return tone === "error" ? "error" : tone === "warning" ? "warning" : "ok";
}
