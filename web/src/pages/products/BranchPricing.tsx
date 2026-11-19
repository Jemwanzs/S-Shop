/** Roadmap 176: a product's selling price at each branch, on the product form.
 *
 * The product keeps one record and one default price; a branch may have its own price. The person edits the price of
 * their Current Branch first (changes apply only there), the default price (every branch without its own), and — for
 * other branches they work at — each branch's own price. Nothing here changes another branch's price by accident. */
import { money } from "@/lib/format";
import { t } from "@/lib/i18n";
import type { BranchPrice } from "@/lib/types";
import { Field } from "@/components/Form";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Pill } from "@/components/Badges";

export type BranchEdits = Record<string, { own: boolean; price: string }>;

const str = (v: unknown) => (v == null ? "" : String(Number(v)));

/** The form's starting state for each branch's price. */
export function initialEdits(prices: BranchPrice[]): BranchEdits {
  return Object.fromEntries(prices.map((p) => [p.branch_id, { own: !p.uses_default, price: str(p.own_price) }]));
}

/** The changes to send (`PUT /products/{id}/branch-prices`), each with the own price the person saw. */
export function branchChanges(prices: BranchPrice[], edits: BranchEdits) {
  return prices.flatMap((p) => {
    const e = edits[p.branch_id];
    if (!e) return [];
    const before = p.own_price == null ? null : Number(p.own_price);
    const after = e.own && e.price.trim() !== "" ? Number(e.price) : null;
    return before === after ? [] : [{ branch_id: p.branch_id, price: after, expected: before }];
  });
}

export function BranchPricing({ prices, edits, onChange, defaultPrice, onDefaultChange, disabled }: {
  prices: BranchPrice[];
  edits: BranchEdits;
  onChange: (e: BranchEdits) => void;
  defaultPrice: string;
  onDefaultChange: (v: string) => void;
  disabled: boolean;
}) {
  const current = prices.find((p) => p.is_current) ?? prices[0];
  const others = prices.filter((p) => p !== current && p.sells_here);
  const set = (id: string, v: { own: boolean; price: string }) => onChange({ ...edits, [id]: v });
  const shown = (id: string) => (edits[id]?.own ? edits[id].price : defaultPrice);
  const clean = (v: string) => v.replace(/[^\d.]/g, "");
  return (
    <div className="space-y-4">
      {current && (
        <div className="rounded-xl border border-primary/30 bg-primary/5 p-3" data-testid="branch-price-current">
          <Field label={`${t("Marked selling price")} — ${current.branch_name}`}
            hint={`${t("Changes to this price apply only to")} ${current.branch_name}. ${t("Other branches retain their existing prices.")}`}>
            <Input inputMode="decimal" className="num bg-background" disabled={disabled} value={shown(current.branch_id)} placeholder={t("Not set")}
              onChange={(e) => set(current.branch_id, { own: true, price: clean(e.target.value) })} />
          </Field>
          <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            {edits[current.branch_id]?.own
              ? <><Pill tone="info">{t("Branch price")}</Pill><span>{t("Default")}: <span className="num">{defaultPrice === "" ? t("Not set") : money(defaultPrice)}</span></span>
                  {!disabled && <Button type="button" variant="link" size="sm" className="h-auto p-0 text-xs" onClick={() => set(current.branch_id, { own: false, price: "" })}>{t("Use the default price")}</Button>}</>
              : <><Pill>{t("Default price")}</Pill><span>{t("Type a price above to give")} {current.branch_name} {t("its own price.")}</span></>}
          </div>
        </div>
      )}
      <Field label="Default selling price" hint="Used by every branch without its own price. Leave blank if not set yet — shown as Price on Request; 0 means free">
        <Input inputMode="decimal" className="num" disabled={disabled} value={defaultPrice} placeholder={t("Not set")} onChange={(e) => onDefaultChange(clean(e.target.value))} />
      </Field>
      {others.length > 0 && (
        <div>
          <div className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{t("Other branches")}</div>
          <ul className="divide-y rounded-xl border">
            {others.map((p) => {
              const e = edits[p.branch_id] ?? { own: false, price: "" };
              return (
                <li key={p.branch_id} className="flex flex-wrap items-center gap-2 p-2.5 text-sm" data-testid="branch-price-row">
                  <span className="min-w-0 flex-1 truncate font-medium">{p.branch_name}</span>
                  {e.own ? (
                    <>
                      <Input inputMode="decimal" className="num h-9 w-32" disabled={disabled} aria-label={`${t("Price at")} ${p.branch_name}`} value={e.price}
                        onChange={(ev) => set(p.branch_id, { own: true, price: clean(ev.target.value) })} />
                      {!disabled && <Button type="button" variant="ghost" size="sm" onClick={() => set(p.branch_id, { own: false, price: "" })}>{t("Use default")}</Button>}
                    </>
                  ) : (
                    <>
                      <span className="num text-muted-foreground">{defaultPrice === "" ? t("Not set") : money(defaultPrice)} · {t("default")}</span>
                      {!disabled && <Button type="button" variant="ghost" size="sm" onClick={() => set(p.branch_id, { own: true, price: defaultPrice })}>{t("Set own price")}</Button>}
                    </>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
      {disabled && <p className="text-xs text-muted-foreground">{t("You cannot change selling prices.")}</p>}
    </div>
  );
}
