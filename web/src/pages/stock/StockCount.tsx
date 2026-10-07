import { useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ClipboardCheck, ScanLine } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { count, signed } from "@/lib/format";
import type { Paged, StockLevel } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Loading, PageHeader } from "@/components/Page";
import { SearchInput } from "@/components/Filters";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";
import { t } from "@/lib/i18n";

/** Physical stock take: enter counted quantities; variances become count adjustments (with approval if configured). */
export default function StockCount() {
  const { branch } = useSession();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [counts, setCounts] = useState<Record<string, string>>({});
  const [q, setQ] = useState("");
  const [reason, setReason] = useState("Stock take");
  const [scan, setScan] = useState(false);
  const countsRef = useRef(counts);
  countsRef.current = counts;
  const { data, isLoading } = useQuery({
    queryKey: ["stock", "count-sheet"],
    queryFn: () => api<Paged<StockLevel>>("/stock", { query: { limit: 500 } }),
  });
  const rows = useMemo(
    () => (data?.items ?? []).filter((r) => !r.track_items && r.is_active && (!q || r.name.toLowerCase().includes(q.toLowerCase()) || r.code.toLowerCase().includes(q.toLowerCase()))),
    [data, q],
  );
  const entered = Object.entries(counts).filter(([, v]) => v !== "");
  const variances = entered.filter(([id, v]) => data?.items.find((r) => r.product_id === id)?.on_hand !== parseInt(v));

  // Scan each unit on the shelf: every read adds one to that product's count.
  const onScan = async (code: string): Promise<ScanOutcome> => {
    let product: { id: string; name: string; track_items: boolean };
    try {
      product = (await api<{ product: { id: string; name: string; track_items: boolean } }>("/products/lookup", { query: { code } })).product;
    } catch (e) {
      return { tone: "error", title: e instanceof ApiError && e.status === 404 ? "Barcode not found" : errorMessage(e) };
    }
    if (product.track_items) return { tone: "error", title: `${product.name} is tracked per item`, detail: "Check individual items from the Stock page (barcoded items)." };
    if (!data?.items.some((r) => r.product_id === product.id)) return { tone: "error", title: `${product.name} is not on this branch's count sheet` };
    const next = (parseInt(countsRef.current[product.id] ?? "") || 0) + 1;
    setCounts((c) => ({ ...c, [product.id]: String(next) }));
    return { tone: "success", title: `${product.name} · counted ${next}` };
  };

  const submit = useMutation({
    mutationFn: () => api<{ applied: number; pending_approval: number; unchanged: number }>("/stock/count", { body: { reason, lines: entered.map(([product_id, v]) => ({ product_id, counted: parseInt(v) })) } }),
    onSuccess: (r) => {
      toast.success(`Count saved: ${r.applied} adjusted, ${r.pending_approval} awaiting approval, ${r.unchanged} matched`);
      qc.invalidateQueries({ queryKey: ["stock"] });
      navigate("/stock?tab=adjustments");
    },
    onError: (e) => toast.error(e),
  });

  return (
    <>
      <PageHeader back="/stock" eyebrow={branch?.name} title="Stock take" description="Count what is physically on the shelf. Individually tracked items are adjusted by scanning from the Stock page." actions={<Button variant="ink" onClick={() => setScan(true)}><ScanLine /> {t("Scan to count")}</Button>} />
      <div className="mb-4 grid gap-3 md:grid-cols-[1fr_320px]">
        <SearchInput value={q} onChange={setQ} placeholder="Filter products" />
        <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Reason for the count" aria-label="Reason" />
      </div>
      {isLoading ? (
        <Loading />
      ) : (
        <div className="surface divide-y pb-2">
          <div className="hidden grid-cols-[1fr_100px_120px_100px] gap-3 px-4 py-3 md:grid">
            <span className="label-caps">{t("Product")}</span><span className="label-caps text-end">{t("System")}</span><span className="label-caps text-end">{t("Counted")}</span><span className="label-caps text-end">{t("Variance")}</span>
          </div>
          {rows.map((r) => {
            const v = counts[r.product_id] ?? "";
            const variance = v === "" ? null : parseInt(v) - r.on_hand;
            return (
              <div key={r.product_id} className="grid grid-cols-[1fr_96px] items-center gap-3 px-4 py-2.5 md:grid-cols-[1fr_100px_120px_100px]">
                <div className="min-w-0">
                  <div className="truncate font-medium">{r.name}</div>
                  <div className="num text-xs text-muted-foreground md:hidden">{t("System")} {count(r.on_hand)}{variance !== null && variance !== 0 && <span className={variance > 0 ? "text-success" : "text-destructive"}> · {signed(variance)}</span>}</div>
                </div>
                <span className="num hidden text-end md:block">{count(r.on_hand)}</span>
                <Input inputMode="numeric" className="num text-center" value={v} placeholder="—" onChange={(e) => setCounts({ ...counts, [r.product_id]: e.target.value.replace(/\D/g, "") })} />
                <span className={cn("num hidden text-end font-semibold md:block", variance && variance > 0 && "text-success", variance && variance < 0 && "text-destructive")}>{variance === null ? "" : variance === 0 ? "✓" : signed(variance)}</span>
              </div>
            );
          })}
        </div>
      )}
      <BarcodeScanner open={scan} onOpenChange={setScan} onDetected={onScan} continuous title="Scan to count" hint="Each read adds one" />
      <div className="fixed inset-x-0 bottom-above-nav z-20 border-t bg-background/95 p-3 backdrop-blur lg:bottom-0 lg:start-sidebar">
        <div className="mx-auto flex max-w-[1680px] items-center justify-between gap-3 px-1 md:px-3 lg:px-5">
          <span className="text-sm text-muted-foreground"><span className="num font-semibold text-foreground">{entered.length}</span> counted · <span className="num font-semibold text-foreground">{variances.length}</span> variances</span>
          <ActionButton online busy={submit.isPending} busyLabel="Submitting…" blockedBy={[!entered.length && "Nothing counted"]} onAction={() => submit.mutateAsync()}>
            <ClipboardCheck /> {t("Submit count")}
          </ActionButton>
        </div>
      </div>
    </>
  );
}
