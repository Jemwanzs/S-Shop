/** Roadmap 176: selling prices on a transfer — what the destination's price will be, shown when the transfer is made, to
 * approvers, and on the transfer itself (where someone allowed to set prices there authorises or declines a request). */
import { useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { count, money } from "@/lib/format";
import { t } from "@/lib/i18n";
import type { Money } from "@/lib/types";
import { Pill, type Tone } from "@/components/Badges";
import { Button } from "@/components/ui/button";

export type PriceAction = "keep" | "source" | "custom";

export const PRICE_ACTION: Record<PriceAction, string> = {
  keep: "Keep Destination Price",
  source: "Use Source Branch Price",
  custom: "Set Custom Destination Price",
};

export const PRICE_STATUS: Record<string, [Tone, string]> = {
  unchanged: ["neutral", "Destination price unchanged"],
  requested: ["warning", "Price change requested"],
  authorized: ["info", "Price change authorised — applies on receipt"],
  applied: ["success", "Destination price changed"],
  declined: ["danger", "Price change declined"],
  not_applied: ["neutral", "Not applied (transfer not completed)"],
};

const price = (v: Money | null | undefined) => (v == null ? t("Not set") : money(v));

/** One line of an approval request for a transfer (from its payload). */
export interface TransferApprovalLine {
  product_id: string; product_name: string; product_code: string; quantity: number;
  source_price: Money | null; dest_price: Money | null; price_action: PriceAction; new_price: Money | null; price_status: string;
}
export interface TransferApprovalPayload { transfer_no: string; from_branch_name: string; to_branch_name: string; total_units: number; lines: TransferApprovalLine[] }

export function isTransferPayload(p: unknown): p is TransferApprovalPayload {
  return !!p && typeof p === "object" && Array.isArray((p as TransferApprovalPayload).lines);
}

/** What an approver sees: every product and quantity, the route, both prices and the proposed price action. */
export function TransferApprovalLines({ p, open = false }: { p: TransferApprovalPayload; open?: boolean }) {
  const [all, setAll] = useState(open);
  const shown = all ? p.lines : p.lines.slice(0, 2);
  return (
    <div className="space-y-2 text-xs" data-testid="transfer-approval-lines">
      <p><span className="text-muted-foreground">{t("From")}</span> <b>{p.from_branch_name}</b> <span className="text-muted-foreground">{t("to")}</span> <b>{p.to_branch_name}</b> · <span className="num">{count(p.total_units)}</span> {t(p.total_units === 1 ? "unit" : "units")}</p>
      <ul className="divide-y rounded-lg border">
        {shown.map((l) => (
          <li key={l.product_id} className="space-y-1 p-2">
            <div className="flex justify-between gap-2"><span className="min-w-0 font-medium">{l.product_name}</span><span className="num shrink-0 font-semibold">× {count(l.quantity)}</span></div>
            <div className="num text-muted-foreground">{t("Source price")} {price(l.source_price)} · {t("Destination price")} {price(l.dest_price)}</div>
            <div className="flex flex-wrap items-center gap-1.5">
              <Pill tone={l.price_action === "keep" ? "neutral" : "warning"}>{t(PRICE_ACTION[l.price_action] ?? l.price_action)}</Pill>
              {l.price_action !== "keep" && <span className="num font-medium">→ {price(l.new_price)}</span>}
              {l.price_status === "requested" && <span className="text-warning">{t("needs someone who sets prices at")} {p.to_branch_name}</span>}
            </div>
          </li>
        ))}
      </ul>
      {p.lines.length > 2 && (
        <Button type="button" variant="link" size="sm" className="h-auto p-0 text-xs" onClick={() => setAll(!all)}>
          {all ? <><ChevronUp className="h-3.5 w-3.5" /> {t("Show less")}</> : <><ChevronDown className="h-3.5 w-3.5" /> {t("Show all")} {p.lines.length} {t("products")}</>}
        </Button>
      )}
    </div>
  );
}
