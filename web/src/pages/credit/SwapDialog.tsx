/** Roadmap 179: swap an item sold on credit for another product. The item comes back (what is owed on it is cancelled;
 * any money already paid beyond that carries over), and the replacement is a new sale linked to this one — on credit
 * or paid now. Everything is shown before confirming; the server checks it all again. */
import { useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowRight, Repeat, ScanLine } from "lucide-react";
import { api } from "@/lib/api";
import { useNavigate } from "@/lib/router";
import { useDebounced } from "@/lib/hooks";
import { money, toNum } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import type { CreditRow, PosProduct, SaleDetail } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Field } from "@/components/Form";
import { Chip } from "@/components/Filters";
import { BarcodeScanner } from "@/components/BarcodeScanner";
import type { SoldItem } from "./RecallDialog";

export function SwapDialog({ open, onOpenChange, credit, items, onDone }: {
  open: boolean; onOpenChange: (o: boolean) => void; credit: CreditRow; items: SoldItem[]; onDone: () => void;
}) {
  const { profile } = useSession();
  const navigate = useNavigate();
  const s = profile!.settings.sales;
  const back = items.filter((i) => i.quantity > i.returned_qty);
  const [lineId, setLineId] = useState<string>(back[0]?.id ?? "");
  const [code, setCode] = useState("");
  const [scan, setScan] = useState(false);
  const [q, setQ] = useState("");
  const term = useDebounced(q);
  const [product, setProduct] = useState<PosProduct | null>(null);
  const [method, setMethod] = useState("credit");
  const [reference, setReference] = useState("");
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const clientRef = useRef(crypto.randomUUID());
  const line = back.find((i) => i.id === lineId);
  const products = useQuery({
    queryKey: ["pos-products", "swap", credit.branch_id, term],
    queryFn: () => api<PosProduct[]>("/pos/products", { query: { q: term, branch_id: credit.branch_id, all: true } }),
    enabled: open && term.trim().length >= 2,
  });
  const methods = [{ key: "credit", label: t("On credit") }, ...s.payment_methods.filter((m) => m.enabled && m.key !== "credit").map((m) => ({ key: m.key, label: m.label }))];
  // What the server will do (it decides): the returned item cancels what is owed on it; money paid beyond carries over.
  const value = line ? toNum(line.unit_price) : 0;
  const balance = toNum(credit.balance);
  const carried = Math.max(value - balance, 0);
  const price = product ? toNum(product.marked_price) : 0;
  const due = Math.max(price - carried, 0);
  const swap = useMutation({
    mutationFn: () => api<SaleDetail | { pending_approval: true }>(`/credit/${credit.id}/swap`, {
      body: {
        return_items: [{ sale_item_id: lineId, quantity: 1, barcodes: line?.tracked ? [code.trim()] : [] }],
        items: [{ product_id: product!.id, quantity: 1, unit_price: price }],
        payment: { method, reference: method === "mpesa" ? reference : "" },
        reason,
        client_ref: clientRef.current,
      },
    }),
    onSuccess: (r) => {
      onOpenChange(false);
      onDone();
      if ("pending_approval" in r) {
        toast.success("Swap sent for approval — nothing changes until it is approved");
        return;
      }
      toast.success("Swap recorded");
      navigate(r.credit ? `/credit/${r.credit.id}` : `/sales/${r.sale.id}`);
    },
    onError: (e) => toast.error(e),
  });

  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} title="Swap item" description="The item comes back and the customer takes another product as a new, linked sale."
      footer={<ActionButton className="w-full md:w-auto" online busy={swap.isPending} busyLabel="Recording…"
        blockedBy={[!line && "Choose the item coming back", !!line?.tracked && !code.trim() && "Scan the item's barcode", !product && "Choose the replacement",
          !!product && product.marked_price == null && "The replacement has no price", method === "mpesa" && reference.length < 8 && "Enter the M-Pesa code",
          reason.trim().length < 3 && "Give the reason", !confirmed && "Confirm"]}
        onAction={() => swap.mutateAsync()}><Repeat /> {t("Record swap")}</ActionButton>}>
      <div className="space-y-4 text-sm" data-testid="swap-dialog">
        <Field label="Coming back">
          <div className="space-y-1.5">
            {back.map((i) => (
              <button key={i.id} type="button" onClick={() => { setLineId(i.id); setConfirmed(false); }}
                className={cn("flex w-full items-center justify-between rounded-lg border p-2.5 text-start", lineId === i.id && "border-primary bg-primary/5")}>
                <span className="min-w-0"><span className="block truncate font-medium">{i.product_name}</span><span className="num block text-xs text-muted-foreground">{i.product_code}{i.barcode && ` · ${i.barcode}`}</span></span>
                <span className="num shrink-0">{money(i.unit_price)}</span>
              </button>
            ))}
          </div>
        </Field>
        {line?.tracked && (
          <Field label="Barcode of the item coming back">
            <div className="flex gap-2">
              <Input className="num" value={code} onChange={(e) => setCode(e.target.value)} placeholder={t("Scan or type")} />
              <Button type="button" variant="outline" onClick={() => setScan(true)}><ScanLine /> {t("Scan")}</Button>
            </div>
          </Field>
        )}
        <Field label="Taking instead">
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("Search products")} />
          {q.trim().length >= 2 && term.trim().length >= 2 && (
            <ul className="mt-1.5 max-h-48 divide-y overflow-y-auto rounded-lg border">
              {products.data?.filter((p) => p.available > 0).map((p) => (
                <li key={p.id}>
                  <button type="button" onClick={() => { setProduct(p); setQ(""); setConfirmed(false); }} className="flex w-full items-center justify-between gap-2 px-3 py-2 text-start hover:bg-accent">
                    <span className="min-w-0 truncate">{p.name}</span><span className="num shrink-0 text-muted-foreground">{p.marked_price == null ? t("Price not set") : money(p.marked_price)}</span>
                  </button>
                </li>
              ))}
              {products.data && !products.data.some((p) => p.available > 0) && <li className="px-3 py-2 text-muted-foreground">{t("Nothing in stock matches.")}</li>}
            </ul>
          )}
          {product && <p className="mt-1.5 rounded-lg bg-muted/60 p-2 font-medium">{product.name} · <span className="num">{money(price)}</span></p>}
        </Field>
        <Field label="The replacement is paid">
          <div className="flex flex-wrap gap-2">{methods.map((m) => <Chip key={m.key} active={method === m.key} onClick={() => { setMethod(m.key); setConfirmed(false); }}>{m.label}</Chip>)}</div>
        </Field>
        {method === "mpesa" && <Field label="M-Pesa confirmation code"><Input className="num uppercase" value={reference} onChange={(e) => setReference(e.target.value.toUpperCase())} /></Field>}
        {line && product && (
          <div className="num space-y-1 rounded-xl border border-primary/30 bg-primary/5 p-3 text-xs" data-testid="swap-preview">
            <p className="flex items-center gap-1.5 text-sm font-medium">{line.product_name} <ArrowRight className="h-3.5 w-3.5" /> {product.name}</p>
            <p className="flex justify-between"><span>{t("Owed on this credit")}</span><span>{money(balance)} → {money(Math.max(balance - value, 0))}</span></p>
            {carried > 0 && <p className="flex justify-between"><span>{t("Already paid, carried over")}</span><span>{money(carried)}</span></p>}
            <p className="flex justify-between"><span>{t("Replacement")}</span><span>{money(price)}</span></p>
            <p className="flex justify-between text-sm font-semibold"><span>{method === "credit" ? t("New credit balance") : t("To pay now")}</span><span>{money(due)}</span></p>
            {carried > price && <p className="text-muted-foreground">{money(carried - price)} {t("stays as return credit for the customer.")}</p>}
          </div>
        )}
        <Field label="Reason"><Textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("e.g. customer wants a different scent")} /></Field>
        <label className="flex items-start gap-2.5 text-xs">
          <Checkbox checked={confirmed} onCheckedChange={(v) => setConfirmed(v === true)} className="mt-0.5" />
          <span>{t("The item is back in my hands; record the return and the replacement sale.")}</span>
        </label>
      </div>
      <BarcodeScanner open={scan} onOpenChange={setScan} onDetected={(c) => setCode(c)} title="Scan the item coming back" />
    </ResponsiveDialog>
  );
}
