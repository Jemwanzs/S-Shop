/** Roadmap 179: settle a credit sale with a returned item — an item of a sale the customer actually paid for comes back
 * (or a return credit already held is used), and the money paid for it goes to this credit instead of being refunded.
 * The server decides what is eligible: only money really paid becomes credit, never more than is owed or left. */
import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { PackageOpen, Search, Undo2 } from "lucide-react";
import { api } from "@/lib/api";
import { useDebounced } from "@/lib/hooks";
import { dateTime, money, toNum } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import type { Money, Outcome } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Checkbox } from "@/components/ui/checkbox";
import { ActionButton } from "@/components/ActionButton";
import { Field } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { ProductName } from "@/components/ProductName";

interface Sources {
  balance: Money;
  credits: { id: string; return_no: string; receipt_no: string; customer_name: string | null; amount: Money; used: Money; remaining: Money; created_at: string; products: string | null; branch_name: string }[];
  sales: {
    sale_id: string; receipt_no: string; created_at: string; customer_name: string | null; branch_name: string; payment_method: string; total: Money; amount_paid: Money; outstanding: Money;
    lines: { sale_item_id: string; product_name: string; product_code: string; barcode: string | null; returnable: number; unit_price: Money; tracked: boolean }[];
  }[];
}
interface Preview { return_value: Money; paid_value: Money; reduces_owed: Money; balance: Money; max_apply: Money }
type Choice =
  | { kind: "credit"; id: string; label: string; max: number }
  | { kind: "item"; sale_id: string; sale_item_id: string; label: string; receipt: string };

