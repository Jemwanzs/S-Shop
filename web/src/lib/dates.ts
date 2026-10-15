// Roadmap 115: calendar days (yyyy-mm-dd) in the business's time zone.
/** Today (yyyy-mm-dd) in the business's time zone. */
export const todayIn = (timeZone?: string) => new Intl.DateTimeFormat("en-CA", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date());

/** "14 Oct 2026" for a yyyy-mm-dd day. */
export const dayLabel = (iso: string) =>
  new Intl.DateTimeFormat("en-GB", { day: "2-digit", month: "short", year: "numeric", timeZone: "UTC" }).format(new Date(`${iso}T00:00:00Z`));

export const EARLIEST_SALE_DATE = "2000-01-01";
