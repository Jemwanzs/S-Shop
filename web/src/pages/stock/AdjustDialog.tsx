import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ScanLine } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count } from "@/lib/format";
import type { Outcome, Paged, Product } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Field, Select } from "@/components/Form";
import { SearchInput } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { BarcodeScanner } from "@/components/BarcodeScanner";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useManualBarcodes } from "@/lib/scan";

const KINDS = [
  ["damage", "Damaged"],
  ["loss", "Lost / missing"],
  ["write_off", "Write-off"],
  ["customer_return", "Customer return (back to stock)"],
  ["supplier_return", "Returned to supplier"],
  ["count", "Recount (set exact quantity)"],
  ["manual", "Manual correction (+/−)"],
  // Roadmap 171: last, with a direction and a written explanation.
  ["other", "Others"],
] as const;

/** Roadmap 171: an "Others" explanation counts once it says something (at least 10 characters, spaces not counted). */
const explained = (s: string) => s.replace(/\s/g, "").length >= 10;

/** Every adjustment records reason, user, time, before/after — never a silent overwrite. */
export function AdjustDialog({ open, onOpenChange, initial }: { open: boolean; onOpenChange: (o: boolean) => void; initial?: Product | null }) {
  const { can } = useSession();
  const qc = useQueryClient();
  const [product, setProduct] = useState<Product | null>(initial ?? null);
  const [q, setQ] = useState("");
  const term = useDebounced(q);
  const [kind, setKind] = useState<string>("damage");
  const [qty, setQty] = useState("");
  const [barcode, setBarcode] = useState("");
  const typingAllowed = useManualBarcodes("stock");
  const [reason, setReason] = useState("");
  const [direction, setDirection] = useState<"in" | "out" | null>(null);
  const other = kind === "other";
  const [scan, setScan] = useState(false);
  const results = useQuery({ queryKey: ["products", "adjust", term], queryFn: () => api<Paged<Product>>("/products", { query: { q: term, limit: 8 } }), enabled: open && !!term && !product });
  const current = product ?? initial ?? null;
  const tracked = !!current?.track_items;
  const kinds = KINDS.filter(([k]) => (k === "write_off" ? can("stock.write_off") : can("stock.adjust")) && !(tracked && (k === "count" || k === "manual")));

  const reset = () => {
    setProduct(null);
    setQ("");
    setQty("");
    setBarcode("");
    setReason("");
    setDirection(null);
  };
  const save = useMutation({
    mutationFn: () =>
      api<Outcome<{ previous_qty: number; new_qty: number }>>("/stock/adjustments", {
        body: {
          product_id: current!.id,
          kind,
          counted_qty: kind === "count" ? parseInt(qty) : undefined,
          quantity: kind === "count" ? undefined : tracked ? 1 : parseInt(qty),
          barcode: tracked ? barcode : undefined,
          direction: other ? direction : undefined,
          reason,
        },
      }),
    onSuccess: (r) => {
      toast.success(r.pending_approval ? "Adjustment sent for approval" : `Stock updated: ${r.result?.previous_qty} → ${r.result?.new_qty}`);
      qc.invalidateQueries({ queryKey: ["stock"] });
      qc.invalidateQueries({ queryKey: ["product"] });
      reset();
      onOpenChange(false);
    },
    onError: (e) => toast.error(e),
  });
  const valid = current && reason.trim() && (!other || (direction && explained(reason))) && (tracked ? barcode.trim() : qty !== "" && (kind === "manual" ? parseInt(qty) !== 0 : other ? parseInt(qty) > 0 : parseInt(qty) >= 0));

  return (
    <>
      <ResponsiveDialog
        open={open}
        onOpenChange={(o) => { if (!o) reset(); onOpenChange(o); }}
        title="Adjust stock"
        description="Sensitive adjustments may need approval."
        footer={<ActionButton className="w-full md:w-auto" online busy={save.isPending} busyLabel="Saving…"
          blockedBy={[other && !direction && "Choose add or remove", other && !explained(reason) && "Specify the adjustment reason", !valid && REASONS.completeFields]}
          onAction={() => save.mutateAsync()}>{t("Save adjustment")}</ActionButton>}
      >
        <div className="space-y-4">
          {current ? (
            <div className="flex items-center justify-between rounded-xl bg-muted/60 p-3">
              <div>
                <div className="font-medium">{current.name}</div>
                <div className="num text-xs text-muted-foreground">{count(current.on_hand)} on hand · {count(current.reserved)} reserved</div>
              </div>
              {!initial && <Button variant="ghost" size="sm" onClick={() => setProduct(null)}>{t("Change")}</Button>}
            </div>
          ) : (
            <div className="space-y-2">
              <SearchInput value={q} onChange={setQ} placeholder="Find product" autoFocus />
              <ul className="divide-y rounded-lg border">
                {results.data?.items.map((p) => (
                  <li key={p.id}><button className="flex w-full justify-between px-3 py-2.5 text-start text-sm hover:bg-accent" onClick={() => setProduct(p)}><span>{p.name}</span><span className="num text-muted-foreground">{count(p.on_hand)}</span></button></li>
                ))}
              </ul>
            </div>
          )}
          <Field label="Type">
            <Select value={kind} onChange={setKind}>
              {kinds.map(([k, label]) => <option key={k} value={k}>{t(label)}</option>)}
            </Select>
          </Field>
          {other && (
            <Field group label="Stock is">
              <div className="grid grid-cols-2 gap-2" role="radiogroup" aria-label={t("Stock is")}>
                {([["in", "Added to stock"], ["out", "Removed from stock"]] as const).map(([d, label]) => (
                  <button key={d} type="button" role="radio" aria-checked={direction === d} onClick={() => setDirection(d)}
                    className={cn("h-11 rounded-xl border text-sm font-medium", direction === d ? "border-primary bg-primary/5 ring-1 ring-primary" : "bg-card hover:bg-accent")}>{t(label)}</button>
                ))}
              </div>
            </Field>
          )}
          {tracked ? (
            <Field label="Item barcode">
              <div className="flex gap-2">
                {/* Roadmap 137: typing follows Settings → Stock (manual barcode entry); scanning always works. */}
                <Input className="num" value={barcode} readOnly={!typingAllowed} placeholder={typingAllowed ? "" : t("Scan barcode")} onChange={(e) => setBarcode(e.target.value)} />
                <Button variant="outline" onClick={() => setScan(true)}><ScanLine /> {t("Scan")}</Button>
              </div>
            </Field>
          ) : (
            <Field label={kind === "count" ? "Counted quantity" : kind === "manual" ? "Change (use − to remove)" : "Quantity"}>
              <Input inputMode={kind === "manual" ? "text" : "numeric"} className="num" value={qty} onChange={(e) => setQty(e.target.value.replace(kind === "manual" ? /[^\d-]/g : /\D/g, ""))} />
            </Field>
          )}
          {other ? (
            <Field label="Specify Adjustment Reason" hint={explained(reason) || !reason ? "Kept with the adjustment, shown to approvers and in the history" : "Explain what happened (at least 10 characters)"}>
              <Textarea value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("e.g. Stock correction following physical verification and reconciliation.")} />
            </Field>
          ) : (
            <Field label="Reason"><Textarea value={reason} onChange={(e) => setReason(e.target.value)} placeholder="What happened?" /></Field>
          )}
        </div>
      </ResponsiveDialog>
      <BarcodeScanner open={scan} onOpenChange={setScan} onDetected={setBarcode} />
    </>
  );
}
