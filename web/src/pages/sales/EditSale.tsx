/** Roadmap 191: Edit Sale — Sales → (sale) → Corrections → Edit Sale.
 *
 * The recorded sale, loaded as it stands, in the familiar Record Sale building blocks (customer search, Sale Owner
 * picker, product search and scanning, sale date). Every business detail can be corrected; verified payments are shown
 * locked. *Review changes* asks the server for the comparison (original → proposed) and its effects on money, stock,
 * loyalty and credit; *Submit for approval* (with a reason) sends it through the workflow engine. The sale is unchanged
 * until the final approval. Receipt number, recording time and the person who recorded it are never editable. */
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "@/lib/router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ClipboardCheck, Lock, Minus, Plus, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { money, toNum, maskCode } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { Customer, PosProduct } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { ErrorState, KV, Loading, PageHeader, Section } from "@/components/Page";
import { Field, Select, ToggleRow } from "@/components/Form";
import { SearchInput } from "@/components/Filters";
import { BarcodeScanner } from "@/components/BarcodeScanner";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import type { ScanSource } from "@/lib/scan";
import { CustomerPicker, type NewCustomerDraft } from "./pos/CustomerPicker";
import { OwnerPicker, type Owner } from "./SaleOwner";
import { SaleDateInput, todayIn } from "./pos/SaleDate";

interface EditLineState { key: string; sale_item_id: string | null; product_id: string; name: string; quantity: number; unit_price: string; marked_price: string | number | null; tracked: boolean; barcode: string | null; scan_source?: ScanSource }
interface PayState { key: string; method: string; label: string; amount: string; reference: string; locked: boolean }
interface Editable {
  id: string; receipt_no: string; revision: number; editable: boolean; blocked_reason: string | null;
  sale_date: string; sale_time: string; branch_id: string; customer: { id: string } | null; owner_id: string | null; owner_name: string | null; notes: string;
  gross_total: string; discount_total: string; total: string; redeemed_points: number; redeemed_value: string; payment_method: string; points_earned: number;
  credit: { due_date: string; paid: string; original: string } | null;
  lines: { sale_item_id: string; product_id: string; product_name: string; quantity: number; unit_price: string; marked_price: string; tracked: boolean; barcode: string | null }[];
  payments: { key: string; method: string; label: string; reference: string; amount: string; locked: boolean }[];
}
interface Change { field: string; label: string; before: string; after: string }
interface Preview {
  changes: Change[];
  total: string;
  implications: {
    currency: string;
    financial: { total_before: string; total_after: string; difference: string; discount_before: string; discount_after: string; payments: { method: string; amount: string; reference: string }[] };
    stock: { branch: string; product: string; change: number }[];
    restock?: boolean;
    loyalty: { customer_before: string | null; points_before: number; customer_after: string | null; points_after: number };
    credit: null | { original_before: string; original_after: string; balance_before: string; balance_after: string; due_before: string; due_after: string };
    performance: string | null;
  };
}

const key = () => crypto.randomUUID();

