/** Roadmap 137: how a barcode reached S'Shop — a keyboard-style handheld scanner, the camera, or typing.
 *
 * Handheld USB / Bluetooth scanners act like keyboards: they "type" the whole code in a few milliseconds and press Enter.
 * People type far slower. That timing is the only signal a web page has, so the result is a strong hint, not proof —
 * S'Shop records it and enforces "scan only" as far as the till can tell; camera scanning is the stronger option. */
import { useRef } from "react";
import { useSession } from "@/lib/session";

export type ScanSource = "scanner" | "camera" | "manual";

/** Fastest a person types per character is ~80 ms; scanners send a character every few milliseconds. */
const SCANNER_MS_PER_CHAR = 35;

/** Times the keys typed into a field; `source(code)` classifies the code just submitted and starts over. */
export function useKeyTiming() {
  const keys = useRef<number[]>([]);
  const pasted = useRef(false);
  return {
    onKeyDown(e: React.KeyboardEvent) {
      if (e.key.length === 1) keys.current.push(performance.now());
    },
    onPaste() {
      pasted.current = true;
    },
    source(code: string): ScanSource {
      const t = keys.current;
      const n = t.length;
      const perChar = n > 1 ? (t[n - 1] - t[0]) / (n - 1) : Infinity;
      const scanner = !pasted.current && code.length >= 3 && n >= code.length - 1 && perChar <= SCANNER_MS_PER_CHAR;
      keys.current = [];
      pasted.current = false;
      return scanner ? "scanner" : "manual";
    },
    reset() {
      keys.current = [];
      pasted.current = false;
    },
  };
}

/** May barcodes be typed here? Till: the sales setting (default off) and the person's permission. Stock: the stock setting. */
export function useManualBarcodes(context: "sale" | "stock"): boolean {
  const { profile, can } = useSession();
  if (context === "sale") return !!profile?.settings.sales.manual_barcode_entry && can("barcodes.manual_sale");
  return profile?.settings.stock.manual_barcode_entry ?? true;
}

export const TYPING_OFF = "Typing barcodes is switched off here — scan the label with the scanner or camera";
