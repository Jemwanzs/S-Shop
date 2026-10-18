/** Item barcodes on products and stock (roadmap 135–136): print / reprint labels, replace a damaged label, reveal one
 * full barcode, label stock that has none yet. Full codes come only from audited server actions; screens show `****047`. */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye, Printer, RefreshCcw, Tags } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { count } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import type { Paged } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";
import { LabelsDialog } from "@/components/LabelsDialog";
import type { Label } from "@/lib/labels";

interface Sheet { business: string; labels: Label[]; title: string }

/** Reprint, replace or reveal one unit's barcode. */
export function ItemBarcodeActions({ itemId, status, onChanged }: { itemId: string; status: string; onChanged?: () => void }) {
  const { can } = useSession();
  const [sheet, setSheet] = useState<Sheet | null>(null);
  const [replacing, setReplacing] = useState(false);
  const [reason, setReason] = useState("");
  const [revealed, setRevealed] = useState<string | null>(null);
  const live = ["in_stock", "in_transit"].includes(status);
  if (!can("barcodes.print") && !can("barcodes.replace") && !can("barcodes.reveal")) return null;
  return (
    <div className="flex flex-wrap gap-2">
      {live && can("barcodes.print") && (
        <ActionButton size="sm" variant="outline" online onAction={async () => {
          const r = await api<{ business: string; labels: Label[] }>("/barcodes/labels", { body: { stock_item_ids: [itemId] } });
          setSheet({ ...r, title: "Reprint label" });
        }}><Printer /> {t("Reprint label")}</ActionButton>
      )}
      {status === "in_stock" && can("barcodes.replace") && (
        <Button size="sm" variant="outline" onClick={() => { setReason(""); setReplacing(true); }}><RefreshCcw /> {t("Replace label")}</Button>
      )}
      {can("barcodes.reveal") && (
        revealed
          ? <span className="num rounded-md bg-warning/10 px-2 py-1 text-sm font-semibold text-warning">{revealed}</span>
          : <ActionButton size="sm" variant="ghost" online onAction={async () => setRevealed((await api<{ code: string }>("/barcodes/reveal", { body: { stock_item_id: itemId } })).code)}>
              <Eye /> {t("Reveal")}
            </ActionButton>
      )}
      <ResponsiveDialog open={replacing} onOpenChange={setReplacing} title="Replace label"
        description="The item gets a new seven-digit barcode; the old one stops working and is never reused."
        footer={<ActionButton online blockedBy={[!reason.trim() && "Say why the label is being replaced"]} onAction={async () => {
          const r = await api<{ code: string; business: string; product: string }>("/barcodes/replace", { body: { stock_item_id: itemId, reason } });
          setReplacing(false);
          toast.success("New barcode issued — print and attach the new label");
          setSheet({ business: r.business, labels: [{ code: r.code, product: r.product }], title: "New label" });
          onChanged?.();
        }}>{t("Replace")}</ActionButton>}>
        <Field label="Reason" hint="e.g. label torn, unreadable, lost"><Textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} /></Field>
      </ResponsiveDialog>
      {sheet && <LabelsDialog business={sheet.business} labels={sheet.labels} title={sheet.title} onClose={() => setSheet(null)} />}
    </div>
  );
}

/** Product details → Barcode labels: print the labels of every unit in stock at a branch; label stock that has none. */
export function ProductBarcodeLabels({ productId, branchId, branchName, onHand }: { productId: string; branchId: string; branchName: string; onHand: number }) {
  const { can } = useSession();
  const qc = useQueryClient();
  const [sheet, setSheet] = useState<Sheet | null>(null);
  const units = useQuery({
    queryKey: ["stock", "items", "count", productId, branchId],
    queryFn: () => api<Paged<unknown>>("/stock/items", { query: { product_id: productId, branch_id: branchId, status: "in_stock", limit: 1 } }),
  });
  const labelled = units.data?.total ?? 0;
  const missing = Math.max(0, onHand - labelled);
  if (!can("barcodes.print") && !can("barcodes.generate")) return null;
  return (
    <div className="space-y-2 text-sm">
      <p className="text-muted-foreground">{count(labelled)} {t("items with their own barcode in stock at")} {branchName}</p>
      {missing > 0 && (
        <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">
          {count(missing)} {t("item(s) on hand have no barcode yet (stock from before individual tracking). Label them to sell them.")}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        {can("barcodes.print") && labelled > 0 && (
          <ActionButton size="sm" variant="outline" online onAction={async () => {
            const r = await api<{ business: string; labels: Label[] }>("/barcodes/labels", { body: { product_id: productId, branch_id: branchId } });
            setSheet({ ...r, title: "Print labels" });
          }}><Printer /> {t("Print labels")} ({count(labelled)})</ActionButton>
        )}
        {missing > 0 && can("barcodes.generate") && can("stock.add") && (
          <ActionButton size="sm" online onAction={async () => {
            const r = await api<{ codes: string[]; label: { business: string; product: string } }>("/barcodes/label-existing", { body: { product_id: productId, branch_id: branchId } });
            toast.success(`${r.codes.length} ${t("barcodes generated — print and attach the labels")}`);
            qc.invalidateQueries({ queryKey: ["stock", "items"] });
            setSheet({ business: r.label.business, labels: r.codes.map((code) => ({ code, product: r.label.product })), title: "Label existing stock" });
          }}><Tags /> {t("Label existing stock")} ({count(missing)})</ActionButton>
        )}
      </div>
      {sheet && <LabelsDialog business={sheet.business} labels={sheet.labels} title={sheet.title} note={sheet.title === "Label existing stock" ? "Attach one label to each item on the shelf" : undefined} onClose={() => setSheet(null)} />}
    </div>
  );
}
