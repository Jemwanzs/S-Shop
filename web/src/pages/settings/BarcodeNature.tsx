/** Roadmap 169: Product Barcode Nature (Settings → Products → Barcode & Quantity Tracking). How every product is tracked
 * now, and changing several at once: find and select products, preview what would happen to each (will change, already
 * that way, or cannot change yet with the reason — the same safeguards as Products → Edit), then confirm. Each product
 * goes through the ordinary product edit (approval when the business asks for it); barcodes, stock and history stay. */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ListChecks } from "lucide-react";
import { api } from "@/lib/api";
import { count } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import type { Paged, Product } from "@/lib/types";
import { useSession } from "@/lib/session";
import { ActionButton } from "@/components/ActionButton";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Pill, type Tone } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { SearchInput, Segments } from "@/components/Filters";
import { Loading } from "@/components/Page";

type Filter = "individual" | "shared" | "all";
interface PreviewItem { id: string; name: string | null; status: "eligible" | "already" | "review" | "blocked" | "not_found"; reasons?: string[]; notes?: string[] }
interface Result {
  summary: { selected: number; changed: number; pending_approval: number; already: number; review: number; blocked: number; failed: number };
  items: { id: string; name: string | null; outcome: string; reasons?: string[]; message?: string }[];
}

const STATE: Record<string, [Tone, string]> = {
  eligible: ["success", "Ready to convert"],
  changed: ["success", "Changed"],
  pending_approval: ["warning", "Waiting for approval"],
  already: ["neutral", "Already this way"],
  review: ["warning", "Conversion requires review"],
  blocked: ["danger", "Blocked"],
  not_found: ["neutral", "Not found"],
  failed: ["danger", "Failed"],
};

const LIMIT = 50;

/** The two counts and the way in. */
export function BarcodeNatureSummary() {
  const { can } = useSession();
  const [open, setOpen] = useState(false);
  const summary = useQuery({ queryKey: ["products-tracking-summary"], queryFn: () => api<{ shared: number; individual: number }>("/products/tracking-summary") });
  return (
    <div className="mt-3 border-t pt-3">
      <p className="text-sm font-medium">{t("Product Barcode Nature")}</p>
      <p className="text-xs text-muted-foreground">{t("How your products are tracked now.")}</p>
      <div className="mt-2 grid grid-cols-2 gap-2">
        <div className="rounded-lg bg-muted/60 p-2.5">
          <p className="num text-xl font-bold">{summary.data ? count(summary.data.individual) : "…"}</p>
          <p className="text-xs font-medium">{t("Individual Items")}</p>
          <p className="text-[11px] text-muted-foreground">{t("Each unit has its own barcode")}</p>
        </div>
        <div className="rounded-lg bg-muted/60 p-2.5">
          <p className="num text-xl font-bold">{summary.data ? count(summary.data.shared) : "…"}</p>
          <p className="text-xs font-medium">{t("Product Barcode (Shared)")}</p>
          <p className="text-[11px] text-muted-foreground">{t("Units share one barcode")}</p>
        </div>
      </div>
      {can("products.edit") && <Button size="sm" variant="outline" className="mt-2" onClick={() => setOpen(true)}><ListChecks /> {t("Review and change products")}</Button>}
      {open && <NatureDialog onClose={() => setOpen(false)} />}
    </div>
  );
}

