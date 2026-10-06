import { Children, type ReactNode } from "react";
import { DICT } from "@/i18n/dict";

/** Interface languages. Arabic switches the layout to right-to-left. */
export const LANGUAGES = [
  { code: "en", label: "English" },
  { code: "sw", label: "Kiswahili" },
  { code: "fr", label: "Français" },
  { code: "ar", label: "العربية" },
] as const;

const COLUMN: Record<string, number> = { sw: 0, fr: 1, ar: 2 };
let current = "en";

/** Applied by the session provider from the user's preferences. */
export function setLanguage(code: string) {
  current = code in COLUMN ? code : "en";
  document.documentElement.lang = current;
  document.documentElement.dir = current === "ar" ? "rtl" : "ltr";
}

/**
 * Dictionary entries containing `{}` are templates for messages with values filled in (mostly server messages such as
 * "Only {} × {} in stock"). Most specific first, so a generic template never captures a more specific message.
 */
let templates: { key: string; re: RegExp }[] | null = null;
function compiledTemplates() {
  if (!templates) {
    const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    templates = Object.keys(DICT)
      .filter((k) => k.includes("{}"))
      .sort((a, b) => b.replace(/\{\}/g, "").length - a.replace(/\{\}/g, "").length)
      .map((key) => ({ key, re: new RegExp("^" + key.split("{}").map(esc).join("(.+?)") + "$", "s") }));
  }
  return templates;
}

/** Translate English UI text; untranslated text falls back to English. */
export function t(text: string): string {
  if (current === "en" || !text) return text;
  const col = COLUMN[current];
  const hit = DICT[text]?.[col];
  if (hit) return hit;
  if (text.length > 400 || !/[A-Za-z]/.test(text)) return text;
  for (const tp of compiledTemplates()) {
    const m = tp.re.exec(text);
    if (!m) continue;
    let i = 1;
    // Values keep their text, translated when they are known words (statuses, record names…).
    return DICT[tp.key][col].replace(/\{\}/g, () => {
      const v = m[i++] ?? "";
      return DICT[v]?.[col] ?? v;
    });
  }
  return text;
}

/**
 * Translate the plain-text parts of children (e.g. a button's "<Icon /> Save"), keeping surrounding spaces.
 * Unknown text passes through unchanged, so only dictionary entries change.
 */
export function tChildren(children: ReactNode): ReactNode {
  if (current === "en") return children;
  const one = (c: ReactNode) => {
    if (typeof c !== "string") return c;
    const core = c.trim();
    if (!core) return c;
    const tr = t(core);
    return tr === core ? c : c.replace(core, tr);
  };
  return Array.isArray(children) ? Children.map(children, one) : one(children);
}

/** Translate a component prop when it is plain text (elements pass through untouched). */
export function tx<T extends ReactNode>(node: T): T | string {
  return typeof node === "string" ? t(node) : node;
}

const DEVICE_KEY = "sshop.lang";
/** Language used before sign-in (sign-in and request-access screens). */
export function deviceLanguage(): string {
  try {
    return localStorage.getItem(DEVICE_KEY) ?? "en";
  } catch {
    return "en";
  }
}
export function rememberDeviceLanguage(code: string) {
  try {
    localStorage.setItem(DEVICE_KEY, code);
  } catch {
    /* storage unavailable */
  }
}
