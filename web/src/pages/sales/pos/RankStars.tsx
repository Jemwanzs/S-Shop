/**
 * Roadmap 110 — Gold / Silver / Bronze sales-performance stars (not customer reviews): rank 1 ★★★★★ gold,
 * rank 2 ★★★ silver, rank 3 ★★ bronze. Hover (or the item sheet on a tap) explains the ranking.
 */
import { Star } from "lucide-react";
import { count, money } from "@/lib/format";
import { t } from "@/lib/i18n";
import type { PosProduct, Settings } from "@/lib/types";

const TIERS: Record<number, { stars: number; color: string; name: string }> = {
  1: { stars: 5, color: "#C9A227", name: "Gold" },
  2: { stars: 3, color: "#9CA3AF", name: "Silver" },
  3: { stars: 2, color: "#B87333", name: "Bronze" },
};
const PERIOD: Record<string, string> = { today: "today", week: "this week", month: "this month" };

/** "#1 top seller this week · 42 units sold" */
export function rankExplanation(p: PosProduct, ts: Settings["sales"]["top_selling"]): string | null {
  if (!p.rank || p.rank > 3) return null;
  const score = Number(p.rank_score ?? 0);
  const measure = ts.metric === "value" ? `${money(score)} ${t("sold")}` : ts.metric === "sales" ? `${count(score)} ${t("sales")}` : `${count(score)} ${t("units sold")}`;
  return `${t(TIERS[p.rank].name)} · #${p.rank} ${t("top seller")} ${t(PERIOD[ts.period] ?? "this week")} · ${measure}`;
}

export function RankStars({ product, settings }: { product: PosProduct; settings: Settings["sales"]["top_selling"] }) {
  const tier = product.rank ? TIERS[product.rank] : undefined;
  const why = rankExplanation(product, settings);
  if (!tier || !why) return null;
  return (
    <span className="inline-flex shrink-0 items-center gap-px" title={why} aria-label={why} role="img">
      {Array.from({ length: tier.stars }, (_, i) => (
        <Star key={i} className="h-2.5 w-2.5" style={{ color: tier.color, fill: tier.color }} aria-hidden />
      ))}
    </span>
  );
}
