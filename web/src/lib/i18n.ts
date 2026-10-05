import type { ReactNode } from "react";
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

/** Translate English UI text; untranslated text falls back to English. */
export function t(text: string): string {
  if (current === "en") return text;
  return DICT[text]?.[COLUMN[current]] ?? text;
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
