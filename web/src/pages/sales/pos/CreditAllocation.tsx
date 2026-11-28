/** Roadmap 196: Payment Allocation Summary — when several items are left (partly) on credit, the money received pays the
 * cheapest items first (selling price after discounts and points); every item still owing becomes its own credit record
 * on the one sale. The server makes the same allocation when the sale is recorded; this is the preview. */
import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { money } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { ProductName } from "@/components/ProductName";

export interface AllocLine { key: string; name: string; unitPrice: number; quantity: number }
export interface UnitShare { key: string; name: string; unit: number; units: number; price: number; paid: number; owed: number }

const round2 = (n: number) => Math.round(n * 100) / 100;

/** The same rule as the server's `allocate_units`: cheapest first, equal prices in cart order then unit order. */
export function allocate(lines: AllocLine[], payable: number, paid: number): UnitShare[] {
  const net = lines.reduce((a, l) => a + l.unitPrice * l.quantity, 0);
  const share = net > 0 ? payable / net : 1;
  const units = lines.flatMap((l, li) => Array.from({ length: Math.max(0, l.quantity) }, (_, u) => ({ li, u: u + 1, key: l.key, name: l.name, units: l.quantity, price: round2(l.unitPrice * share) })));
  units.sort((a, b) => a.price - b.price || a.li - b.li || a.u - b.u);
  const sum = round2(units.reduce((a, u) => a + u.price, 0));
  if (units.length) units[units.length - 1].price = round2(units[units.length - 1].price + payable - sum);
  let left = Math.max(0, paid);
  return units.map((u) => {
    const take = round2(Math.min(left, Math.max(0, u.price)));
    left = round2(left - take);
    return { key: u.key, name: u.name, unit: u.u, units: u.units, price: u.price, paid: take, owed: round2(u.price - take) };
  });
}

export function CreditAllocation({ lines, payable, paid }: { lines: AllocLine[]; payable: number; paid: number }) {
  const [open, setOpen] = useState(true);
  const rows = allocate(lines, payable, paid);
  const owed = round2(rows.reduce((a, r) => a + r.owed, 0));
  const records = rows.filter((r) => r.owed > 0).length;
  return (
    <div className="mt-2 rounded-lg border bg-background/60 text-xs" data-testid="credit-allocation">
      <button type="button" className="flex w-full items-center justify-between gap-2 p-2.5 text-start" onClick={() => setOpen(!open)} aria-expanded={open}>
        <span className="font-semibold">{t("Payment Allocation Summary")}</span>
        <span className="flex items-center gap-1.5 text-muted-foreground">
          <span className="num">{records} {t(records === 1 ? "credit record" : "credit records")} · {money(owed)}</span>
          <ChevronDown className={cn("h-3.5 w-3.5 transition-transform", open && "rotate-180")} />
        </span>
      </button>
      {open && (
        <ul className="divide-y border-t">
          {rows.map((r, i) => {
            const status = r.owed <= 0 ? "Fully paid" : r.paid > 0 ? "Partially paid" : "Unpaid";
            return (
              <li key={`${r.key}-${r.unit}-${i}`} className="px-2.5 py-1.5">
                <div className="flex items-center justify-between gap-2">
                  <span className="min-w-0 font-medium"><ProductName name={r.name} />{r.units > 1 && <span className="text-muted-foreground"> · {r.unit}/{r.units}</span>}</span>
                  <span className={cn("shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium", r.owed <= 0 ? "bg-success/15 text-success" : r.paid > 0 ? "bg-warning/15 text-warning" : "bg-destructive/10 text-destructive")}>{t(status)}</span>
                </div>
                <div className="num mt-0.5 flex justify-between text-muted-foreground">
                  <span>{t("Price")} {money(r.price)}</span>
                  <span>{t("Paid")} {money(r.paid)}</span>
                  <span className={cn(r.owed > 0 && "font-medium text-foreground")}>{t("Credit")} {money(r.owed)}</span>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
