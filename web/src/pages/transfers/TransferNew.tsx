import { useState } from "react";
import { useNavigate } from "@/lib/router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowRight, Minus, Plus, ScanLine, Trash2, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, todayIso, maskCode, money } from "@/lib/format";
import type { Money, Paged, StockLevel } from "@/lib/types";
import { Checkbox } from "@/components/ui/checkbox";
import { PRICE_ACTION, type PriceAction } from "./TransferPrices";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { PageHeader, Section } from "@/components/Page";
import { Field, Select } from "@/components/Form";
import { SearchInput } from "@/components/Filters";
import { BarcodeScanner, type ScanOutcome } from "@/components/BarcodeScanner";
import { t } from "@/lib/i18n";

interface Line {
  product: StockLevel;
  quantity: number;
  barcodes: string[];
}

/** Roadmap 176: both branches' prices for the products on the transfer. */
interface PriceCheck {
  may_set_price: boolean;
  items: { product_id: string; product_name: string; source_price: Money | null; dest_price: Money | null; dest_has_own_price: boolean }[];
}

export default function TransferNew() {
  const { profile, branch } = useSession();
  const navigate = useNavigate();
  const [to, setTo] = useState("");
  // Roadmap 176: the source branch (default: the working branch) and what happens to each product's price at the destination.
  const [from, setFrom] = useState(branch?.id ?? "");
  const fromName = profile?.branches.find((b) => b.id === from)?.name ?? branch?.name ?? "";
  const [choice, setChoiceState] = useState<Record<string, { action: PriceAction; price: string }>>({});
  const [confirmed, setConfirmed] = useState(false);
  const setChoice = (id: string, c: { action: PriceAction; price: string }) => { setChoiceState((s) => ({ ...s, [id]: c })); setConfirmed(false); };
  const [date, setDate] = useState(todayIso());
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<Line[]>([]);
  const [q, setQ] = useState("");
  const [scanFor, setScanFor] = useState<string | null>(null);
  const term = useDebounced(q);
  const branches = useQuery({ queryKey: ["branches"], queryFn: () => api<{ id: string; name: string; is_active: boolean }[]>("/branches") });
  const results = useQuery({ queryKey: ["stock", "transfer-pick", term, from], queryFn: () => api<Paged<StockLevel>>("/stock", { query: { q: term, status: "in", limit: 10, branch_id: from } }), enabled: !!term });
  const destinations = (branches.data ?? profile?.branches.map((b) => ({ ...b, is_active: true })) ?? []).filter((b) => b.is_active && b.id !== from);
  const toName = destinations.find((d) => d.id === to)?.name ?? "";
  const ids = lines.map((l) => l.product.product_id);
  const priceCheck = useQuery({
    queryKey: ["transfer-prices", from, to, ids.join(",")],
    queryFn: () => api<PriceCheck>("/transfers/prices", { query: { from_branch_id: from, to_branch_id: to, products: ids.join(",") } }),
    enabled: !!to && ids.length > 0,
  });
  const choiceOf = (id: string) => choice[id] ?? { action: "keep" as PriceAction, price: "" };
  const priceChanges = lines.filter((l) => choiceOf(l.product.product_id).action !== "keep");

  const update = (id: string, fn: (l: Line) => Line) => setLines((ls) => ls.map((l) => (l.product.product_id === id ? fn(l) : l)));

  // Scanning adds products (product barcode) or specific units (item labels), checked against this branch's stock.
  const onScan = async (code: string): Promise<ScanOutcome> => {
    let r: { product: { id: string; name: string; track_items: boolean; transfer_allowed: boolean; is_active: boolean; available: number }; stock_item: { status: string; in_current_branch: boolean; branch_name: string } | null };
    try {
      r = await api("/products/lookup", { query: { code, branch_id: from } });
    } catch (e) {
      return { tone: "error", title: e instanceof ApiError && e.status === 404 ? "Barcode not found" : errorMessage(e) };
    }
    const p = r.product;
    if (!p.is_active || !p.transfer_allowed) return { tone: "error", title: `${p.name} cannot be transferred` };
    let level = lines.find((l) => l.product.product_id === p.id)?.product;
    if (!level) {
      const found = await api<Paged<StockLevel>>("/stock", { query: { q: code, limit: 5, branch_id: from } }).catch(() => null);
      level = found?.items.find((x) => x.product_id === p.id)
        ?? (await api<Paged<StockLevel>>("/stock", { query: { q: p.name, limit: 20, branch_id: from } }).catch(() => null))?.items.find((x) => x.product_id === p.id);
      if (!level) return { tone: "error", title: `${p.name} has no stock at ${fromName}` };
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
          from_branch_id: from,
          to_branch_id: to,
          transfer_date: date,
          notes,
          submit,
          price_confirmed: confirmed,
          items: lines.map((l) => {
            const c = choiceOf(l.product.product_id);
            return {
              product_id: l.product.product_id, quantity: l.product.track_items ? l.barcodes.length : l.quantity, barcodes: l.barcodes,
              price_action: c.action, new_price: c.action === "custom" && c.price !== "" ? Number(c.price) : undefined,
            };
          }),
        },
      }),
    onSuccess: (r) => {
      toast.success(r.status === "pending_approval" ? "Transfer sent for approval" : r.status === "approved" ? "Transfer approved — ready to dispatch" : "Draft saved");
      navigate(`/transfers/${r.id}`);
    },
    onError: (e) => toast.error(e),
  });
  // What is still missing, in the order the screen asks for it (first one labels the action).
  const missing = [
    !lines.length && "Add products",
    lines.some((l) => (l.product.track_items ? l.barcodes.length === 0 : l.quantity <= 0)) && "Enter quantities",
    !to && "Choose destination",
    priceChanges.some((l) => choiceOf(l.product.product_id).action === "custom" && choiceOf(l.product.product_id).price === "") && "Enter the new price",
    priceChanges.length > 0 && !confirmed && "Confirm the price change",
  ];

  return (
    <>
      <PageHeader back="/transfers" eyebrow="Stock" title="New transfer" actions={<Button variant="ink" onClick={() => setScanFor("any")}><ScanLine /> {t("Scan")}</Button>} />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        <div className="min-w-0 space-y-5">
        <Section title="Products">
          <SearchInput value={q} onChange={setQ} placeholder={`${t("Add products with stock at")} ${fromName}`} />
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
                    <Button variant="outline" size="sm" onClick={() => setScanFor(l.product.product_id)}><ScanLine /> {t("Scan (")}{l.barcodes.length})</Button>
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
                      <span key={b} className="num inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs">{maskCode(b)}<button onClick={() => update(l.product.product_id, (x) => ({ ...x, barcodes: x.barcodes.filter((y) => y !== b) }))} aria-label="Remove"><X className="h-3 w-3" /></button></span>
                    ))}
                  </div>
                )}
              </li>
            ))}
            {!lines.length && <li className="py-8 text-center text-sm text-muted-foreground">{t("Search above to add products.")}</li>}
          </ul>
        </Section>
        {to && lines.length > 0 && (
          <Section title={`${t("Selling prices at")} ${toName}`}>
            <p className="mb-3 text-xs text-muted-foreground">{t("By default the destination keeps its own prices. Choose a change only if the price at")} {toName} {t("should change once this stock arrives.")}</p>
            {priceCheck.isLoading ? <p className="text-sm text-muted-foreground">{t("Loading…")}</p> : (
              <ul className="divide-y">
                {priceCheck.data?.items.map((i) => {
                  const c = choiceOf(i.product_id);
                  const same = i.source_price != null && i.dest_price != null && Number(i.source_price) === Number(i.dest_price);
                  return (
                    <li key={i.product_id} className="space-y-2 py-3 text-sm" data-testid="transfer-price-line">
                      <div className="font-medium">{i.product_name}</div>
                      <div className="grid grid-cols-2 gap-2 text-xs">
                        <div className="rounded-lg bg-muted/60 p-2"><div className="text-muted-foreground">{fromName}</div><div className="num font-semibold">{i.source_price == null ? t("Not set") : money(i.source_price)}</div></div>
                        <div className="rounded-lg bg-muted/60 p-2"><div className="text-muted-foreground">{toName}</div><div className="num font-semibold">{i.dest_price == null ? t("Not set") : money(i.dest_price)}</div></div>
                      </div>
                      {same ? <p className="text-xs text-muted-foreground">{t("Same price at both branches — nothing to change.")}</p> : (
                        <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_160px]">
                          <Select value={c.action} label="Destination price" onChange={(v) => setChoice(i.product_id, { action: v as PriceAction, price: c.price })}>
                            <option value="keep">{t(PRICE_ACTION.keep)}</option>
                            {i.source_price != null && <option value="source">{t(PRICE_ACTION.source)} ({money(i.source_price)})</option>}
                            <option value="custom">{t(PRICE_ACTION.custom)}</option>
                          </Select>
                          {c.action === "custom" && <Input inputMode="decimal" className="num" aria-label={t("New price")} placeholder={t("New price")} value={c.price} onChange={(e) => setChoice(i.product_id, { action: "custom", price: e.target.value.replace(/[^\d.]/g, "") })} />}
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
            {priceChanges.length > 0 && (
              <div className="mt-2 space-y-2 rounded-lg bg-warning/10 p-3 text-sm">
                {!priceCheck.data?.may_set_price && <p className="text-xs text-warning">{t("You cannot set prices at")} {toName}: {t("the change is sent as a request for someone who can. Nothing changes until they authorise it.")}</p>}
                <label className="flex items-start gap-2.5">
                  <Checkbox checked={confirmed} onCheckedChange={(v) => setConfirmed(v === true)} className="mt-0.5" />
                  <span>{t("I confirm the new selling price at")} {toName} {t("for")} {count(priceChanges.length)} {t(priceChanges.length === 1 ? "product" : "products")}. {t("It applies only once the stock is received.")}</span>
                </label>
              </div>
            )}
          </Section>
        )}
        </div>
        <div className="space-y-5">
          <Section title="Route">
            <div className="space-y-4">
              <div className="flex items-center gap-2 rounded-xl bg-muted/60 p-3 text-sm">
                <span className="font-medium">{fromName}</span><ArrowRight className="h-4 w-4 text-muted-foreground" /><span className="text-muted-foreground">{toName || t("Choose destination")}</span>
              </div>
              {(profile?.branches.length ?? 0) > 1 && (
                <Field label="From branch" hint={lines.length ? "Changing it clears the products" : undefined}>
                  <Select value={from} onChange={(v) => { if (v === from) return; setFrom(v); setLines([]); setChoiceState({}); setConfirmed(false); if (v === to) setTo(""); }}>
                    {profile?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
                  </Select>
                </Field>
              )}
              <Field label="To branch">
                <Select value={to} onChange={setTo}>
                  <option value="">Select…</option>
                  {destinations.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
                </Select>
              </Field>
              <Field label="Transfer date"><Input type="date" value={date} onChange={(e) => setDate(e.target.value)} /></Field>
              <Field label="Reason / notes" optional><Textarea value={notes} onChange={(e) => setNotes(e.target.value)} /></Field>
            </div>
          </Section>
          <div className="grid grid-cols-2 gap-2">
            <ActionButton variant="outline" online quietReason busy={save.isPending && save.variables === false} disabled={save.isPending} busyLabel="Saving…"
              blockedBy={missing} onAction={() => save.mutateAsync(false)}>{t("Save draft")}</ActionButton>
            <ActionButton online busy={save.isPending && save.variables === true} disabled={save.isPending} busyLabel="Submitting…"
              blockedBy={missing} onAction={() => save.mutateAsync(true)}>Submit for approval</ActionButton>
          </div>
        </div>
      </div>
      <BarcodeScanner open={!!scanFor} onOpenChange={(o) => !o && setScanFor(null)} continuous title="Scan items to transfer" onDetected={onScan} />
    </>
  );
}
