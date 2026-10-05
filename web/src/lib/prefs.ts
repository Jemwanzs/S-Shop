/** User preferences: font and display currency (language arrives with translations). */

export const FONTS = ["Outfit", "Poppins", "Inter", "Roboto", "Nunito"] as const;
export const CURRENCIES = [
  { code: "KES", label: "Kenyan shilling (KES)" },
  { code: "USD", label: "US dollar (USD)" },
  { code: "EUR", label: "Euro (EUR)" },
] as const;

export interface Preferences {
  language: string;
  font: string;
  currency: string;
}

export const DEFAULT_PREFERENCES: Preferences = { language: "en", font: "Outfit", currency: "KES" };

export interface Fx {
  base: string;
  rates: Record<string, number>;
  updated_at: string;
  source: string;
  stale: boolean;
}

/** Switches the app font, loading it from Google Fonts on first use (Outfit ships with the page). */
export function applyFont(font: string) {
  const name = (FONTS as readonly string[]).includes(font) ? font : "Outfit";
  if (name !== "Outfit" && !document.getElementById(`font-${name}`)) {
    const link = document.createElement("link");
    link.id = `font-${name}`;
    link.rel = "stylesheet";
    link.href = `https://fonts.googleapis.com/css2?family=${name}:wght@300;400;500;600;700&display=swap`;
    document.head.appendChild(link);
  }
  document.documentElement.style.setProperty("--app-font", `"${name}"`);
}
