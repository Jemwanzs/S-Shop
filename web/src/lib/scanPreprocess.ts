/**
 * Roadmap 177: other ways of reading one camera frame, for barcodes the plain picture does not show clearly.
 *
 * Decoders read brightness (luma). Gold or bronze bars on light packaging are nearly as bright as the background in
 * luma, but strongly darker in the **blue channel** (yellow absorbs blue); silver and low-contrast prints gain from a
 * **contrast stretch**; light bars on a dark box (gold on black, white on black) need the picture **inverted**. Each
 * variant is a grey picture the same size as the frame. Variants are tried one per frame, alternating with the plain
 * picture, so normal barcodes are read as fast as before.
 *
 * Pure functions on pixel arrays (no DOM), so they are measured by the same code in tests.
 */

export type Variant = "luma" | "blue" | "luma_inverted" | "blue_inverted" | "stretched";

/** The order tried, one per frame: the plain picture every other frame, the enhanced ones in between — inverted first:
 * light bars on a dark box (gold on black) are the case the plain picture never reads with ZXing (iPhones). */
export const CAMERA_CYCLE: Variant[] = ["luma", "luma_inverted", "luma", "blue", "luma", "stretched", "luma", "blue_inverted"];
/** A still photo: every variant once. */
export const PHOTO_VARIANTS: Variant[] = ["luma", "blue", "stretched", "luma_inverted", "blue_inverted"];

/** Barcode formats with a check digit (or error correction): one read is trusted. Others need the same read twice when it
 * came from an enhanced picture, so an enhancement never turns noise into a wrong code. */
const CHECKED = new Set(["ean_13", "ean_8", "upc_a", "upc_e", "code_128", "code_93", "qr_code", "EAN_13", "EAN_8", "UPC_A", "UPC_E", "CODE_128", "CODE_93", "QR_CODE"]);
export function needsConfirmation(format: string | undefined, variant: Variant): boolean {
  return variant !== "luma" && !CHECKED.has(format ?? "");
}

/** Grey levels (0–255, one byte per pixel) of an RGBA picture, for one variant. */
export function toGrey(rgba: Uint8ClampedArray, width: number, height: number, variant: Variant): Uint8ClampedArray<ArrayBuffer> {
  const n = width * height;
  const out = new Uint8ClampedArray(n);
  const blue = variant === "blue" || variant === "blue_inverted";
  for (let i = 0, j = 0; i < n; i++, j += 4) {
    // Luma (Rec. 601, integer); the blue channel alone for gold and bronze.
    out[i] = blue ? rgba[j + 2] : (rgba[j] * 77 + rgba[j + 1] * 150 + rgba[j + 2] * 29) >> 8;
  }
  if (variant !== "luma") stretch(out);
  if (variant === "luma_inverted" || variant === "blue_inverted") {
    for (let i = 0; i < n; i++) out[i] = 255 - out[i];
  }
  return out;
}

/** Spreads the picture's 2nd–98th percentile over 0–255 (in place): faint bars become dark, glare saturates instead of
 * washing the bars out. A flat picture is left as it is. */
export function stretch(g: Uint8ClampedArray): void {
  const hist = new Uint32Array(256);
  for (let i = 0; i < g.length; i++) hist[g[i]]++;
  const cut = Math.max(1, Math.floor(g.length * 0.02));
  let lo = 0, hi = 255, acc = 0;
  for (; lo < 255; lo++) { acc += hist[lo]; if (acc >= cut) break; }
  acc = 0;
  for (; hi > 0; hi--) { acc += hist[hi]; if (acc >= cut) break; }
  if (hi - lo < 8) return;
  const k = 255 / (hi - lo);
  const lut = new Uint8ClampedArray(256);
  for (let v = 0; v < 256; v++) lut[v] = (v - lo) * k;
  for (let i = 0; i < g.length; i++) g[i] = lut[g[i]];
}

/** A grey picture as RGBA (what the browser's BarcodeDetector takes). */
export function greyToRgba(g: Uint8ClampedArray): Uint8ClampedArray<ArrayBuffer> {
  const out = new Uint8ClampedArray(g.length * 4);
  for (let i = 0, j = 0; i < g.length; i++, j += 4) {
    out[j] = out[j + 1] = out[j + 2] = g[i];
    out[j + 3] = 255;
  }
  return out;
}

/** How well lit the picture is (mean luma 0–255) and how much of it is blown out by glare (share of pixels ≥ 250) —
 * for the scanner's guidance. Samples every 4th pixel. */
export function lighting(rgba: Uint8ClampedArray): { mean: number; glare: number } {
  let sum = 0, bright = 0, count = 0;
  for (let j = 0; j < rgba.length; j += 16) {
    const y = (rgba[j] * 77 + rgba[j + 1] * 150 + rgba[j + 2] * 29) >> 8;
    sum += y;
    if (y >= 250) bright++;
    count++;
  }
  return { mean: count ? sum / count : 0, glare: count ? bright / count : 0 };
}

/** Requires an enhanced read to be seen twice (same code, within `windowMs`) before it counts. */
export class Confirmations {
  private seen = new Map<string, number>();
  constructor(private windowMs = 1500) {}
  /** True when this read may be used now. */
  accept(code: string, format: string | undefined, variant: Variant, now = Date.now()): boolean {
    if (!needsConfirmation(format, variant)) return true;
    const before = this.seen.get(code);
    this.seen.set(code, now);
    for (const [c, at] of this.seen) if (now - at > this.windowMs) this.seen.delete(c);
    return before !== undefined && now - before <= this.windowMs;
  }
}
