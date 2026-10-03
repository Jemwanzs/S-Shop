import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ScanLine } from "lucide-react";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count } from "@/lib/format";
import type { Outcome, Paged, Product } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Field, NativeSelect } from "@/components/Form";
import { SearchInput } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { BarcodeScanner } from "@/components/BarcodeScanner";

const KINDS = [
  ["damage", "Damaged"],
  ["loss", "Lost / missing"],
  ["write_off", "Write-off"],
  ["customer_return", "Customer return (back to stock)"],
  ["supplier_return", "Returned to supplier"],
  ["count", "Recount (set exact quantity)"],
  ["manual", "Manual correction (+/−)"],
] as const;

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
  const [reason, setReason] = useState("");
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
    onError: (e) => toast.error(errorMessage(e)),
  });
  const valid = current && reason.trim() && (tracked ? barcode.trim() : qty !== "" && (kind === "manual" ? parseInt(qty) !== 0 : parseInt(qty) >= 0));

  return (
    <>
      <ResponsiveDialog
        open={open}
        onOpenChange={(o) => { if (!o) reset(); onOpenChange(o); }}
        title="Adjust stock"
        description="Sensitive adjustments may need approval."
        footer={<Button className="w-full md:w-auto" disabled={!valid || save.isPending} onClick={() => save.mutate()}>Save adjustment</Button>}
      >
        <div className="space-y-4">
          {current ? (
            <div className="flex items-center justify-between rounded-xl bg-muted/60 p-3">
              <div>
                <div className="font-medium">{current.name}</div>
                <div className="num text-xs text-muted-foreground">{count(current.on_hand)} on hand · {count(current.reserved)} reserved</div>
              </div>
              {!initial && <Button variant="ghost" size="sm" onClick={() => setProduct(null)}>Change</Button>}
            </div>
          ) : (
            <div className="space-y-2">
              <SearchInput value={q} onChange={setQ} placeholder="Find product" autoFocus />
              <ul className="divide-y rounded-lg border">
                {results.data?.items.map((p) => (
                  <li key={p.id}><button className="flex w-full justify-between px-3 py-2.5 text-left text-sm hover:bg-accent" onClick={() => setProduct(p)}><span>{p.name}</span><span className="num text-muted-foreground">{count(p.on_hand)}</span></button></li>
                ))}
              </ul>
            </div>
          )}
          <Field label="Type">
            <NativeSelect value={kind} onChange={setKind}>
              {kinds.map(([k, label]) => <option key={k} value={k}>{label}</option>)}
            </NativeSelect>
          </Field>
          {tracked ? (
            <Field label="Item barcode">
              <div className="flex gap-2">
                <Input className="num" value={barcode} onChange={(e) => setBarcode(e.target.value)} />
                <Button variant="outline" onClick={() => setScan(true)}><ScanLine /> Scan</Button>
              </div>
            </Field>
          ) : (
            <Field label={kind === "count" ? "Counted quantity" : kind === "manual" ? "Change (use − to remove)" : "Quantity"}>
              <Input inputMode={kind === "manual" ? "text" : "numeric"} className="num" value={qty} onChange={(e) => setQty(e.target.value.replace(kind === "manual" ? /[^\d-]/g : /\D/g, ""))} />
            </Field>
          )}
          <Field label="Reason"><Textarea value={reason} onChange={(e) => setReason(e.target.value)} placeholder="What happened?" /></Field>
        </div>
      </ResponsiveDialog>
      <BarcodeScanner open={scan} onOpenChange={setScan} onDetected={setBarcode} />
    </>
  );
}
