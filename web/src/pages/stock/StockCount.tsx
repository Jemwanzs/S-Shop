import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ClipboardCheck, Eraser } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { usePersistentState } from "@/lib/hooks";
import { count, signed } from "@/lib/format";
import type { Paged, Product, StockLevel } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Loading, PageHeader } from "@/components/Page";
import { SearchInput } from "@/components/Filters";
import type { ScanOutcome } from "@/components/BarcodeScanner";
import { BarcodeInput, useBarcodeSettings } from "@/components/BarcodeInput";
import { t } from "@/lib/i18n";

interface Lookup { product: Product; stock_item: { barcode: string; branch_name: string; status: string; in_current_branch: boolean } | null }
/** The stock-take session, kept on this device per branch until submitted or cleared (roadmap 98): counted
 * quantities, products identified by barcode, and the unit barcodes found for individually tracked products. */
interface CountSession { counts: Record<string, string>; scanned: string[]; units: Record<string, string[]> }
const empty: CountSession = { counts: {}, scanned: [], units: {} };

/** Physical stock take: count by barcode (scanned or typed), by product, or unit by unit for tracked products.
 * Nothing changes live stock until submitted; variances become count / loss adjustments (with approval if configured). */
export default function StockCount() {
  const { branch, profile } = useSession();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const barcodes = useBarcodeSettings();
  const requireScan = barcodes.enabled && !!profile?.settings.stock.require_barcode_stocktake;
  const [session, setSession] = usePersistentState<CountSession>(`sshop.stocktake.${branch?.id}`, empty);
  const [q, setQ] = useState("");
  const [reason, setReason] = useState("Stock take");
  const { data, isLoading } = useQuery({
    queryKey: ["stock", "count-sheet"],
    queryFn: () => api<Paged<StockLevel>>("/stock", { query: { limit: 500 } }),
  });
  const match = (r: StockLevel) => r.is_active && (!q || r.name.toLowerCase().includes(q.toLowerCase()) || r.code.toLowerCase().includes(q.toLowerCase()));
  const rows = useMemo(() => (data?.items ?? []).filter((r) => !r.track_items && match(r)), [data, q]); // eslint-disable-line react-hooks/exhaustive-deps
  const tracked = useMemo(() => (data?.items ?? []).filter((r) => r.track_items && match(r) && (r.on_hand > 0 || session.units[r.product_id]?.length)), [data, q, session.units]); // eslint-disable-line react-hooks/exhaustive-deps
  const entered = Object.entries(session.counts).filter(([, v]) => v !== "");
  const unitLines = Object.entries(session.units).filter(([, u]) => u !== undefined);
  const variances = entered.filter(([id, v]) => data?.items.find((r) => r.product_id === id)?.on_hand !== parseInt(v)).length
    + unitLines.filter(([id, u]) => data?.items.find((r) => r.product_id === id)?.on_hand !== u.length).length;

  // A barcode (scanned or typed): a product-level barcode adds one to that product's count; a unit barcode counts that
  // exact unit once. Live stock is not touched.
  const onCode = async (code: string): Promise<ScanOutcome> => {
    let r: Lookup;
    try {
      r = await api<Lookup>("/products/lookup", { query: { code } });
    } catch (e) {
      return { tone: "error", title: e instanceof ApiError && e.status === 404 ? `Unknown barcode ${code}` : errorMessage(e) };
    }
    const p = r.product;
    if (!data?.items.some((x) => x.product_id === p.id)) return { tone: "error", title: `${p.name} is not on this branch's count sheet` };
    if (p.track_items) {
      const unit = r.stock_item;
      if (!unit) return { tone: "error", title: `${p.name} is tracked per item`, detail: t("Scan each unit's own label, not the product barcode") };
      if (!unit.in_current_branch || unit.status !== "in_stock") return { tone: "error", title: `${unit.barcode} is not in stock here`, detail: `${unit.status.replace("_", " ")} · ${unit.branch_name}` };
      const have = session.units[p.id] ?? [];
      if (have.includes(unit.barcode)) return { tone: "info", title: `${unit.barcode} already counted`, detail: p.name };
      setSession((s) => ({ ...s, units: { ...s.units, [p.id]: [...(s.units[p.id] ?? []), unit.barcode] } }));
      return { tone: "success", title: `${p.name} · ${have.length + 1} counted` };
    }
    let next = 0;
    setSession((s) => {
      next = (parseInt(s.counts[p.id] ?? "") || 0) + 1;
      return { ...s, counts: { ...s.counts, [p.id]: String(next) }, scanned: s.scanned.includes(p.id) ? s.scanned : [...s.scanned, p.id] };
    });
    return { tone: "success", title: `${p.name} · counted ${(parseInt(session.counts[p.id] ?? "") || 0) + 1}` };
  };

  const submit = useMutation({
    mutationFn: () => api<{ applied: number; pending_approval: number; unchanged: number }>("/stock/count", {
      body: {
        reason,
        lines: [
          ...entered.map(([product_id, v]) => ({ product_id, counted: parseInt(v), scanned: session.scanned.includes(product_id) })),
          ...unitLines.map(([product_id, units]) => ({ product_id, barcodes: units })),
        ],
      },
    }),
    onSuccess: (r) => {
      toast.success(`Count saved: ${r.applied} adjusted, ${r.pending_approval} awaiting approval, ${r.unchanged} matched`);
      setSession(empty);
      qc.invalidateQueries({ queryKey: ["stock"] });
      navigate("/stock?tab=adjustments");
    },
    onError: (e) => toast.error(e),
  });

  const lines = entered.length + unitLines.length;
  return (
    <>
      <PageHeader back="/stock" eyebrow={branch?.name} title="Stock take"
        description="Count what is physically on the shelf. Nothing changes until you submit; differences go through the usual adjustment approvals."
        actions={lines > 0 && <Button variant="outline" onClick={() => { if (window.confirm(t("Clear this count?"))) setSession(empty); }}><Eraser /> {t("Clear")}</Button>} />
      <div className="mb-4 grid gap-3 md:grid-cols-[1fr_1fr_280px]">
        {barcodes.enabled && <BarcodeInput onCode={onCode} autoFocus cameraTitle="Scan to count" />}
        <SearchInput value={q} onChange={setQ} placeholder="Filter products" />
        <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Reason for the count" aria-label="Reason" />
      </div>
      {requireScan && <p className="mb-3 text-xs text-muted-foreground">{t("Products with a barcode are counted by scanning or entering it; their quantity can be corrected afterwards.")}</p>}
      {isLoading ? (
        <Loading />
      ) : (
        <div className="space-y-4 pb-24">
          <div className="surface divide-y pb-2">
            <div className="hidden grid-cols-[1fr_100px_120px_100px] gap-3 px-4 py-3 md:grid">
              <span className="label-caps">{t("Product")}</span><span className="label-caps text-end">{t("Expected")}</span><span className="label-caps text-end">{t("Counted")}</span><span className="label-caps text-end">{t("Variance")}</span>
            </div>
            {rows.map((r) => {
              const v = session.counts[r.product_id] ?? "";
              const variance = v === "" ? null : parseInt(v) - r.on_hand;
              const locked = requireScan && !!r.has_barcode && !session.scanned.includes(r.product_id);
              return (
                <div key={r.product_id} className="grid grid-cols-[1fr_96px] items-center gap-3 px-4 py-2.5 md:grid-cols-[1fr_100px_120px_100px]">
                  <div className="min-w-0">
                    <div className="truncate font-medium">{r.name}</div>
                    <div className="num text-xs text-muted-foreground md:hidden">{t("Expected")} {count(r.on_hand)}{variance !== null && variance !== 0 && <span className={variance > 0 ? "text-success" : "text-destructive"}> · {signed(variance)}</span>}</div>
                  </div>
                  <span className="num hidden text-end md:block">{count(r.on_hand)}</span>
                  <Input inputMode="numeric" className="num text-center" value={v} placeholder={locked ? t("Scan") : "—"} disabled={locked}
                    title={locked ? t("Scan or enter this product's barcode to count it") : undefined}
                    onChange={(e) => setSession((s) => ({ ...s, counts: { ...s.counts, [r.product_id]: e.target.value.replace(/\D/g, "") } }))} />
                  <span className={cn("num hidden text-end font-semibold md:block", variance && variance > 0 && "text-success", variance && variance < 0 && "text-destructive")}>{variance === null ? "" : variance === 0 ? "✓" : signed(variance)}</span>
                </div>
              );
            })}
          </div>
          {tracked.length > 0 && (
            <div className="surface divide-y pb-2">
              <div className="px-4 py-3">
                <p className="font-medium">{t("Individually tracked items")}</p>
                <p className="text-xs text-muted-foreground">{t("Scan each unit's label. Units not found are recorded as lost when you submit (approvals apply); a product you do not scan is left as it is.")}</p>
              </div>
              {tracked.map((r) => {
                const units = session.units[r.product_id];
                const variance = units ? units.length - r.on_hand : null;
                return (
                  <div key={r.product_id} className="grid grid-cols-[1fr_auto] items-center gap-3 px-4 py-2.5">
                    <div className="min-w-0">
                      <div className="truncate font-medium">{r.name}</div>
                      <div className="num text-xs text-muted-foreground">
                        {t("Expected")} {count(r.on_hand)} · {t("Counted")} {units ? count(units.length) : "—"}
                        {variance !== null && <span className={cn("font-semibold", variance < 0 ? "text-destructive" : "text-success")}> · {variance === 0 ? "✓" : signed(variance)}</span>}
                      </div>
                      {!!units?.length && <div className="num mt-1 truncate text-[11px] text-muted-foreground">{units.join(" · ")}</div>}
                    </div>
                    {units
                      ? <Button size="sm" variant="ghost" onClick={() => setSession((s) => { const u = { ...s.units }; delete u[r.product_id]; return { ...s, units: u }; })}>{t("Reset")}</Button>
                      : <Button size="sm" variant="outline" title={t("None of its units is on the shelf")} onClick={() => setSession((s) => ({ ...s, units: { ...s.units, [r.product_id]: [] } }))}>{t("None found")}</Button>}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
      <div className="fixed inset-x-0 bottom-above-nav z-20 border-t bg-background/95 p-3 backdrop-blur lg:bottom-0 lg:start-sidebar">
        <div className="mx-auto flex max-w-[1680px] items-center justify-between gap-3 px-1 md:px-3 lg:px-5">
          <span className="text-sm text-muted-foreground"><span className="num font-semibold text-foreground">{lines}</span> {t("counted")} · <span className="num font-semibold text-foreground">{variances}</span> {t("variances")}</span>
          <ActionButton online busy={submit.isPending} busyLabel="Submitting…" blockedBy={[!lines && "Nothing counted"]} onAction={() => submit.mutateAsync()}>
            <ClipboardCheck /> {t("Submit count")}
          </ActionButton>
        </div>
      </div>
    </>
  );
}