export default function EditSale() {
  const { id } = useParams();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { profile } = useSession();
  const s = profile!.settings;
  const today = todayIn(profile?.tenant.timezone);
  const q = useQuery({ queryKey: ["sale-edit", id], queryFn: () => api<Editable>(`/sales/${id}/edit`) });
  const cust = useQuery({
    queryKey: ["customer", q.data?.customer?.id],
    queryFn: () => api<{ customer: Customer }>(`/customers/${q.data!.customer!.id}`),
    enabled: !!q.data?.customer?.id,
  });

  const [date, setDate] = useState("");
  const [time, setTime] = useState("");
  const [branch, setBranch] = useState("");
  const [customer, setCustomer] = useState<Customer | null>(null);
  const [draft, setDraft] = useState<NewCustomerDraft | null>(null);
  const [owner, setOwner] = useState<{ id: string | null; name: string | null }>({ id: null, name: null });
  const [ownerPicker, setOwnerPicker] = useState(false);
  const [notes, setNotes] = useState("");
  const [due, setDue] = useState("");
  const [lines, setLines] = useState<EditLineState[]>([]);
  const [pays, setPays] = useState<PayState[]>([]);
  const [search, setSearch] = useState("");
  const term = useDebounced(search);
  const [scanFor, setScanFor] = useState<PosProduct | null>(null);
  const [review, setReview] = useState<Preview | null>(null);
  const [reason, setReason] = useState("");
  // Roadmap 192: removed / reduced items go back to stock (off when they cannot be resold).
  const [restock, setRestock] = useState(true);

  // The form starts as the sale stands.
  const reset = (d: Editable) => {
    setDate(d.sale_date);
    setTime(d.sale_time);
    setBranch(d.branch_id);
    setOwner({ id: d.owner_id, name: d.owner_name });
    setNotes(d.notes);
    setDue(d.credit?.due_date ?? "");
    setLines(d.lines.map((l) => ({ key: key(), sale_item_id: l.sale_item_id, product_id: l.product_id, name: l.product_name, quantity: l.quantity, unit_price: String(toNum(l.unit_price)), marked_price: l.marked_price, tracked: l.tracked, barcode: l.barcode })));
    setPays(d.payments.map((p) => ({ key: p.key, method: p.method, label: p.label, amount: String(toNum(p.amount)), reference: p.reference, locked: p.locked })));
  };
  useEffect(() => { if (q.data) reset(q.data); }, [q.data]);
  useEffect(() => { setCustomer(cust.data?.customer ?? null); }, [cust.data]);

  const products = useQuery({
    queryKey: ["pos-products", "edit", term, branch],
    queryFn: () => api<PosProduct[]>("/pos/products", { query: { q: term, branch_id: branch } }),
    enabled: term.trim().length >= 2,
  });

  const d = q.data;
  const credit = !!d?.credit;
  const net = lines.reduce((a, l) => a + toNum(l.unit_price) * l.quantity, 0);
  const gross = lines.reduce((a, l) => a + toNum(l.marked_price ?? l.unit_price) * l.quantity, 0);
  const total = Math.round((net - toNum(d?.redeemed_value ?? 0)) * 100) / 100;
  const allocated = pays.reduce((a, p) => a + toNum(p.amount), 0);
  const remaining = Math.round((total - allocated) * 100) / 100;
  const methods = s.sales.payment_methods.filter((m) => m.enabled && m.key !== "credit");

  const proposal = () => ({
    restock, sale_date: date, sale_time: time, branch_id: branch, customer_id: customer?.id ?? null, owner_id: owner.id, notes, due_date: credit ? due : null,
    lines: lines.map((l) => ({ sale_item_id: l.sale_item_id, product_id: l.product_id, quantity: l.quantity, unit_price: toNum(l.unit_price), barcode: l.sale_item_id ? null : l.barcode, scan_source: l.sale_item_id ? undefined : l.scan_source })),
    payments: credit ? [] : pays.filter((p) => toNum(p.amount) > 0).map((p) => (p.locked ? { key: p.key, method: p.method, amount: toNum(p.amount), reference: p.reference } : { method: p.method, amount: toNum(p.amount), reference: p.reference })),
  });
  // Anything different from the sale as loaded?
  const changed = useMemo(() => {
    if (!d) return false;
    const orig = JSON.stringify([d.sale_date, d.sale_time, d.branch_id, d.customer?.id ?? null, d.owner_id, d.notes, d.credit?.due_date ?? "",
      d.lines.map((l) => [l.sale_item_id, l.quantity, toNum(l.unit_price)]), credit ? [] : d.payments.map((p) => [p.method, toNum(p.amount), p.reference])]);
    const now = JSON.stringify([date, time, branch, customer?.id ?? null, owner.id, notes, due,
      lines.map((l) => [l.sale_item_id, l.quantity, toNum(l.unit_price)]), credit ? [] : pays.filter((p) => toNum(p.amount) > 0).map((p) => [p.method, toNum(p.amount), p.reference])]);
    return orig !== now;
  }, [d, date, time, branch, customer, owner, notes, due, lines, pays, credit]);

  const previewM = useMutation({
    mutationFn: () => api<Preview>(`/sales/${id}/edit/preview`, { body: { proposal: proposal(), revision: d!.revision } }),
    onSuccess: (r) => setReview(r),
    onError: (e) => toast.error(e),
  });
  const submit = useMutation({
    mutationFn: () => api<{ approval_id: string }>(`/sales/${id}/edit`, { body: { proposal: proposal(), reason: reason.trim(), revision: d!.revision } }),
    onSuccess: () => {
      toast.success("Correction sent for approval — the sale stays as it is until it is approved");
      qc.invalidateQueries({ queryKey: ["sale", id] });
      qc.invalidateQueries({ queryKey: ["sale-edit", id] });
      qc.invalidateQueries({ queryKey: ["sale-revisions", id] });
      navigate(`/sales/${id}`, { replace: true });
    },
    onError: (e) => toast.error(e),
  });

  if (q.error) return <ErrorState error={q.error} retry={q.refetch} />;
  if (q.isLoading || !d) return <Loading />;
  if (!d.editable) {
    return (
      <>
        <PageHeader back={`/sales/${id}`} eyebrow="Edit Sale" title={<span className="num">{d.receipt_no}</span>} />
        <div className="surface p-5 text-sm">{t(d.blocked_reason ?? "This sale cannot be edited")}</div>
      </>
    );
  }

  const add = (p: PosProduct) => {
    if (p.track_items) return setScanFor(p);
    setLines((ls) => {
      const found = ls.find((l) => l.product_id === p.id && !l.tracked);
      if (found) return ls.map((l) => (l === found ? { ...l, quantity: l.quantity + 1 } : l));
      return [...ls, { key: key(), sale_item_id: null, product_id: p.id, name: p.name, quantity: 1, unit_price: String(toNum(p.marked_price ?? 0)), marked_price: p.marked_price, tracked: false, barcode: null }];
    });
    setSearch("");
  };
  const fillRemaining = (k: string) => setPays((ps) => ps.map((p) => (p.key === k ? { ...p, amount: String(Math.max(0, toNum(p.amount) + remaining)) } : p)));
  const blockers = [
    !changed && REASONS.nothingToSave,
    !lines.length && "Add at least one product",
    lines.some((l) => l.quantity <= 0 || toNum(l.unit_price) < 0 || l.unit_price === "") && "Enter a quantity and price for every product",
    !credit && Math.abs(remaining) > 0.004 && (remaining > 0 ? "Allocate the full total to payments" : "Payments exceed the total"),
  ];

  return (
    <>
      <PageHeader back={`/sales/${id}`} eyebrow="Edit Sale" title={<span className="num">{d.receipt_no}</span>}
        description="Correct the sale, review the changes and submit them for approval. Nothing changes until the correction is approved." />
      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
        <div className="space-y-5">
          <Section title="Sale details">
            <div className="grid gap-3 sm:grid-cols-3">
              <Field label="Sale date"><div className="flex h-10 items-center rounded-lg border px-3"><SaleDateInput value={date} today={today} onChange={setDate} className="w-full" /></div></Field>
              <Field label="Time"><Input type="time" value={time} onChange={(e) => setTime(e.target.value)} className="num" aria-label={t("Sale time")} /></Field>
              <Field label="Branch">
                <Select value={branch} onChange={setBranch}>{(profile?.branches ?? []).map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}</Select>
              </Field>
            </div>
            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <Field label="Sale owner">
                <Button type="button" variant="outline" className="w-full justify-start" onClick={() => setOwnerPicker(true)}>{owner.name ?? t("Choose…")}</Button>
              </Field>
              {credit && <Field label="Credit due date"><Input type="date" value={due} min={date} onChange={(e) => setDue(e.target.value)} className="num" /></Field>}
            </div>
            <p className="mt-2 text-xs text-muted-foreground">{t("Receipt number, recording time and the person who recorded the sale never change.")}</p>
          </Section>
          <Section title="Customer">
            <CustomerPicker customer={customer} onChange={setCustomer} draft={draft} onDraft={setDraft} />
            {d.redeemed_points > 0 && <p className="mt-2 text-xs text-muted-foreground">{t("Points were redeemed on this sale — the customer cannot be changed.")}</p>}
          </Section>
          <Section title="Products">
            <SearchInput value={search} onChange={setSearch} placeholder="Search product name or code" />
            {term.trim().length >= 2 && (
              <ul className="mt-2 divide-y rounded-lg border">
                {(products.data ?? []).slice(0, 8).map((p) => (
                  <li key={p.id}>
                    <button type="button" disabled={p.marked_price == null} onClick={() => add(p)} className="flex w-full items-center justify-between gap-3 px-3 py-2.5 text-start text-sm hover:bg-accent disabled:opacity-50">
                      <span className="min-w-0 truncate">{p.name}{p.track_items && <span className="text-xs text-muted-foreground"> · {t("scan item")}</span>}<span className="text-xs text-muted-foreground"> · {p.available} {t("available")}</span></span>
                      <span className="num shrink-0 font-medium">{p.marked_price == null ? t("Price not set") : money(p.marked_price)}</span>
                    </button>
                  </li>
                ))}
                {products.data?.length === 0 && <li className="px-3 py-3 text-sm text-muted-foreground">{t("No products found.")}</li>}
              </ul>
            )}
            <ul className="mt-3 divide-y">
              {lines.map((l) => (
                <li key={l.key} className="space-y-1.5 py-2.5 text-sm">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <div className="truncate font-medium">{l.name}{!l.sale_item_id && <span className="ms-1 rounded bg-primary/10 px-1 text-[10px] text-primary">{t("Added")}</span>}</div>
                      <div className="num text-xs text-muted-foreground">{l.marked_price != null && `${t("Marked")} ${money(l.marked_price)}`}{l.barcode && ` · ${l.tracked && !l.sale_item_id ? maskCode(l.barcode) : l.barcode}`}</div>
                    </div>
                    <span className="num shrink-0 font-medium">{money(toNum(l.unit_price) * l.quantity)}</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    {!l.tracked ? (
                      <div className="flex items-center gap-1">
                        <Button size="icon-sm" variant="outline" aria-label={t("Less")} onClick={() => setLines(lines.map((x) => (x === l ? { ...x, quantity: Math.max(1, x.quantity - 1) } : x)))}><Minus /></Button>
                        <Input inputMode="numeric" aria-label={t("Quantity")} className="num h-8 w-12 text-center" value={l.quantity}
                          onChange={(e) => setLines(lines.map((x) => (x === l ? { ...x, quantity: Math.max(0, parseInt(e.target.value) || 0) } : x)))} />
                        <Button size="icon-sm" variant="outline" aria-label={t("More")} onClick={() => setLines(lines.map((x) => (x === l ? { ...x, quantity: x.quantity + 1 } : x)))}><Plus /></Button>
                      </div>
                    ) : <span className="text-xs text-muted-foreground">1 · {t("tracked item")}</span>}
                    <span className="ms-auto text-xs text-muted-foreground">@</span>
                    <Input inputMode="decimal" aria-label={`${t("Selling price")} ${l.name}`} className="num h-8 w-28 text-end" value={l.unit_price}
                      onChange={(e) => setLines(lines.map((x) => (x === l ? { ...x, unit_price: e.target.value.replace(/[^\d.]/g, "") } : x)))} />
                    <Button size="icon-sm" variant="ghost" aria-label={t("Remove")} onClick={() => setLines(lines.filter((x) => x !== l))}><Trash2 /></Button>
                  </div>
                </li>
              ))}
            </ul>
          </Section>
          {d.lines.some((o) => { const n = lines.find((l) => l.sale_item_id === o.sale_item_id); return !n || n.quantity < o.quantity; }) && (
            <Section title="Items coming off the sale">
              <ToggleRow label="Return removed items to stock" hint="On approval they are available again at the selling branch; tracked items get their original barcode back. Turn off for items that cannot be resold." checked={restock} onChange={setRestock} />
            </Section>
          )}
          <Section title="Notes">
            <Textarea rows={2} maxLength={500} value={notes} onChange={(e) => setNotes(e.target.value)} />
          </Section>
        </div>

        <div className="space-y-5">
          <Section title="Payment">
            <div className="space-y-1 text-sm">
              <KV label="Subtotal"><span className="num">{money(gross)}</span></KV>
              <KV label="Discount"><span className="num">{money(Math.max(0, gross - net))}</span></KV>
              {toNum(d.redeemed_value) > 0 && <KV label="Paid with points"><span className="num">−{money(d.redeemed_value)}</span></KV>}
              <KV label="Corrected total" className="font-semibold"><span className="num">{money(total)}</span></KV>
              <KV label="Original total"><span className="num text-muted-foreground">{money(d.total)}</span></KV>
            </div>
            {credit ? (
              <p className="mt-3 rounded-lg bg-muted/50 p-2.5 text-xs text-muted-foreground">
                {t("Credit sale — the amount owed follows the corrected total (never below what was already paid:")} {money(d.credit!.paid)}).
              </p>
            ) : (
              <div className="mt-3 space-y-2">
                {pays.map((p) => (
                  <div key={p.key} className={cn("rounded-lg border p-2", p.locked && "bg-muted/40")}>
                    {p.locked ? (
                      <div className="flex items-center justify-between gap-2 text-sm">
                        <span className="flex items-center gap-1.5"><Lock className="h-3.5 w-3.5 text-muted-foreground" /> {p.label}{p.reference && <span className="num text-xs text-muted-foreground">{p.reference}</span>}</span>
                        <span className="num font-medium">{money(p.amount)}</span>
                      </div>
                    ) : (
                      <div className="grid grid-cols-[minmax(0,1fr)_7rem_auto] items-center gap-1.5">
                        <Select value={p.method} onChange={(v) => setPays(pays.map((x) => (x === p ? { ...x, method: v } : x)))}>
                          {[...methods.map((m) => [m.key, m.label]), ...(methods.some((m) => m.key === p.method) ? [] : [[p.method, p.label]])].map(([k, l]) => <option key={k} value={k}>{l}</option>)}
                        </Select>
                        <Input inputMode="decimal" aria-label={t("Amount")} className="num h-9 text-end" value={p.amount} onChange={(e) => setPays(pays.map((x) => (x === p ? { ...x, amount: e.target.value.replace(/[^\d.]/g, "") } : x)))} />
                        <Button size="icon-sm" variant="ghost" aria-label={t("Remove")} onClick={() => setPays(pays.filter((x) => x !== p))}><Trash2 /></Button>
                        <Input className="num col-span-2 h-8 text-xs uppercase placeholder:normal-case" placeholder={t(p.method === "mpesa" ? "M-Pesa confirmation code (optional)" : "Reference (optional)")} value={p.reference}
                          onChange={(e) => setPays(pays.map((x) => (x === p ? { ...x, reference: e.target.value.toUpperCase().slice(0, 40) } : x)))} />
                        {Math.abs(remaining) > 0.004 && <Button size="sm" variant="ghost" className="h-8 px-2 text-xs" onClick={() => fillRemaining(p.key)}>{remaining > 0 ? t("+ rest") : t("fit")}</Button>}
                      </div>
                    )}
                  </div>
                ))}
                {pays.some((p) => p.locked) && <p className="text-xs text-muted-foreground">{t("Verified payments are locked. A lower total than they cover needs Return items / refund.")}</p>}
                <Button variant="outline" size="sm" className="w-full" onClick={() => setPays([...pays, { key: key(), method: methods[0]?.key ?? "cash", label: methods[0]?.label ?? "Cash", amount: remaining > 0 ? String(remaining) : "", reference: "", locked: false }])}>
                  <Plus /> {t("Add payment")}
                </Button>
                <div className={cn("flex justify-between rounded-lg p-2 text-sm font-medium", Math.abs(remaining) <= 0.004 ? "bg-success/10 text-success" : "bg-warning/10 text-warning")}>
                  <span>{Math.abs(remaining) <= 0.004 ? t("Fully allocated") : remaining > 0 ? t("Still to allocate") : t("Over-allocated")}</span>
                  <span className="num">{money(Math.abs(remaining))}</span>
                </div>
              </div>
            )}
            <ActionButton className="mt-4 w-full" size="lg" online busy={previewM.isPending} busyLabel="Checking…" blockedBy={blockers} onAction={() => previewM.mutateAsync()}>
              <ClipboardCheck /> {t("Review changes")}
            </ActionButton>
            {changed && <Button variant="ghost" size="sm" className="mt-2 w-full" onClick={() => reset(d)}>{t("Undo all changes")}</Button>}
          </Section>
        </div>
      </div>

      <OwnerPicker open={ownerPicker} onOpenChange={setOwnerPicker} branchId={branch} value={owner.id} title="Sale owner"
        onPick={(o: Owner) => { setOwner({ id: o.id, name: o.name }); setOwnerPicker(false); }} />
      <BarcodeScanner
        open={!!scanFor}
        onOpenChange={(o) => !o && setScanFor(null)}
        title={scanFor ? `${t("Scan")} ${scanFor.name}` : ""}
        context="sale"
        onDetected={async (barcode, source) => {
          if (!scanFor) return;
          if (lines.some((l) => l.barcode === barcode)) return { tone: "error", title: t("Already in this sale") };
          await api("/sales/check-barcode", { body: { product_id: scanFor.id, barcode, branch_id: branch } });
          setLines((ls) => [...ls, { key: key(), sale_item_id: null, product_id: scanFor.id, name: scanFor.name, quantity: 1, unit_price: String(toNum(scanFor.marked_price ?? 0)), marked_price: scanFor.marked_price, tracked: true, barcode, scan_source: source }]);
          setSearch("");
          setScanFor(null);
        }}
      />
      <ResponsiveDialog open={!!review} onOpenChange={(o) => !o && setReview(null)} title="Review the correction" description="Nothing changes until the correction is approved."
        footer={<ActionButton online busy={submit.isPending} busyLabel="Submitting…" blockedBy={[!reason.trim() && "Give the reason for the correction"]} onAction={() => submit.mutateAsync()}>Submit for Approval</ActionButton>}>
        {review && (
          <div className="space-y-4">
            <SaleEditChanges changes={review.changes} implications={review.implications} />
            <Field label="Reason for the correction"><Textarea rows={2} maxLength={500} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("e.g. wrong customer and price entered")} /></Field>
          </div>
        )}
      </ResponsiveDialog>
    </>
  );
}

