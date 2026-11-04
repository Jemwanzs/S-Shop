import { useMemo, useRef, useState } from "react";
import { useNavigate } from "@/lib/router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, ClipboardCheck, Eraser, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { usePersistentState } from "@/lib/hooks";
import { count, signed, maskCode } from "@/lib/format";
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
import type { ScanCountLine } from "@/components/ScanCounter";

interface Lookup { product: Product; stock_item: { barcode: string; branch_name: string; status: string; in_current_branch: boolean } | null }
/** The stock-take session, kept on this device per branch until submitted or cleared (roadmap 98): counted
 * quantities, products identified by barcode, and the unit barcodes found for individually tracked products. */
interface CountSession {
  counts: Record<string, string>;
  scanned: string[];
  units: Record<string, string[]>;
  /** Roadmap 153: the shared barcode each scanned product was read by, for the counter. */
  codes?: Record<string, string>;
  /** Roadmap 153: labels read that do not belong in this count — never counted, listed for checking. */
  flags?: { code: string; title: string; detail?: string }[];
}
const empty: CountSession = { counts: {}, scanned: [], units: {} };
interface Compare { expected: number; found: number; missing: string[]; unexpected: { barcode: string; kind: string; product?: string; status?: string; branch?: string }[] }

/** Physical stock take: count by barcode (scanned or typed), by product, or unit by unit for tracked products.
 * Nothing changes live stock until submitted; variances become count / loss adjustments (with approval if configured). */
