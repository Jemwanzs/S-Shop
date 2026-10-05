import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowRight, Loader2, Minus, Plus, ScanLine, Trash2, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, todayIso } from "@/lib/format";
import type { Paged, StockLevel } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { PageHeader, Section } from "@/components/Page";
import { Field, NativeSelect } from "@/components/Form";
import { SearchInput } from "@/components/Filters";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";

interface Line {
  product: StockLevel;
  quantity: number;
  barcodes: string[];
}

export default function TransferNew() {
  const { profile, branch } = useSession();
  const navigate = useNavigate();
  const [to, setTo] = useState("");
  const [date, setDate] = useState(todayIso());
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<Line[]>([]);
  const [q, setQ] = useState("");
  const [scanFor, setScanFor] = useState<string | null>(null);
  const term = useDebounced(q);
  const branches = useQuery({ queryKey: ["branches"], queryFn: () => api<{ id: string; name: string; is_active: boolean }[]>("/branches") });
  const results = useQuery({ queryKey: ["stock", "transfer-pick", term], queryFn: () => api<Paged<StockLevel>>("/stock", { query: { q: term, status: "in", limit: 10 } }), enabled: !!term });
  const destinations = (branches.data ?? profile?.branches.map((b) => ({ ...b, is_active: true })) ?? []).filter((b) => b.is_active && b.id !== branch?.id);

  const update = (id: string, fn: (l: Line) => Line) => setLines((ls) => ls.map((l) => (l.product.product_id === id ? fn(l) : l)));

  // Scanning adds products (product barcode) or specific units (item labels), checked against this branch's stock.
  const onScan = async (code: string): Promise<ScanOutcome> => {
    let r: { product: { id: string; name: string; track_items: boolean; transfer_allowed: boolean; is_active: boolean; available: number }; stock_item: { status: string; in_current_branch: boolean; branch_name: string } | null };
    try {
      r = await api("/products/lookup", { query: { code } });
    } catch (e) {
      return { tone: "error", title: e instanceof ApiError && e.status === 404 ? "Barcode not found" : errorMessage(e) };
    }
    const p = r.product;
    if (!p.is_active || !p.transfer_allowed) return { tone: "error", title: `${p.name} cannot be transferred` };
    let level = lines.find((l) => l.product.product_id === p.id)?.product;
    if (!level) {
      const found = await api<Paged<StockLevel>>("/stock", { query: { q: code, limit: 5 } }).catch(() => null);
      level = found?.items.find((x) => x.product_id === p.id)
        ?? (await api<Paged<StockLevel>>("/stock", { query: { q: p.name, limit: 20 } }).catch(() => null))?.items.find((x) => x.product_id === p.id);
      if (!level) return { tone: "error", title: `${p.name} has no stock at ${branch?.name}` };
    }
    const product = level;
    if (p.track_items) {
      const item = r.stock_item;
      if (!item) return { tone: "error", title: `${p.name} is tracked per item`, detail: "Scan the individual item's barcode label." };
      if (!item.in_current_branch) return { tone: "error", title: `This item is at ${item.branch_name}` };
      if (item.status !== "in_stock") return { tone: "error", title: `This item is ${item.status.replace("_", " ")}` };
      if (lines.some((l) => l.barcodes.includes(code))) return { tone: "info", title: "Already on this transfer" };
      setLines((ls) => ls.some((l) => l.product.product_id === p.id)
        ? ls.map((l) => (l.product.product_id === p.id ? { ...l, barcodes: [...l.barcodes, code] } : l))
        : [...ls, { product, quantity: 1, barcodes: [code] }]);
      return { tone: "success", title: `${p.name} added` };
    }
    const current = lines.find((l) => l.product.product_id === p.id)?.quantity ?? 0;
    if (current + 1 > product.available) return { tone: "error", title: `Only ${product.available} ${p.name} available here` };
    setLines((ls) => ls.some((l) => l.product.product_id === p.id)
      ? ls.map((l) => (l.product.product_id === p.id ? { ...l, quantity: l.quantity + 1 } : l))
      : [...ls, { product, quantity: 1, barcodes: [] }]);
    return { tone: "success", title: `${p.name} · Qty ${current + 1}` };
  };
  const save = useMutation({
    mutationFn: (submit: boolean) =>
      api<{ id: string; status: string }>("/transfers", {
        body: {
          to_branch_id: to,
          transfer_date: date,
          notes,
          submit,
          items: lines.map((l) => ({ product_id: l.product.product_id, quantity: l.product.track_items ? l.barcodes.length : l.quantity, barcodes: l.barcodes })),
        },
      }),
    onSuccess: (r) => {
      toast.success(r.status === "pending_approval" ? "Transfer sent for approval" : r.status === "approved" ? "Transfer approved — ready to dispatch" : "Draft saved");
      navigate(`/transfers/${r.id}`);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const valid = to && lines.length > 0 && lines.every((l) => (l.product.track_items ? l.barcodes.length > 0 : l.quantity > 0));

  return (
    <>
      <PageHeader back="/transfers" eyebrow="Stock" title="New transfer" actions={<Button variant="ink" onClick={() => setScanFor("any")}><ScanLine /> Scan</Button>} />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        <Section title="Products">
          <SearchInput value={q} onChange={setQ} placeholder="Add products with stock at this branch" />
          {term && (
            <ul className="mt-2 divide-y rounded-lg border">
              {results.data?.items.filter((r) => !lines.some((l) => l.product.product_id === r.product_id)).map((r) => (
                <li key={r.product_id}>
                  <button className="flex w-full justify-between px-3 py-2.5 text-start text-sm hover:bg-accent" onClick={() => { setLines([...lines, { product: r, quantity: 1, barcodes: [] }]); setQ(""); }}>
                    <span>{r.name}</span><span className="num text-muted-foreground">{count(r.available)} available</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <ul className="mt-3 divide-y">
            {lines.map((l) => (
              <li key={l.product.product_id} className="space-y-2 py-3">
                <div className="flex items-center gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{l.product.name}</div>
                    <div className="num text-xs text-muted-foreground">{count(l.product.available)} available</div>
                  </div>
                  {l.product.track_items ? (
                    <Button variant="outline" size="sm" onClick={() => setScanFor(l.product.product_id)}><ScanLine /> Scan ({l.barcodes.length})</Button>
                  ) : (
                    <div className="flex items-center rounded-lg border">
                      <button className="p-2" onClick={() => update(l.product.product_id, (x) => ({ ...x, quantity: Math.max(1, x.quantity - 1) }))} aria-label="Less"><Minus className="h-3.5 w-3.5" /></button>
                      <Input inputMode="numeric" className="num h-8 w-14 border-0 text-center" value={l.quantity} onChange={(e) => update(l.product.product_id, (x) => ({ ...x, quantity: Math.min(x.product.available, parseInt(e.target.value) || 0) }))} />
                      <button className="p-2" onClick={() => update(l.product.product_id, (x) => ({ ...x, quantity: Math.min(x.product.available, x.quantity + 1) }))} aria-label="More"><Plus className="h-3.5 w-3.5" /></button>
                    </div>
                  )}
                  <Button variant="ghost" size="icon-sm" onClick={() => setLines(lines.filter((x) => x !== l))} aria-label="Remove"><Trash2 /></Button>
                </div>
                {l.barcodes.length > 0 && (
                  <div className="flex flex-wrap gap-1.5">
                    {l.barcodes.map((b) => (
                      <span key={b} className="num inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs">{b}<button onClick={() => update(l.product.product_id, (x) => ({ ...x, barcodes: x.barcodes.filter((y) => y !== b) }))} aria-label="Remove"><X className="h-3 w-3" /></button></span>
                    ))}
                  </div>
                )}
              </li>
            ))}
            {!lines.length && <li className="py-8 text-center text-sm text-muted-foreground">Search above to add products.</li>}
          </ul>
        </Section>
        <div className="space-y-5">
          <Section title="Route">
            <div className="space-y-4">
              <div className="flex items-center gap-2 rounded-xl bg-muted/60 p-3 text-sm">
                <span className="font-medium">{branch?.name}</span><ArrowRight className="h-4 w-4 text-muted-foreground" /><span className="text-muted-foreground">{destinations.find((d) => d.id === to)?.name ?? "Choose destination"}</span>
              </div>
              <Field label="To branch">
                <NativeSelect value={to} onChange={setTo}>
                  <option value="">Select…</option>
                  {destinations.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
                </NativeSelect>
              </Field>
              <Field label="Transfer date"><Input type="date" value={date} onChange={(e) => setDate(e.target.value)} /></Field>
              <Field label="Reason / notes" optional><Textarea value={notes} onChange={(e) => setNotes(e.target.value)} /></Field>
            </div>
          </Section>
          <div className="grid grid-cols-2 gap-2">
            <Button variant="outline" disabled={!valid || save.isPending} onClick={() => save.mutate(false)}>Save draft</Button>
            <Button disabled={!valid || save.isPending} onClick={() => save.mutate(true)}>{save.isPending ? <Loader2 className="animate-spin" /> : "Submit"}</Button>
          </div>
        </div>
      </div>
      <BarcodeScanner open={!!scanFor} onOpenChange={(o) => !o && setScanFor(null)} continuous title="Scan items to transfer" onDetected={onScan} />
    </>
  );
}
