import { useState } from "react";
import { CheckCircle2, RotateCcw, ScanLine, Store } from "lucide-react";
import { api, type ApiError } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { count, money, toNum } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { CreditRow, Money, Outcome } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Field, Select } from "@/components/Form";
import { Segments } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { BarcodeScanner } from "@/components/BarcodeScanner";

export interface SoldItem {
  id: string;
  product_name: string;
  product_code: string;
  quantity: number;
  returned_qty: number;
  unit_price: Money;
  barcode: string | null;
  tracked: boolean;
}

/**
 * Recall a credit sale: goods come back to the branch they were sold from. Tracked units must be scanned again
 * and match the unit sold; the balance is reduced and any overpayment is refunded now or kept as customer credit.
 */
export function RecallDialog({ open, onOpenChange, credit, items, onDone }: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  credit: CreditRow;
  items: SoldItem[];
  onDone: () => void;
}) {
  const { profile } = useSession();
  const left = items.filter((i) => i.quantity > i.returned_qty);
  const [qty, setQty] = useState<Record<string, number>>({});
  const [scanned, setScanned] = useState<Record<string, string>>({});
  const [scanFor, setScanFor] = useState<SoldItem | null>(null);
  const [reason, setReason] = useState("");
  const [settle, setSettle] = useState<"refund" | "credit">("credit");
  const [method, setMethod] = useState("cash");
  const [busy, setBusy] = useState(false);
  const methods = profile?.settings.sales.payment_methods.filter((m) => m.enabled && m.key !== "credit") ?? [];

  const chosen = left.filter((i) => (qty[i.id] ?? 0) > 0);
  const value = chosen.reduce((a, i) => a + toNum(i.unit_price) * (qty[i.id] ?? 0), 0);
  const balance = Math.max(toNum(credit.balance), 0);
  const after = Math.max(balance - value, 0);
  const over = Math.max(value - balance, 0);
  const unscanned = chosen.filter((i) => i.tracked && !scanned[i.id]);
  const everything = left.length > 0 && left.every((i) => (qty[i.id] ?? 0) === i.quantity - i.returned_qty);

  const reset = () => {
    setQty({});
    setScanned({});
    setReason("");
    setSettle("credit");
  };
  const submit = async () => {
    setBusy(true);
    try {
      const r = await api<Outcome<unknown>>(`/credit/${credit.id}/recall`, {
        body: {
          items: chosen.map((i) => ({ sale_item_id: i.id, quantity: qty[i.id], barcodes: scanned[i.id] ? [scanned[i.id]] : [] })),
          reason: reason.trim(),
          settle,
          refund_method: settle === "refund" ? method : "",
        },
      });
      toast.success(r.pending_approval ? t("Recall sent for approval") : t("Goods recalled to stock"));
      reset();
      onOpenChange(false);
      onDone();
    } catch (e) {
      toast.error(e as ApiError);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <ResponsiveDialog
        open={open}
        onOpenChange={(o) => {
          if (busy) return;
          if (!o) reset();
          onOpenChange(o);
        }}
        title="Recall credit sale"
        description={`${credit.customer_name} · ${credit.receipt_no}`}
        footer={
          <ActionButton className="w-full md:w-auto" online busy={busy} busyLabel="Recalling…"
            blockedBy={[!chosen.length && "Choose items", unscanned.length > 0 && "Scan the items", reason.trim().length < 3 && REASONS.enterReason]}
            onAction={submit}>
            <RotateCcw /> {t("Recall to stock")}
          </ActionButton>
        }
      >
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-2 text-sm">
            <div className="rounded-lg bg-muted p-2.5">
              <p className="label-caps">{t("Outstanding")}</p>
              <p className="num font-semibold">{money(credit.balance)}</p>
            </div>
            <div className="rounded-lg bg-muted p-2.5">
              <p className="label-caps">{t("Returns to")}</p>
              <p className="flex items-center gap-1 font-semibold"><Store className="h-3.5 w-3.5" /> {credit.branch_name}</p>
            </div>
          </div>

          <div className="flex items-center justify-between">
            <p className="label-caps">{t("Items")}</p>
            <button
              type="button"
              className="text-xs font-medium text-primary"
              onClick={() => setQty(everything ? {} : Object.fromEntries(left.map((i) => [i.id, i.quantity - i.returned_qty])))}
            >
              {everything ? t("Clear") : t("Recall everything")}
            </button>
          </div>
          <ul className="divide-y rounded-lg border">
            {left.map((i) => {
              const max = i.quantity - i.returned_qty;
              const n = qty[i.id] ?? 0;
              return (
                <li key={i.id} className="space-y-2 p-3">
                  <div className="flex items-start justify-between gap-3 text-sm">
                    <div className="min-w-0">
                      <div className="truncate font-medium">{i.product_name}</div>
                      <div className="num truncate text-xs text-muted-foreground">
                        {i.product_code}{i.barcode && ` · ${i.barcode}`} · {money(i.unit_price)}
                        {i.returned_qty > 0 && ` · ${count(i.returned_qty)} ${t("already returned")}`}
                      </div>
                    </div>
                    {i.tracked ? (
                      <Button size="sm" variant={n ? "outline" : "ghost"} onClick={() => setQty({ ...qty, [i.id]: n ? 0 : 1 })}>
                        {n ? t("Selected") : t("Select")}
                      </Button>
                    ) : (
                      <Input
                        inputMode="numeric"
                        className="num h-9 w-20 text-center"
                        value={n || ""}
                        placeholder="0"
                        aria-label={t("Quantity")}
                        onChange={(e) => setQty({ ...qty, [i.id]: Math.min(Number(e.target.value.replace(/\D/g, "")) || 0, max) })}
                      />
                    )}
                  </div>
                  {i.tracked && n > 0 && (
                    <div className={cn("flex items-center gap-2 rounded-lg border-2 border-dashed p-2 text-xs", scanned[i.id] ? "border-success/50 bg-success/5" : "border-primary/40")}>
                      {scanned[i.id] ? <CheckCircle2 className="h-4 w-4 text-success" /> : <ScanLine className="h-4 w-4 text-primary" />}
                      <span className="flex-1">{scanned[i.id] ? t("Returned unit verified") : t("Scan the returned item's barcode")}</span>
                      <Button size="sm" variant={scanned[i.id] ? "outline" : "default"} onClick={() => setScanFor(i)}>{scanned[i.id] ? t("Rescan") : t("Scan")}</Button>
                    </div>
                  )}
                </li>
              );
            })}
          </ul>

          {chosen.length > 0 && (
            <div className="space-y-1 rounded-lg bg-muted p-3 text-sm">
              <div className="flex justify-between"><span>{t("Value recalled")}</span><span className="num">≈ {money(value)}</span></div>
              <div className="flex justify-between"><span>{t("Balance now")}</span><span className="num">{money(balance)}</span></div>
              <div className="flex justify-between font-semibold"><span>{t("Balance after recall")}</span><span className="num">≈ {money(after)}</span></div>
              {over > 0 && <div className="flex justify-between font-semibold text-warning"><span>{t("Customer has overpaid")}</span><span className="num">≈ {money(over)}</span></div>}
            </div>
          )}
          {over > 0 && (
            <div className="space-y-2">
              <Segments value={settle} onChange={setSettle} options={[{ value: "credit", label: "Keep as customer credit" }, { value: "refund", label: "Refund now" }]} />
              {settle === "refund" && (
                <Select value={method} onChange={setMethod} label="Refund method">
                  {methods.map((m) => <option key={m.key} value={m.key}>{m.label}</option>)}
                </Select>
              )}
              <p className="text-xs text-muted-foreground">
                {settle === "credit" ? t("Recorded on the recall for follow-up; payment history is kept as it is.") : t("A refund payment is recorded against the sale.")}
              </p>
            </div>
          )}
          <Field label="Recall reason" hint="Required — saved on the recall, the stock record and the audit trail">
            <Textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("e.g. customer could not pay; goods collected back")} />
          </Field>
        </div>
      </ResponsiveDialog>
      <BarcodeScanner
        open={!!scanFor}
        onOpenChange={(o) => !o && setScanFor(null)}
        title={scanFor ? `${t("Scan")} ${scanFor.product_name}` : ""}
        onDetected={(code) => {
          if (!scanFor) return;
          if (code.trim() !== scanFor.barcode) {
            return { tone: "error", title: t("Barcode mismatch"), detail: t("This is not the unit sold on this credit sale. Scan the returned item's own barcode.") };
          }
          setScanned((s) => ({ ...s, [scanFor.id]: code.trim() }));
        }}
      />
    </>
  );
}