export default function StockCount() {
  const { branch, profile } = useSession();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const barcodes = useBarcodeSettings();
  const requireScan = barcodes.enabled && !!profile?.settings.stock.require_barcode_stocktake;
  const [session, setSession] = usePersistentState<CountSession>(`sshop.stocktake.${profile?.tenant.id}.${profile?.user.id}.${branch?.id}`, empty, `sshop.stocktake.${branch?.id}`);
  // Roadmap 153: the next scan must see this scan's count even before the screen updates.
  const sessionRef = useRef(session);
  sessionRef.current = session;
  const change = (fn: (s: CountSession) => CountSession) => { sessionRef.current = fn(sessionRef.current); setSession(sessionRef.current); };
  const [lastScanned, setLastScanned] = useState<string | null>(null);
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
  const flag = (code: string, title: string, detail?: string) => change((c) => ({ ...c, flags: [...(c.flags ?? []).filter((f) => f.code !== code), { code, title, detail }] }));
  const onCode = async (code: string): Promise<ScanOutcome> => {
    let r: Lookup;
    try {
      r = await api<Lookup>("/products/lookup", { query: { code } });
    } catch (e) {
      const unknown = e instanceof ApiError && e.status === 404;
      if (unknown) flag(code, "Unknown barcode");
      return { tone: "error", title: unknown ? `Unknown barcode ${code}` : errorMessage(e) };
    }
    const p = r.product;
    if (!data?.items.some((x) => x.product_id === p.id)) return { tone: "error", title: `${p.name} is not on this branch's count sheet` };
    if (p.track_items) {
      const unit = r.stock_item;
      if (!unit) return { tone: "error", title: `${p.name} is tracked per item`, detail: t("Scan each unit's own label, not the product barcode") };
      if (!unit.in_current_branch || unit.status !== "in_stock") {
        const misplaced = unit.status === "in_stock";
        const detail = `${p.name} · ${unit.status.replace("_", " ")} · ${unit.branch_name}`;
        flag(code, misplaced ? "Misplaced — belongs to another branch" : "Unexpected — not in stock", detail);
        return { tone: "error", title: `${maskCode(code)} is not in stock here`, detail };
      }
      const have = sessionRef.current.units[p.id] ?? [];
      setLastScanned(code);
      if (have.includes(code)) return { tone: "warning", title: "Already scanned — 1 unit captured", detail: `${p.name} · ${maskCode(code)}` };
      change((s) => ({ ...s, units: { ...s.units, [p.id]: [...(s.units[p.id] ?? []), code] } }));
      return { tone: "success", title: `${p.name} · ${have.length + 1} counted` };
    }
    const next = (parseInt(sessionRef.current.counts[p.id] ?? "") || 0) + 1;
    change((s) => ({
      ...s,
      counts: { ...s.counts, [p.id]: String(next) },
      scanned: s.scanned.includes(p.id) ? s.scanned : [...s.scanned, p.id],
      codes: { ...s.codes, [p.id]: code },
    }));
    setLastScanned(p.id);
    return { tone: "success", title: `${p.name} · counted ${next}` };
  };

  // Roadmap 153: the scanning session — what was read, with its corrections. A correction changes the count only; it never
  // creates or removes a physical barcode record.
  const nameOf = (id: string) => data?.items.find((x) => x.product_id === id)?.name ?? "";
  const scanSession = {
    lines: [
      ...session.scanned.filter((id) => session.counts[id] !== undefined && session.counts[id] !== "").map((id): ScanCountLine => ({ key: id, name: nameOf(id), barcode: session.codes?.[id], kind: "shared", quantity: parseInt(session.counts[id]) || 0, min: 0 })),
      ...Object.entries(session.units).flatMap(([id, codes]) => codes.map((c): ScanCountLine => ({ key: c, name: nameOf(id), barcode: maskCode(c), kind: "unique", quantity: 1 }))),
    ],
    activeKey: lastScanned,
    onSet: (key: string, quantity: number) => change((s) => ({ ...s, counts: { ...s.counts, [key]: String(quantity) } })),
    onRemove: (key: string) => change((s) => {
      if (s.scanned.includes(key)) {
        const counts = { ...s.counts };
        delete counts[key];
        return { ...s, counts, scanned: s.scanned.filter((x) => x !== key) };
      }
      return { ...s, units: Object.fromEntries(Object.entries(s.units).map(([id, codes]) => [id, codes.filter((c) => c !== key)])) };
    }),
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
        actions={lines > 0 && <Button variant="outline" onClick={() => { if (window.confirm(t("Clear this count?"))) change(() => empty); }}><Eraser /> {t("Clear")}</Button>} />
      <div className="mb-4 grid gap-3 md:grid-cols-[1fr_1fr_280px]">
        {barcodes.enabled && <BarcodeInput onCode={onCode} autoFocus cameraTitle="Scan to count" session={scanSession} />}
        <SearchInput value={q} onChange={setQ} placeholder="Filter products" />
        <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Reason for the count" aria-label="Reason" />
      </div>
      {requireScan && <p className="mb-3 text-xs text-muted-foreground">{t("Products with a barcode are counted by scanning or entering it; their quantity can be corrected afterwards.")}</p>}
      {isLoading ? (
        <Loading />
      ) : (
        <div className="space-y-4 pb-24">
          {!!session.flags?.length && (
            <div className="surface divide-y border-warning/40 pb-2">
              <div className="flex items-center gap-2 px-4 py-3">
                <AlertTriangle className="h-4 w-4 text-warning" />
                <p className="font-medium">{t("Labels to check")} <span className="num text-muted-foreground">· {session.flags.length}</span></p>
              </div>
              {session.flags.map((f) => (
                <div key={f.code} className="flex items-center justify-between gap-3 px-4 py-2 text-sm">
                  <div className="min-w-0">
                    <div className="font-medium">{t(f.title)} <span className="num text-xs text-muted-foreground">{maskCode(f.code)}</span></div>
                    {f.detail && <div className="truncate text-xs text-muted-foreground">{f.detail}</div>}
                  </div>
                  <button className="rounded p-1.5 text-muted-foreground hover:bg-muted" aria-label={t("Remove")} onClick={() => change((s) => ({ ...s, flags: s.flags?.filter((x) => x.code !== f.code) }))}><X className="h-3.5 w-3.5" /></button>
                </div>
              ))}
            </div>
          )}
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
                    onChange={(e) => change((s) => ({ ...s, counts: { ...s.counts, [r.product_id]: e.target.value.replace(/\D/g, "") } }))} />
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
                      {!!units?.length && <div className="num mt-1 truncate text-[11px] text-muted-foreground">{units.map(maskCode).join(" · ")}</div>}
                      {units && <UnitCheck productId={r.product_id} units={units} />}
                    </div>
                    {units
                      ? <Button size="sm" variant="ghost" onClick={() => change((s) => { const u = { ...s.units }; delete u[r.product_id]; return { ...s, units: u }; })}>{t("Reset")}</Button>
                      : <Button size="sm" variant="outline" title={t("None of its units is on the shelf")} onClick={() => change((s) => ({ ...s, units: { ...s.units, [r.product_id]: [] } }))}>{t("None found")}</Button>}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
      <div className="fixed inset-x-0 bottom-above-nav z-20 border-t bg-background/95 p-3 backdrop-blur-sm lg:bottom-0 lg:inset-s-sidebar">
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

/** Roadmap 153: the units scanned against the units expected here — which are missing, and any that do not belong. */
function UnitCheck({ productId, units }: { productId: string; units: string[] }) {
  const { data } = useQuery({
    queryKey: ["stock", "count-compare", productId, units.join(",")],
    queryFn: () => api<Compare>("/stock/count/compare", { body: { product_id: productId, barcodes: units } }),
    placeholderData: (p) => p,
  });
  if (!data || (!data.missing.length && !data.unexpected.length)) return null;
  return (
    <div className="mt-1 space-y-0.5 text-[11px]">
      {data.missing.length > 0 && <p className="text-warning"><span className="font-semibold">{t("Missing")} ({data.missing.length}):</span> <span className="num">{data.missing.slice(0, 12).join(" · ")}{data.missing.length > 12 ? " …" : ""}</span></p>}
      {data.unexpected.map((u) => <p key={u.barcode} className="text-destructive"><span className="num">{u.barcode}</span> · {t(u.kind === "misplaced" ? "Misplaced — belongs to another branch" : u.kind === "unknown" ? "Unknown barcode" : "Unexpected — not in stock")}{u.branch ? ` · ${u.branch}` : ""}</p>)}
    </div>
  );
}