export function ReturnSettle({ creditId, balance, onDone }: { creditId: string; balance: Money; onDone: () => void }) {
  const [q, setQ] = useState("");
  const term = useDebounced(q);
  const [choice, setChoice] = useState<Choice | null>(null);
  const [amount, setAmount] = useState("");
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const sources = useQuery({
    queryKey: ["credit", creditId, "return-sources", term],
    queryFn: () => api<Sources>(`/credit/${creditId}/return-sources`, { query: { q: term.trim() || undefined } }),
  });
  const item = choice?.kind === "item" ? choice : null;
  const preview = useQuery({
    queryKey: ["credit", creditId, "return-preview", item?.sale_item_id],
    queryFn: () => api<Preview>(`/credit/${creditId}/return-credit/preview`, { body: { sale_id: item!.sale_id, items: [{ sale_item_id: item!.sale_item_id, quantity: 1 }] } }),
    enabled: !!item,
    retry: false,
  });
  const max = choice?.kind === "credit" ? choice.max : toNum(preview.data?.max_apply ?? 0);
  const apply = amount === "" ? max : toNum(amount);
  const after = toNum(balance) - apply;
  const pick = (c: Choice) => { setChoice(c); setAmount(""); setConfirmed(false); };
  const settle = useMutation({
    mutationFn: () => api<Outcome<{ applied: Money; balance: Money; status: string }>>(`/credit/${creditId}/return-credit`, {
      body: choice?.kind === "credit"
        ? { return_credit_id: choice.id, amount: apply }
        : { source: { sale_id: item!.sale_id, items: [{ sale_item_id: item!.sale_item_id, quantity: 1 }], reason }, amount: apply },
    }),
    onSuccess: (r) => {
      toast.success(r.pending_approval ? "Return sent for approval — the balance changes once it is approved" : r.result?.status === "paid" ? "Fully paid 🎉" : `${money(r.result?.applied ?? apply)} applied · balance ${money(r.result?.balance ?? after)}`);
      onDone();
    },
    onError: (e) => toast.error(e),
  });

  return (
    <div className="space-y-4" data-testid="return-settle">
      <div className="relative">
        <Search className="pointer-events-none absolute inset-s-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input className="ps-9" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("Receipt, customer, product or barcode of the paid sale")} aria-label={t("Find the paid sale")} />
      </div>

      {!!sources.data?.credits.length && (
        <div>
          <p className="label-caps mb-1.5">{t("Return credits available")}</p>
          <ul className="space-y-1.5">
            {sources.data.credits.map((c) => (
              <li key={c.id}>
                <button type="button" onClick={() => pick({ kind: "credit", id: c.id, label: `${c.return_no} · ${c.products ?? ""}`, max: Math.min(toNum(c.remaining), toNum(balance)) })}
                  className={cn("flex w-full items-center justify-between gap-3 rounded-lg border p-2.5 text-start text-sm", choice?.kind === "credit" && choice.id === c.id && "border-primary bg-primary/5")}>
                  <span className="min-w-0"><span className="block truncate font-medium">{c.products ?? c.return_no}</span>
                    <span className="num block text-xs text-muted-foreground">{c.return_no} · {c.receipt_no}{c.customer_name && ` · ${c.customer_name}`} · {dateTime(c.created_at)}</span></span>
                  <span className="shrink-0 text-end"><span className="num block font-semibold">{money(c.remaining)}</span>
                    {toNum(c.used) > 0 && <span className="num block text-[11px] text-muted-foreground">{t("of")} {money(c.amount)} · {money(c.used)} {t("used")}</span>}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {term.trim().length >= 2 && (
        <div>
          <p className="label-caps mb-1.5">{t("Items from paid sales")}</p>
          {sources.isLoading ? <p className="text-sm text-muted-foreground">{t("Searching…")}</p> : !sources.data?.sales.length ? (
            <p className="text-sm text-muted-foreground">{t("No sale with an item to return matches.")}</p>
          ) : (
            <ul className="space-y-2">
              {sources.data.sales.map((s) => (
                <li key={s.sale_id} className="rounded-lg border p-2.5 text-sm">
                  <div className="num flex flex-wrap justify-between gap-x-3 text-xs text-muted-foreground">
                    <span>{s.receipt_no} · {dateTime(s.created_at)}{s.customer_name && ` · ${s.customer_name}`}</span>
                    <span>{t("Paid")} {money(s.amount_paid)}{toNum(s.outstanding) > 0 && <span className="text-warning"> · {t("owes")} {money(s.outstanding)}</span>}</span>
                  </div>
                  <ul className="mt-1.5 space-y-1">
                    {s.lines.map((l) => (
                      <li key={l.sale_item_id}>
                        <button type="button" onClick={() => { pick({ kind: "item", sale_id: s.sale_id, sale_item_id: l.sale_item_id, label: l.product_name, receipt: s.receipt_no }); }}
                          className={cn("flex w-full items-center justify-between gap-2 rounded-md px-2 py-1.5 text-start hover:bg-muted", item?.sale_item_id === l.sale_item_id && "bg-primary/10")}>
                          <span className="min-w-0"><ProductName name={l.product_name} expandable={false} className="block font-medium" />
                            <span className="num block text-[11px] text-muted-foreground">{l.product_code}{l.barcode && ` · ${l.barcode}`}</span></span>
                          <span className="num shrink-0">{money(l.unit_price)}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {!choice && !sources.data?.credits.length && term.trim().length < 2 && (
        <p className="flex items-start gap-2 rounded-lg bg-muted/60 p-3 text-xs text-muted-foreground">
          <PackageOpen className="mt-0.5 h-4 w-4 shrink-0" />
          {t("Find the sale the returned item came from. Only money actually paid for it can settle this credit; an item still owed on credit comes back through Recall instead.")}
        </p>
      )}

      {choice && (
        <div className="space-y-3 rounded-xl border border-primary/30 bg-primary/5 p-3 text-sm" data-testid="return-settle-preview">
          <p className="font-medium">{choice.label}</p>
          {item && (preview.isLoading ? <p className="text-xs text-muted-foreground">{t("Checking…")}</p> : preview.error ? (
            <p role="alert" className="text-xs text-destructive">{preview.error instanceof Error ? preview.error.message : t("This item cannot be returned")}</p>
          ) : preview.data && (
            <dl className="num grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 text-xs">
              <dt className="text-muted-foreground">{t("Value of the item")}</dt><dd>{money(preview.data.return_value)}</dd>
              {toNum(preview.data.reduces_owed) > 0 && <><dt className="text-muted-foreground">{t("Only cancels what is still owed on")} {item.receipt}</dt><dd>−{money(preview.data.reduces_owed)}</dd></>}
              <dt className="font-medium">{t("Paid value — eligible credit")}</dt><dd className="font-semibold">{money(preview.data.paid_value)}</dd>
            </dl>
          ))}
          {max > 0 && (
            <>
              <Field label="Amount to apply" hint={`${t("Up to")} ${money(max)}`}>
                <Input inputMode="decimal" className="num bg-background" value={amount} placeholder={String(max)} onChange={(e) => { setAmount(e.target.value.replace(/[^\d.]/g, "")); setConfirmed(false); }} />
              </Field>
              {item && <Field label="Reason for the return"><Textarea rows={2} className="bg-background" value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("e.g. customer brought it back unopened")} /></Field>}
              <div className="num flex items-center justify-between rounded-lg bg-background p-2.5">
                <span>{t("Balance")} {money(balance)} → <b>{money(Math.max(after, 0))}</b></span>
                {after <= 0 && <Pill tone="success">{t("Paid")}</Pill>}
              </div>
              <label className="flex items-start gap-2.5 text-xs">
                <Checkbox checked={confirmed} onCheckedChange={(v) => setConfirmed(v === true)} className="mt-0.5" />
                <span>{item ? t("Take the item back into stock and apply its paid value to this credit.") : t("Apply this return credit to this credit sale.")}</span>
              </label>
            </>
          )}
          <ActionButton className="w-full" online busy={settle.isPending} busyLabel="Applying…"
            blockedBy={[
              max <= 0 && (item && preview.data ? "Nothing paid to apply" : "Choose a credit or an item"),
              (apply <= 0 || apply > max) && "Check the amount",
              !!item && reason.trim().length < 3 && "Give the reason",
              !confirmed && "Confirm",
            ]}
            onAction={() => settle.mutateAsync()}><Undo2 /> {t("Apply return credit")}</ActionButton>
        </div>
      )}
    </div>
  );
}