/** The comparison (Parameter · Original · Proposed) and what the correction does to money, stock, loyalty and credit —
 * on the review dialog, in Approvals and in the sale's correction history. */
export function SaleEditChanges({ changes, implications }: { changes: Change[]; implications?: Preview["implications"] | null }) {
  const im = implications;
  const cur = im?.currency ?? "";
  const m = (v: string | number) => `${cur} ${Number(v).toLocaleString(undefined, { maximumFractionDigits: 2 })}`.trim();
  return (
    <div className="space-y-3" data-testid="sale-edit-changes">
      <div className="overflow-x-auto rounded-lg border">
        <table className="w-full text-sm">
          <thead className="bg-muted/50 text-xs text-muted-foreground">
            <tr><th className="px-2.5 py-1.5 text-start font-medium">{t("Parameter")}</th><th className="px-2.5 py-1.5 text-start font-medium">{t("Original")}</th><th className="px-2.5 py-1.5 text-start font-medium">{t("Proposed")}</th></tr>
          </thead>
          <tbody className="divide-y">
            {changes.map((c, i) => (
              <tr key={i} className="align-top">
                <td className="px-2.5 py-1.5 font-medium">{c.label}</td>
                <td className="px-2.5 py-1.5 text-muted-foreground"><span className="num">{c.before}</span></td>
                <td className="px-2.5 py-1.5"><span className="num">{c.after}</span></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {im && (
        <div className="grid gap-2 text-xs sm:grid-cols-2">
          <div className="rounded-lg bg-muted/40 p-2.5">
            <div className="mb-1 font-medium">{t("Money")}</div>
            <div>{t("Total")} <span className="num">{m(im.financial.total_before)} → {m(im.financial.total_after)}</span> ({toNum(im.financial.difference) >= 0 ? "+" : ""}<span className="num">{m(im.financial.difference)}</span>)</div>
            {im.financial.payments.map((p, i) => <div key={i}>{p.method} <span className="num">{toNum(p.amount) > 0 ? "+" : ""}{m(p.amount)}</span> · {t("linked adjustment")}</div>)}
          </div>
          <div className="rounded-lg bg-muted/40 p-2.5">
            <div className="mb-1 font-medium">{t("Stock")}</div>
            {im.restock === false && <div className="text-warning">{t("Removed items are not returned to stock")}</div>}
            {im.stock.length ? im.stock.map((x, i) => <div key={i}>{x.product} · {x.branch} <span className="num">{x.change > 0 ? `+${x.change} ${t("back to stock")}` : `${x.change} ${t("from stock")}`}</span></div>) : <div className="text-muted-foreground">{t("No stock movement")}</div>}
          </div>
          <div className="rounded-lg bg-muted/40 p-2.5">
            <div className="mb-1 font-medium">{t("Loyalty & customer")}</div>
            <div>{im.loyalty.customer_before ?? t("Walk-in")} · <span className="num">{im.loyalty.points_before}</span> {t("pts")} → {im.loyalty.customer_after ?? t("Walk-in")} · <span className="num">{im.loyalty.points_after}</span> {t("pts")}</div>
          </div>
          {im.credit ? (
            <div className="rounded-lg bg-muted/40 p-2.5">
              <div className="mb-1 font-medium">{t("Credit")}</div>
              <div>{t("Owed")} <span className="num">{m(im.credit.balance_before)} → {m(im.credit.balance_after)}</span></div>
              {im.credit.due_after && im.credit.due_after !== im.credit.due_before && <div>{t("Due")} <span className="num">{im.credit.due_before} → {im.credit.due_after}</span></div>}
            </div>
          ) : im.performance ? <div className="rounded-lg bg-muted/40 p-2.5"><div className="mb-1 font-medium">{t("Performance")}</div><div>{t(im.performance)}</div></div> : null}
        </div>
      )}
    </div>
  );
}
