import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Minus, Plus, RefreshCcw, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { amount, money, toNum } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { PosProduct, SaleDetail } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ErrorState, Loading, PageHeader, Section } from "@/components/Page";
import { Field, Select } from "@/components/Form";
import { Chip, SearchInput } from "@/components/Filters";
import { BarcodeScanner } from "@/components/BarcodeScanner";

interface NewLine {
  key: string;
  product: PosProduct;
  quantity: number;
  barcode?: string;
}

/** Exchange: items come back from a sale and others go out, settled in one step (the difference only). */
export default function Exchange() {
  const { id } = useParams();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { profile } = useSession();
  const s = profile!.settings;
  const sale = useQuery({ queryKey: ["sale", id], queryFn: () => api<SaleDetail>(`/sales/${id}`) });
  const [back, setBack] = useState<Record<string, number>>({});
  const [lines, setLines] = useState<NewLine[]>([]);
  const [q, setQ] = useState("");
  const term = useDebounced(q);
  const [scanFor, setScanFor] = useState<PosProduct | null>(null);
  const methods = s.sales.payment_methods.filter((m) => m.enabled && m.key !== "credit");
  const [method, setMethod] = useState(methods.find((m) => m.key === "cash")?.key ?? methods[0]?.key ?? "cash");
  const [code, setCode] = useState("");
  const [refundMethod, setRefundMethod] = useState("cash");
  const [reason, setReason] = useState("");
  const [clientRef] = useState(() => crypto.randomUUID());

  const products = useQuery({
    queryKey: ["pos-products", "exchange", term],
    queryFn: () => api<PosProduct[]>("/pos/products", { query: { q: term } }),
    enabled: term.trim().length >= 2,
  });

  const returnable = useMemo(() => sale.data?.items.filter((i) => i.quantity > i.returned_qty) ?? [], [sale.data]);
  const backValue = returnable.reduce((a, i) => a + toNum(i.unit_price) * (back[i.id] ?? 0), 0);
  const newTotal = lines.reduce((a, l) => a + toNum(l.product.marked_price) * l.quantity, 0);
  const diff = newTotal - backValue;

  const add = (p: PosProduct) => {
    if (p.track_items) return setScanFor(p);
    setLines((ls) => {
      const found = ls.find((l) => l.product.id === p.id);
      if (found) return ls.map((l) => (l === found ? { ...l, quantity: Math.min(l.quantity + 1, p.available) } : l));
      return [...ls, { key: crypto.randomUUID(), product: p, quantity: 1 }];
    });
    setQ("");
  };

  const submit = useMutation({
    mutationFn: () =>
      api<SaleDetail>(`/sales/${id}/exchange`, {
        body: {
          return_items: Object.entries(back).filter(([, n]) => n > 0).map(([sale_item_id, quantity]) => ({ sale_item_id, quantity })),
          items: lines.map((l) => ({ product_id: l.product.id, quantity: l.quantity, unit_price: toNum(l.product.marked_price), barcode: l.barcode })),
          payment: { method, reference: method === "mpesa" ? code : "", phone: "" },
          refund_method: refundMethod,
          reason: reason.trim(),
          client_ref: clientRef,
        },
      }),
    onSuccess: (r) => {
      toast.success("Exchange recorded");
      qc.invalidateQueries({ queryKey: ["sale", id] });
      qc.invalidateQueries({ queryKey: ["sales"] });
      qc.invalidateQueries({ queryKey: ["pos-products"] });
      navigate(`/sales/${r.sale.id}`, { replace: true });
    },
    onError: (e) => toast.error(e),
  });

  if (sale.error) return <ErrorState error={sale.error} retry={sale.refetch} />;
  if (sale.isLoading || !sale.data) return <Loading />;
  const sd = sale.data.sale;
  const blockers: string[] = [];
  if (!Object.values(back).some((n) => n > 0)) blockers.push("Choose what is coming back");
  if (!lines.length) blockers.push("Add what the customer takes instead");
  if (reason.trim().length < 3) blockers.push("Enter the reason");

  return (
    <>
      <PageHeader back={`/sales/${id}`} eyebrow="Exchange" title={<span className="num">{sd.receipt_no}</span>} description={sd.customer?.name} />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        <div className="space-y-5">
          <Section title="Coming back">
            <ul className="divide-y">
              {returnable.map((i) => (
                <li key={i.id} className="flex items-center gap-3 py-2.5">
                  <div className="min-w-0 flex-1 text-sm">
                    <div className="truncate font-medium">{i.product_name}</div>
                    <div className="num text-xs text-muted-foreground">{i.quantity - i.returned_qty} {t("returnable")} · {amount(i.unit_price)}{i.barcode && ` · ${i.barcode}`}</div>
                  </div>
                  <Input
                    inputMode="numeric"
                    className="num w-20 text-center"
                    placeholder="0"
                    aria-label={t("Quantity")}
                    value={back[i.id] || ""}
                    onChange={(e) => setBack({ ...back, [i.id]: Math.min(i.quantity - i.returned_qty, Math.max(0, parseInt(e.target.value) || 0)) })}
                  />
                </li>
              ))}
            </ul>
          </Section>
          <Section title="Taking instead">
            <SearchInput value={q} onChange={setQ} placeholder="Search product name or code" />
            {term.trim().length >= 2 && (
              <ul className="mt-2 divide-y rounded-lg border">
                {(products.data ?? []).slice(0, 8).map((p) => (
                  <li key={p.id}>
                    <button type="button" disabled={p.available <= 0} onClick={() => add(p)} className="flex w-full items-center justify-between gap-3 px-3 py-2.5 text-start text-sm hover:bg-accent disabled:opacity-50">
                      <span className="min-w-0 truncate">{p.name}{p.track_items && <span className="text-xs text-muted-foreground"> · {t("scan item")}</span>}</span>
                      <span className="num shrink-0 font-medium">{money(p.marked_price)}</span>
                    </button>
                  </li>
                ))}
                {products.data?.length === 0 && <li className="px-3 py-3 text-sm text-muted-foreground">{t("No products found.")}</li>}
              </ul>
            )}
            <ul className="mt-3 divide-y">
              {lines.map((l) => (
                <li key={l.key} className="flex items-center gap-3 py-2.5 text-sm">
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{l.product.name}</div>
                    <div className="num text-xs text-muted-foreground">{money(l.product.marked_price)}{l.barcode && ` · ${l.barcode}`}</div>
                  </div>
                  {!l.barcode && (
                    <div className="flex items-center gap-1">
                      <Button size="icon-sm" variant="outline" aria-label="Less" onClick={() => setLines(lines.map((x) => (x === l ? { ...x, quantity: Math.max(1, x.quantity - 1) } : x)))}><Minus /></Button>
                      <span className="num w-6 text-center">{l.quantity}</span>
                      <Button size="icon-sm" variant="outline" aria-label="More" onClick={() => setLines(lines.map((x) => (x === l ? { ...x, quantity: Math.min(x.product.available, x.quantity + 1) } : x)))}><Plus /></Button>
                    </div>
                  )}
                  <Button size="icon-sm" variant="ghost" aria-label="Remove" onClick={() => setLines(lines.filter((x) => x !== l))}><Trash2 /></Button>
                </li>
              ))}
            </ul>
          </Section>
        </div>
        <div className="space-y-5">
          <Section title="Settlement">
            <div className="space-y-1.5 text-sm">
              <div className="flex justify-between"><span>{t("Coming back")}</span><span className="num">≈ {money(backValue)}</span></div>
              <div className="flex justify-between"><span>{t("Taking instead")}</span><span className="num">{money(newTotal)}</span></div>
              <div className={cn("flex justify-between rounded-lg p-2.5 font-semibold", diff > 0 ? "bg-primary/10 text-primary" : diff < 0 ? "bg-warning/10 text-warning" : "bg-muted")}>
                <span>{diff > 0 ? t("Customer pays") : diff < 0 ? t("Refund to customer") : t("Even exchange")}</span>
                <span className="num">{money(Math.abs(diff))}</span>
              </div>
            </div>
            {diff > 0 && (
              <div className="mt-3 space-y-2">
                <div className="flex flex-wrap gap-2">
                  {methods.map((m) => <Chip key={m.key} active={method === m.key} onClick={() => setMethod(m.key)}>{m.label}</Chip>)}
                </div>
                {method === "mpesa" && (
                  <Field label="M-Pesa confirmation code" optional>
                    <Input value={code} onChange={(e) => setCode(e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ""))} placeholder="e.g. QFT1ABC2DE" className="num uppercase placeholder:normal-case" maxLength={12} />
                  </Field>
                )}
              </div>
            )}
            {diff < 0 && (
              <Field label="Refund via" className="mt-3">
                <Select value={refundMethod} onChange={setRefundMethod}>
                  {methods.map((m) => <option key={m.key} value={m.key}>{m.label}</option>)}
                </Select>
              </Field>
            )}
            <Field label="Reason" className="mt-3">
              <Textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. wrong size, customer preferred another colour" />
            </Field>
            <Button className="mt-4 w-full" size="lg" disabled={blockers.length > 0 || submit.isPending} onClick={() => submit.mutate()}>
              <RefreshCcw /> {t("Complete exchange")}
            </Button>
            {blockers.length > 0 && <p className="mt-2 text-center text-xs text-muted-foreground">{t(blockers[0])}</p>}
          </Section>
        </div>
      </div>
      <BarcodeScanner
        open={!!scanFor}
        onOpenChange={(o) => !o && setScanFor(null)}
        title={scanFor ? `${t("Scan")} ${scanFor.name}` : ""}
        onDetected={async (barcode) => {
          if (!scanFor) return;
          if (lines.some((l) => l.barcode === barcode)) return { tone: "error", title: t("Already in this sale") };
          await api("/sales/check-barcode", { body: { product_id: scanFor.id, barcode } });
          setLines((ls) => [...ls, { key: crypto.randomUUID(), product: scanFor, quantity: 1, barcode }]);
          setQ("");
        }}
      />
    </>
  );
}
