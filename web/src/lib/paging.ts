/** Page numbers to show for `pages` pages around `current` (1-based), with "…" gaps — shared by the website and the
 * ordering link (roadmap 80). Always shows the first and last page. */
export function pageItems(current: number, pages: number, around = 1): (number | "…")[] {
  if (pages <= 7) return Array.from({ length: pages }, (_, i) => i + 1);
  const out: (number | "…")[] = [1];
  const from = Math.max(2, current - around);
  const to = Math.min(pages - 1, current + around);
  if (from > 2) out.push("…");
  for (let i = from; i <= to; i++) out.push(i);
  if (to < pages - 1) out.push("…");
  out.push(pages);
  return out;
}
