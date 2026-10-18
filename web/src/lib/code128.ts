/** Code 128 (roadmap 134): the barcode on S'Shop stickers. Digits travel in subset C (two per symbol, the shortest
 * bars), anything else in subset B; a modulo-103 check symbol and the stop pattern close it. Verified by decoding the
 * printed output with the app's own scanner engine (tests). */

/** Bar/space widths (in modules) of symbols 0–105, then the stop pattern. Each symbol starts with a bar. */
const PATTERNS = [
  "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213",
  "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132",
  "221231", "213212", "223112", "312131", "311222", "321122", "321221", "312212", "322112", "322211",
  "212123", "212321", "232121", "111323", "131123", "131321", "112313", "132113", "132311", "211313",
  "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121", "313121", "211331",
  "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
  "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214",
  "112412", "122114", "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111",
  "111242", "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
  "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311", "113141",
  "114131", "311141", "411131", "211412", "211214", "211232",
];
const STOP = "2331112";
const START_B = 104;
const START_C = 105;
const CODE_B = 100;
const CODE_C = 99;

/** Symbol values for `text` (start, data, check — without the stop). Printable ASCII only. */
export function symbols(text: string): number[] {
  if (!text || ![...text].every((c) => c.charCodeAt(0) >= 32 && c.charCodeAt(0) <= 126)) {
    throw new Error("Code 128 labels take printable characters only");
  }
  const out: number[] = [];
  const digitRun = (i: number) => {
    let j = i;
    while (j < text.length && text[j] >= "0" && text[j] <= "9") j++;
    return j - i;
  };
  let i = 0;
  let set: "B" | "C";
  // Start in C when the text opens with an even run of 4+ digits (or is all digits, even length).
  const lead = digitRun(0);
  if (lead >= 4 && lead % 2 === 0) {
    set = "C";
    out.push(START_C);
  } else {
    set = "B";
    out.push(START_B);
  }
  while (i < text.length) {
    const run = digitRun(i);
    if (set === "B" && run >= 4) {
      // Odd run: one digit in B first, the rest in C.
      if (run % 2 === 1) {
        out.push(text.charCodeAt(i) - 32);
        i++;
      }
      out.push(CODE_C);
      set = "C";
      continue;
    }
    if (set === "C") {
      if (run >= 2) {
        out.push(Number(text.slice(i, i + 2)));
        i += 2;
        continue;
      }
      out.push(CODE_B);
      set = "B";
      continue;
    }
    out.push(text.charCodeAt(i) - 32);
    i++;
  }
  const check = out.reduce((sum, v, k) => sum + v * (k === 0 ? 1 : k), 0) % 103;
  out.push(check);
  return out;
}

/** Alternating bar/space widths in modules (bar first), stop included; no quiet zones. */
export function modules(text: string): number[] {
  const widths: number[] = [];
  for (const s of symbols(text)) for (const d of PATTERNS[s]) widths.push(Number(d));
  for (const d of STOP) widths.push(Number(d));
  return widths;
}

/** Quiet zone each side, in modules (the standard minimum is 10). */
export const QUIET = 10;

/** Bars as [x, width] in modules from the left edge of the symbol (quiet zone not included). */
export function bars(text: string): { x: number; w: number }[] {
  const out: { x: number; w: number }[] = [];
  let x = 0;
  modules(text).forEach((w, k) => {
    if (k % 2 === 0) out.push({ x, w });
    x += w;
  });
  return out;
}

/** Total width of the symbol in modules (quiet zones not included). */
export const symbolWidth = (text: string) => modules(text).reduce((a, b) => a + b, 0);