function NatureDialog({ onClose }: { onClose: () => void }) {
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<Filter>("individual");
  const [offset, setOffset] = useState(0);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [target, setTarget] = useState<boolean | null>(null); // individual?
  const [preview, setPreview] = useState<{ items: PreviewItem[]; needs_approval: boolean } | null>(null);
  const [result, setResult] = useState<Result | null>(null);
  useEffect(() => { setOffset(0); }, [q, filter]);
  const query = { q: q.trim() || undefined, status: "all", tracking: filter };
  const list = useQuery({ queryKey: ["nature-list", q, filter, offset], queryFn: () => api<Paged<Product>>("/products", { query: { ...query, limit: LIMIT, offset } }) });
  const toggle = (id: string) => setPicked((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const selectAll = async () => {
    try {
      const r = await api<{ ids: string[]; total: number; capped: boolean }>("/products/matching-ids", { query });
      setPicked(new Set(r.ids));
      if (r.capped) toast.warning(`${t("Only the first")} ${count(r.ids.length)} ${t("of")} ${count(r.total)} ${t("can be changed at once")}`);
    } catch (e) {
      toast.error(e);
    }
  };
  const previewFor = async (individual: boolean) => {
    try {
      setTarget(individual);
      setPreview(await api("/products/tracking/preview", { body: { ids: [...picked], individual } }));
    } catch (e) {
      toast.error(e);
      setTarget(null);
    }
  };
  const apply = useMutation({
    mutationFn: () => api<Result>("/products/tracking/apply", { body: { ids: preview!.items.filter((i) => i.status === "eligible").map((i) => i.id), individual: target } }),
    onSuccess: (r) => {
      setResult(r);
      qc.invalidateQueries({ queryKey: ["products-tracking-summary"] });
      qc.invalidateQueries({ queryKey: ["nature-list"] });
      qc.invalidateQueries({ queryKey: ["products"] });
      toast.success(`${count(r.summary.changed)} ${t("product(s) changed")}${r.summary.pending_approval ? ` · ${count(r.summary.pending_approval)} ${t("waiting for approval")}` : ""}`);
    },
    onError: (e) => toast.error(e),
  });
  const eligible = preview?.items.filter((i) => i.status === "eligible").length ?? 0;
  const targetName = target ? "Individual Items" : "Product Barcode (Shared)";

  // ── Step 3: what happened
  if (result) {
    return (
      <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Product Barcode Nature" confirmDiscard={false} wide
        footer={<Button onClick={onClose}>{t("Done")}</Button>}>
        <p className="mb-2 flex items-center gap-2 text-sm"><CheckCircle2 className="h-4 w-4 text-success" />
          {count(result.summary.changed)} {t("changed")} · {count(result.summary.pending_approval)} {t("waiting for approval")} · {count(result.summary.review)} {t("require review")} · {count(result.summary.blocked)} {t("blocked")} · {count(result.summary.already)} {t("already this way")}</p>
        <Outcomes items={result.items.map((i) => ({ id: i.id, name: i.name, status: i.outcome, reasons: i.reasons ?? (i.message ? [i.message] : undefined) }))} />
      </ResponsiveDialog>
    );
  }
  // ── Step 2: preview, then confirm
  if (preview && target !== null) {
    return (
      <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title={`${t("Switch to")} ${t(targetName)}`} confirmDiscard={false} wide
        description={t(target ? "Each unit will need its own barcode when stock is received and sold." : "Units of each product will share one barcode, and quantities are counted with scans, + and −. No product barcode has to be added now.")}
        footer={
          <div className="flex w-full flex-wrap justify-end gap-2">
            <Button variant="ghost" onClick={() => { setPreview(null); setTarget(null); }}>{t("Back")}</Button>
            <ActionButton online blockedBy={[!eligible && "Nothing can change"]} busy={apply.isPending} onAction={() => apply.mutateAsync()}>
              {t("Change")} {count(eligible)} {t(eligible === 1 ? "product" : "products")}
            </ActionButton>
          </div>
        }>
        {preview.needs_approval && <p className="mb-2 rounded-lg bg-warning/10 p-2.5 text-xs text-warning">{t("Product edits need approval in your business: each change waits for approval, and the product stays as it is until then.")}</p>}
        <p className="mb-2 text-xs text-muted-foreground">{t("Stock in hand stays: items with their own barcodes keep their count, and their labels stay on record and still scan to the product. Past sales are not changed. Products that require review or are blocked are left as they are, with the reason.")}</p>
        <p className="mb-2 text-xs font-medium">{count(eligible)} {t("ready to convert")} · {count(preview.items.filter((i) => i.status === "review").length)} {t("require review")} · {count(preview.items.filter((i) => i.status === "blocked").length)} {t("blocked")} · {count(preview.items.filter((i) => i.status === "already").length)} {t("already this way")}</p>
        <Outcomes items={preview.items.map((i) => ({ id: i.id, name: i.name, status: i.status, reasons: i.status === "blocked" || i.status === "review" ? i.reasons : i.notes }))} />
      </ResponsiveDialog>
    );
  }
  // ── Step 1: find and select
  const rows = list.data?.items ?? [];
  const total = list.data?.total ?? 0;
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Product Barcode Nature" confirmDiscard={false} wide
      description="Select products, then choose how they should be tracked. You see what will happen before anything changes."
      footer={
        <div className="flex w-full flex-wrap items-center justify-end gap-2">
          <span className="me-auto text-xs text-muted-foreground">{count(picked.size)} {t("selected")}</span>
          <ActionButton variant="outline" blockedBy={[!picked.size && "Select products"]} quietReason onAction={() => previewFor(true)}>{t("Switch to Individual Items")}</ActionButton>
          <ActionButton blockedBy={[!picked.size && "Select products"]} quietReason onAction={() => previewFor(false)}>{t("Switch to Product Barcode (Shared)")}</ActionButton>
        </div>
      }>
      <div className="space-y-3">
        <SearchInput value={q} onChange={setQ} placeholder="Search name, code or barcode" />
        <Segments value={filter} onChange={setFilter} options={[{ value: "individual", label: "Individual Items" }, { value: "shared", label: "Product Barcode (Shared)" }, { value: "all", label: "All" }]} />
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="text-muted-foreground">{count(total)} {t(total === 1 ? "product" : "products")}</span>
          {total > 0 && <Button size="sm" variant="ghost" className="h-7" onClick={selectAll}>{t("Select all")} {count(total)}</Button>}
          {picked.size > 0 && <Button size="sm" variant="ghost" className="h-7" onClick={() => setPicked(new Set())}>{t("Clear selection")}</Button>}
        </div>
        {list.isLoading ? <Loading /> : (
          <ul className="divide-y rounded-lg border">
            {rows.map((p) => (
              <li key={p.id}>
                <label className="flex cursor-pointer items-center gap-3 px-3 py-2.5 text-sm">
                  <Checkbox checked={picked.has(p.id)} onCheckedChange={() => toggle(p.id)} aria-label={p.name} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{p.name}</span>
                    <span className="block truncate text-xs text-muted-foreground">{p.code}{p.barcode ? ` · ${p.barcode}` : ""}{p.is_active ? "" : ` · ${t("Inactive")}`}</span>
                  </span>
                  <Pill tone={p.track_items ? "info" : "neutral"}>{t(p.track_items ? "Individual Items" : "Product Barcode (Shared)")}</Pill>
                </label>
              </li>
            ))}
            {!rows.length && <li className="px-3 py-6 text-center text-sm text-muted-foreground">{t("No products here")}</li>}
          </ul>
        )}
        {total > LIMIT && (
          <div className="flex items-center justify-between text-xs">
            <Button size="sm" variant="ghost" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - LIMIT))}>{t("Previous")}</Button>
            <span className="text-muted-foreground">{count(offset + 1)}–{count(Math.min(total, offset + LIMIT))} {t("of")} {count(total)}</span>
            <Button size="sm" variant="ghost" disabled={offset + LIMIT >= total} onClick={() => setOffset(offset + LIMIT)}>{t("Next")}</Button>
          </div>
        )}
      </div>
    </ResponsiveDialog>
  );
}

function Outcomes({ items }: { items: { id: string; name: string | null; status: string; reasons?: string[] }[] }) {
  const order = ["blocked", "review", "failed", "eligible", "changed", "pending_approval", "already", "not_found"];
  const sorted = [...items].sort((a, b) => order.indexOf(a.status) - order.indexOf(b.status));
  return (
    <ul className="max-h-[50vh] divide-y overflow-y-auto rounded-lg border" data-testid="nature-outcomes">
      {sorted.map((i) => {
        const [tone, label] = STATE[i.status] ?? ["neutral", i.status];
        return (
          <li key={i.id} className="px-3 py-2 text-sm">
            <div className="flex items-center justify-between gap-2">
              <span className="min-w-0 truncate">{i.name ?? "—"}</span>
              <Pill tone={tone}>{t(label)}</Pill>
            </div>
            {!!i.reasons?.length && <ul className={cn("mt-1 space-y-0.5 text-xs", i.status === "blocked" ? "text-destructive" : i.status === "review" ? "text-warning" : "text-muted-foreground")}>{i.reasons.map((r) => <li key={r}>{t(r)}</li>)}</ul>}
          </li>
        );
      })}
    </ul>
  );
}
