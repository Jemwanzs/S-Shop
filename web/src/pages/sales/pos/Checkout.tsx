import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { CheckCircle2, Pencil, ShieldCheck, Smartphone, Trash2, UserRound, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, ApiError, errorMessage } from "@/lib/api";
import { enqueueSale, offlineBlocker, type QueuedSale } from "@/lib/offline";
import { locationHeader } from "@/lib/location";
import { useSession } from "@/lib/session";
import { count, money, phone as fmtPhone, todayIso, toNum } from "@/lib/format";
import type { Customer, SaleDetail } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ActionButton, REASONS } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/PasswordInput";
import { Field, ToggleRow } from "@/components/Form";
import { Chip } from "@/components/Filters";
import { PointsPill } from "@/components/Badges";
import { exceedsMax, lineTotal, marked, salePoints, totals, type CartLine } from "./cart";
import { redemptionBlocked } from "@/lib/loyalty";
import { t as tr } from "@/lib/i18n";
import { SShopLoader } from "@/components/SShopLoader";

interface MpesaReq {
  id: string;
  status: "pending" | "success" | "failed" | "cancelled" | "timeout";
  result_desc: string | null;
  mpesa_receipt: string | null;
  amount: string;
}

/** Roadmap 91: a sale waiting for its STK Push — the server records it only once Safaricom confirms the payment. */
interface StkSale {
  id: string;
  status: "awaiting" | "completed" | "failed" | "cancelled" | "timeout" | "attention";
  amount: string;
  phone: string;
  error: string | null;
  request: MpesaReq | null;
  sale: SaleDetail | null;
}
const STK_LABEL: Record<StkSale["status"], string> = {
  awaiting: "Pending", completed: "Successful", failed: "Failed", cancelled: "Cancelled", timeout: "Timed out", attention: "Needs attention",
};

export function CartLines({ lines, onEdit, onRemove }: { lines: CartLine[]; onEdit: (l: CartLine) => void; onRemove: (key: string) => void }) {
  const { profile } = useSession();
  const s = profile!.settings;
  const net = lines.reduce((a, l) => a + lineTotal(l), 0);
  const earned = net >= toNum(s.loyalty.min_spend) ? salePoints(lines, s).perLine : new Map<string, number>();
  return (
    <ul className="divide-y">
      {lines.map((l) => {
        const disc = marked(l) - l.unitPrice;
        const pts = earned.get(l.key) ?? 0;
        return (
          <li key={l.key} className="flex gap-3 py-3">
            <div className="min-w-0 flex-1">
              <div className="truncate font-medium">{l.product.name}</div>
              <div className="num text-xs text-muted-foreground">
                {count(l.quantity)} × {money(l.unitPrice)}
                {disc !== 0 && <span className={disc > 0 ? "text-destructive" : "text-success"}> ({disc > 0 ? "−" : "+"}{count(Math.abs(disc))})</span>}
              </div>
              <div className="mt-1 flex flex-wrap gap-1.5 text-[11px]">
                {l.barcode && <span className="num rounded bg-success/10 px-1.5 py-0.5 text-success">✓ {l.barcode}</span>}
                {pts > 0 && <span className="rounded bg-points/10 px-1.5 py-0.5 text-points">🌼 +{pts}</span>}
                {exceedsMax(l) && <span className="rounded bg-warning/15 px-1.5 py-0.5 text-warning">needs approval</span>}
              </div>
            </div>
            <div className="flex flex-col items-end justify-between">
              <span className="num font-semibold">{money(lineTotal(l))}</span>
              <span className="flex gap-0.5">
                <button className="rounded p-1.5 text-muted-foreground hover:bg-muted" onClick={() => onEdit(l)} aria-label="Edit"><Pencil className="h-3.5 w-3.5" /></button>
                <button className="rounded p-1.5 text-muted-foreground hover:bg-muted hover:text-destructive" onClick={() => onRemove(l.key)} aria-label="Remove"><Trash2 className="h-3.5 w-3.5" /></button>
              </span>
            </div>
          </li>
        );
      })}
    </ul>
  );
}

export function Checkout({ lines, onDone, onQueued, clientRef, ownerId, onWaiting }: {
  lines: CartLine[]; onDone: (sale: SaleDetail) => void; onQueued: (sale: QueuedSale) => void; clientRef: string; ownerId?: string;
  /** A sale is waiting for its STK Push (the cart must not change). */
  onWaiting?: (waiting: boolean) => void;
}) {
  const { profile, can, branch } = useSession();
  const s = profile!.settings;
  const methods = s.sales.payment_methods.filter((m) => m.enabled && (m.key !== "credit" || s.sales.credit_enabled));
  const [method, setMethod] = useState(methods[0]?.key ?? "cash");
  // Roadmap 107: split payment — an amount per method (each once), the rest optionally left on credit.
  const splitMethods = methods.filter((m) => m.key !== "credit");
  const canSplit = splitMethods.length >= 2;
  const creditAllowed = methods.some((m) => m.key === "credit");
  const [splitAmt, setSplitAmt] = useState<Record<string, string>>({});
  const [splitCredit, setSplitCredit] = useState(false);

  // Customer
  const [mobile, setMobile] = useState("");
  const [firstName, setFirstName] = useState("");
  const [nickname, setNickname] = useState("");
  const digits = mobile.replace(/\D/g, "");
  const lookup = useQuery({
    queryKey: ["customer-lookup", digits],
    queryFn: () => api<{ mobile: string; customer: Customer | null }>("/customers/lookup", { query: { mobile: digits } }),
    enabled: digits.length >= 9,
    retry: false,
  });
  const customer = lookup.data?.customer ?? null;
  const isNew = !!lookup.data && !customer;

  // Payment details
  const [reference, setReference] = useState("");
  const [payPhone, setPayPhone] = useState("");
  const [tendered, setTendered] = useState("");
  const [dueDate, setDueDate] = useState(() => {
    const d = new Date();
    d.setDate(d.getDate() + s.sales.credit_default_days);
    return d.toISOString().slice(0, 10);
  });
  const [redeem, setRedeem] = useState("");
  // Optional deposit on a credit sale: collected now with the deposit method, the rest stays on credit.
  const depositMethods = methods.filter((m) => m.key !== "credit");
  const [deposit, setDeposit] = useState("");
  const [depositMethod, setDepositMethod] = useState(depositMethods[0]?.key ?? "cash");
  const [stk, setStk] = useState<MpesaReq | null>(null);
  // Push STK: only with the business's verified M-Pesa account serving this branch (roadmap 91).
  const stkBranches = profile?.integrations.mpesa_stk_branches;
  const stkReady = !!profile?.integrations.mpesa_stk && (!stkBranches || stkBranches.includes(branch?.id ?? ""));
  const manualMpesa = s.sales.mpesa_manual_confirmation;
  const [mpesaMode, setMpesaMode] = useState<"stk" | "manual">(stkReady ? "stk" : "manual");
  const [stkSale, setStkSale] = useState<StkSale | null>(null);
  const [supEmail, setSupEmail] = useState("");
  const [supPin, setSupPin] = useState("");
  const [needSupervisor, setNeedSupervisor] = useState(false);

  useEffect(() => {
    if (customer && !payPhone) setPayPhone(fmtPhone(customer.mobile));
  }, [customer, payPhone]);

  const redeemPts = Math.max(0, parseInt(redeem) || 0);
  const t = useMemo(() => totals(lines, s, redeemPts), [lines, s, redeemPts]);
  const isCredit = method === "credit";
  const isSplit = method === "split";
  const round2 = (n: number) => Math.round(n * 100) / 100;
  const splitParts = isSplit ? splitMethods.map((m) => ({ method: m.key, amount: round2(toNum(splitAmt[m.key])) })).filter((p) => p.amount > 0) : [];
  const allocated = round2(splitParts.reduce((a, p) => a + p.amount, 0));
  const remaining = round2(t.payable - allocated);
  const splitOnCredit = isSplit && splitCredit && creditAllowed && remaining > 0;
  const splitPart = (k: string) => splitParts.find((p) => p.method === k)?.amount ?? 0;
  const depositAmount = isCredit ? toNum(deposit) : 0;
  const takingDeposit = depositAmount > 0;
  // What is collected now, and how: the full payable, or the deposit on a credit sale.
  // What is collected through the M-Pesa / cash boxes: in a split, the M-Pesa part (or else the cash part).
  const payMethod = isSplit ? (splitPart("mpesa") > 0 ? "mpesa" : splitPart("cash") > 0 ? "cash" : "") : isCredit ? depositMethod : method;
  const payAmount = isSplit ? splitPart(payMethod) : isCredit ? depositAmount : t.payable;
  const collecting = isSplit ? !!payMethod : !isCredit || takingDeposit;
  // A full M-Pesa sale by STK Push: sent with the cart, completed by the server once paid.
  const stkMode = method === "mpesa" && stkReady && (mpesaMode === "stk" || !manualMpesa);
  const stkWaiting = stkSale?.status === "awaiting";
  const excessive = lines.some(exceedsMax) && !can("sales.discount_override");
  // Roadmap 106: redemption window, minimum, 50 % cap and (optionally) a supervisor — the server enforces each.
  const redeemBlocked = redemptionBlocked(s.loyalty);
  const redeemApproval = redeemPts > 0 && s.loyalty.redemption_needs_approval && !can("loyalty.manage");
  const showSupervisor = excessive || needSupervisor || redeemApproval;
  const canRedeem = !redeemBlocked && can("customers.redeem_points") && !!customer && customer.points_available >= s.loyalty.min_redemption_points;
  const showPoints = s.loyalty.enabled && !!customer && customer.points_available > 0 && can("customers.redeem_points");

  // Poll STK status until it settles (live SSE events also refresh it).
  const stkStatus = useQuery({
    queryKey: ["mpesa", stk?.id],
    queryFn: () => api<MpesaReq>(`/mpesa/stk/${stk!.id}`),
    enabled: !!stk && stk.status === "pending",
    refetchInterval: 3000,
  });
  useEffect(() => {
    if (stkStatus.data) setStk(stkStatus.data);
  }, [stkStatus.data]);

  const stkPoll = useQuery({
    queryKey: ["stk-sale", stkSale?.id],
    queryFn: () => api<StkSale>(`/sales/stk/${stkSale!.id}`),
    enabled: !!stkSale && stkSale.status === "awaiting",
    refetchInterval: 3000,
  });
  useEffect(() => {
    if (stkPoll.data) setStkSale(stkPoll.data);
  }, [stkPoll.data]);
  useEffect(() => {
    onWaiting?.(stkWaiting);
  }, [stkWaiting, onWaiting]);
  useEffect(() => () => onWaiting?.(false), [onWaiting]);
  useEffect(() => {
    if (stkSale?.status === "completed" && stkSale.sale) {
      const sale = stkSale.sale;
      setStkSale(null);
      onWaiting?.(false);
      onDone(sale);
    }
  }, [stkSale, onDone, onWaiting]);

  const push = useMutation({
    mutationFn: () => api<{ id: string; message: string }>("/mpesa/stk", { body: { phone: payPhone, amount: payAmount, reference: profile!.tenant.name.slice(0, 12) } }),
    onSuccess: (r) => {
      setStk({ id: r.id, status: "pending", result_desc: r.message, mpesa_receipt: null, amount: String(payAmount) });
      toast.success(r.message);
    },
    onError: (e) => toast.error(e),
  });

  // Offline: a sale that needs no live check is kept on this device and synced later (same client_ref).
  const queueOffline = async (body: Record<string, unknown>): Promise<{ queued: QueuedSale }> => {
    const blocker = offlineBlocker({
      method,
      stk: !!stk,
      customerNew: isNew && digits.length >= 9,
      redeem: redeemPts,
      deposit: takingDeposit,
      supervisor: showSupervisor,
      tracked: lines.some((l) => l.product.track_items),
      clearance: lines.some((l) => !!l.barcode),
    });
    if (blocker) throw new ApiError(blocker, 0, "offline", "You're offline");
    const queued: QueuedSale = {
      client_ref: clientRef,
      tenant_id: profile!.tenant.id,
      branch_id: branch!.id,
      user_id: profile!.user.id,
      sold_at: new Date().toISOString(),
      body: { ...body, offline_at: new Date().toISOString() },
      location: locationHeader(),
      total: t.payable,
      items: lines.reduce((a, l) => a + l.quantity, 0),
      status: "pending",
    };
    await enqueueSale(queued);
    return { queued };
  };
  const saleBody = () => ({
          customer_id: customer?.id,
          customer: !customer && digits.length >= 9 ? { mobile: digits, first_name: firstName, nickname } : undefined,
          items: lines.map((l) => ({ product_id: l.product.id, quantity: l.quantity, unit_price: l.unitPrice, barcode: l.barcode })),
          payment: {
            method,
            reference: method === "mpesa" && !stk ? reference : "",
            mpesa_request_id: method === "mpesa" && stk?.status === "success" ? stk.id : undefined,
            phone: payPhone.replace(/\D/g, ""),
          },
          payments: isSplit
            ? splitParts.map((p) => ({
                method: p.method,
                amount: p.amount,
                reference: p.method === "mpesa" && !stk ? reference : "",
                mpesa_request_id: p.method === "mpesa" && stk?.status === "success" ? stk.id : undefined,
              }))
            : undefined,
          on_credit: splitOnCredit || undefined,
          deposit: takingDeposit
            ? {
                amount: depositAmount,
                method: depositMethod,
                reference: depositMethod === "mpesa" && !stk ? reference : "",
                mpesa_request_id: depositMethod === "mpesa" && stk?.status === "success" ? stk.id : undefined,
              }
            : undefined,
          redeem_points: canRedeem ? redeemPts : 0,
          due_date: isCredit || splitOnCredit ? dueDate : undefined,
          supervisor: showSupervisor && supEmail ? { email: supEmail, pin: supPin } : undefined,
          client_ref: clientRef,
          owner_id: ownerId,
  });
  const complete = useMutation({
    mutationFn: async (): Promise<SaleDetail | { queued: QueuedSale }> => {
      const body = saleBody();
      if (!navigator.onLine) return queueOffline(body);
      try {
        return await api<SaleDetail>("/sales", { body });
      } catch (e) {
        if (e instanceof ApiError && e.code === "network") return queueOffline(body);
        throw e;
      }
    },
    onSuccess: (r) => ("queued" in r ? onQueued(r.queued) : onDone(r)),
    onError: (e) => {
      if (e instanceof ApiError && /supervisor/i.test(e.message)) setNeedSupervisor(true);
      toast.error(e);
    },
  });
  // STK Push sale: validated in full and the stock held by the server, then the prompt goes to the customer.
  const sendStk = useMutation({
    mutationFn: () => api<StkSale | { status: "completed"; sale: SaleDetail }>("/sales/stk", {
      body: { phone: payPhone, sale: { ...saleBody(), payment: { method: "mpesa", reference: "", phone: payPhone.replace(/\D/g, "") } } },
    }),
    onSuccess: (r) => {
      if (r.status === "completed" && r.sale && !("request" in r)) return onDone(r.sale);
      setStkSale(r as StkSale);
      toast.success("Prompt sent — the customer enters their M-Pesa PIN");
    },
    onError: (e) => {
      if (e instanceof ApiError && /supervisor/i.test(e.message)) setNeedSupervisor(true);
      toast.error(e);
    },
  });
  const stopStk = useMutation({
    mutationFn: () => api<StkSale>(`/sales/stk/${stkSale!.id}/cancel`, { method: "POST" }),
    onSuccess: (r) => setStkSale(r),
    onError: (e) => toast.error(e),
  });

  const change = toNum(tendered) - payAmount;
  const blockers: string[] = [];
  if (!lines.length) blockers.push("Add items");
  if ((method === "credit" || splitOnCredit) && !customer && !(isNew && firstName.trim())) blockers.push("Credit needs a customer");
  if (isSplit && !splitParts.length) blockers.push("Enter how much is paid by each method");
  if (isSplit && remaining < 0) blockers.push("The payments add up to more than the total");
  if (isSplit && remaining > 0 && !splitOnCredit) blockers.push(creditAllowed ? "Allocate the rest, or leave it on credit" : "Allocate the rest of the payment");
  if (isNew && digits.length >= 9 && !firstName.trim()) blockers.push("Enter the customer's first name");
  if (takingDeposit && depositAmount >= t.payable) blockers.push("A deposit must be less than the total");
  if (collecting && payMethod === "mpesa" && stk?.status === "pending") blockers.push("Waiting for the M-Pesa payment");
  if (collecting && payMethod === "mpesa" && !stkMode && !s.sales.mpesa_manual_confirmation && stk?.status !== "success")
    blockers.push(stkReady ? "Push STK to collect payment" : "M-Pesa needs Push STK, which is not configured");
  if (collecting && payMethod === "mpesa" && reference && (reference.length < 8 || reference.length > 12)) blockers.push("Check the M-Pesa code (8–12 characters) or clear it");
  if (showSupervisor && (!supEmail || !supPin)) blockers.push("Supervisor approval needed");
  if (stkMode && payPhone.replace(/\D/g, "").length < 9) blockers.push("Enter the customer's M-Pesa number");
  if (stkWaiting) blockers.push("Waiting for the M-Pesa payment");
  if (redeemPts > 0 && customer && redeemPts > customer.points_available) blockers.push("Not enough points");
  if (redeemPts > 0 && t.redeemValue > t.cap.value) blockers.push(`Points can pay at most ${t.cap.pct}% of this sale`);

  return (
    <div className="space-y-5">
      {/* Customer */}
      <section className="space-y-2">
        <p className="label-caps">{tr("Customer")} {method === "credit" || splitOnCredit ? "" : "· optional"}</p>
        <div className="relative">
          <UserRound className="absolute inset-s-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input inputMode="tel" placeholder="Mobile number, e.g. 0712 345 678" value={mobile} onChange={(e) => setMobile(e.target.value)} className="num ps-9 pe-9" />
          {mobile && <button className="absolute inset-e-2 top-1/2 -translate-y-1/2 p-1.5 text-muted-foreground" onClick={() => { setMobile(""); setFirstName(""); setNickname(""); setRedeem(""); }} aria-label="Clear customer"><X className="h-4 w-4" /></button>}
        </div>
        {lookup.isFetching && <p className="text-xs text-muted-foreground">Checking customer book…</p>}
        {lookup.error && <p className="text-xs text-destructive">{errorMessage(lookup.error)}</p>}
        {customer && (
          <div className="flex items-center gap-3 rounded-xl bg-accent/60 p-3 animate-fade-up">
            <CheckCircle2 className="h-5 w-5 text-success" />
            <div className="min-w-0 flex-1">
              <div className="truncate font-medium">{customer.first_name} {customer.other_names}{customer.nickname && <span className="text-muted-foreground"> “{customer.nickname}”</span>}</div>
              <div className="text-xs text-muted-foreground">{customer.tier || "Member"} · {count(customer.purchase_count)} visits{toNum(customer.credit_balance) > 0 && <span className="text-destructive"> · owes {money(customer.credit_balance)}</span>}</div>
            </div>
            <PointsPill own={customer.points_available} />
          </div>
        )}
        {isNew && (
          <div className="grid grid-cols-2 gap-2 animate-fade-up">
            <Field label="First name"><Input value={firstName} onChange={(e) => setFirstName(e.target.value)} autoFocus /></Field>
            <Field label="Nickname" optional><Input value={nickname} onChange={(e) => setNickname(e.target.value)} placeholder="Optional" /></Field>
          </div>
        )}
      </section>

      {/* Payment */}
      <section className="space-y-3">
        <p className="label-caps">{tr("Payment")}</p>
        <div className="flex flex-wrap gap-2">
          {methods.map((m) => (
            <Chip key={m.key} active={method === m.key} onClick={() => setMethod(m.key)} className="h-10">{m.label}</Chip>
          ))}
          {canSplit && <Chip active={isSplit} onClick={() => setMethod("split")} className="h-10">{tr("Split payment")}</Chip>}
        </div>
        {isSplit && (
          <div className="space-y-2 rounded-xl border p-3">
            {splitMethods.map((m) => (
              <div key={m.key} className="flex items-center gap-2">
                <span className="w-28 shrink-0 truncate text-sm">{m.label}</span>
                <Input inputMode="decimal" className="num" placeholder="0" value={splitAmt[m.key] ?? ""} aria-label={m.label}
                  disabled={m.key === "mpesa" && (stk?.status === "pending" || stk?.status === "success")}
                  onChange={(e) => setSplitAmt({ ...splitAmt, [m.key]: e.target.value.replace(/[^\d.]/g, "") })} />
                <button type="button" className="shrink-0 text-xs text-primary underline disabled:opacity-40" disabled={remaining <= 0 || (m.key === "mpesa" && !!stk)}
                  onClick={() => setSplitAmt({ ...splitAmt, [m.key]: String(round2(toNum(splitAmt[m.key]) + remaining)) })}>{tr("Rest")}</button>
              </div>
            ))}
            <div className="grid grid-cols-3 gap-2 rounded-lg bg-muted/50 p-2 text-center text-xs">
              <div><p className="text-muted-foreground">{tr("Total payable")}</p><p className="num text-sm font-semibold">{money(t.payable)}</p></div>
              <div><p className="text-muted-foreground">{tr("Allocated")}</p><p className="num text-sm font-semibold">{money(allocated)}</p></div>
              <div><p className="text-muted-foreground">{tr("Remaining")}</p><p className={cn("num text-sm font-semibold", remaining < 0 ? "text-destructive" : remaining > 0 ? "text-warning" : "text-success")}>{money(remaining)}</p></div>
            </div>
            {creditAllowed && (
              <ToggleRow label="Leave the rest on credit" hint="Needs a customer; the balance is tracked in Credit Sales until paid" checked={splitCredit} onChange={setSplitCredit} />
            )}
            {splitOnCredit && (
              <Field label="Due date"><Input type="date" min={todayIso()} value={dueDate} onChange={(e) => setDueDate(e.target.value)} /></Field>
            )}
          </div>
        )}
        {isCredit && (
          <div className="space-y-3 rounded-xl border p-3">
            <Field label="Due date" hint="Credit is recorded against the customer and tracked until paid">
              <Input type="date" min={todayIso()} value={dueDate} onChange={(e) => setDueDate(e.target.value)} />
            </Field>
            <Field label="Deposit now" optional hint="Part-payment today; the balance stays on credit">
              <Input inputMode="decimal" className="num" value={deposit} onChange={(e) => setDeposit(e.target.value.replace(/[^\d.]/g, ""))} placeholder="0" />
            </Field>
            {takingDeposit && (
              <div className="flex flex-wrap gap-2">
                {depositMethods.map((m) => (
                  <Chip key={m.key} active={depositMethod === m.key} onClick={() => setDepositMethod(m.key)} className="h-9">{m.label}</Chip>
                ))}
              </div>
            )}
          </div>
        )}
        {method === "mpesa" && stkReady && manualMpesa && (
          <div className="grid grid-cols-2 gap-1 rounded-xl bg-muted p-1" role="radiogroup" aria-label={tr("M-Pesa payment")}>
            {([["manual", "Manual M-PESA"], ["stk", "STK Push"]] as const).map(([k, label]) => (
              <button key={k} type="button" role="radio" aria-checked={mpesaMode === k} disabled={stkWaiting} onClick={() => setMpesaMode(k)}
                className={cn("h-9 rounded-lg text-sm font-medium transition-colors", mpesaMode === k ? "bg-card shadow-xs" : "text-muted-foreground")}>{tr(label)}</button>
            ))}
          </div>
        )}
        {stkMode && (
          <div className="space-y-3 rounded-xl border p-3">
            <Field label="Customer phone number" hint="07XXXXXXXX, 01XXXXXXXX or 254XXXXXXXXX">
              <div className="relative">
                <Smartphone className="absolute inset-s-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input inputMode="tel" placeholder="07XXXXXXXX" value={payPhone} onChange={(e) => setPayPhone(e.target.value)} className="num ps-9" disabled={stkWaiting} />
              </div>
            </Field>
            <div className="flex items-baseline justify-between text-sm">
              <span className="text-muted-foreground">{tr("Amount")}</span>
              <span className="num text-lg font-semibold">{money(stkSale?.amount ?? t.payable)}</span>
            </div>
            {stkSale && (
              <div className={cn("flex items-start gap-2 rounded-lg p-2.5 text-sm",
                stkSale.status === "awaiting" ? "bg-muted" : stkSale.status === "attention" ? "bg-warning/10 text-warning" : "bg-destructive/10 text-destructive")} role="status" aria-live="polite">
                {stkSale.status === "awaiting" ? <SShopLoader size="sm" className="mt-0.5" /> : <X className="mt-0.5 h-4 w-4 shrink-0" />}
                <span className="min-w-0 flex-1">
                  <b>{tr(STK_LABEL[stkSale.status])}</b>
                  {" · "}
                  {stkSale.status === "awaiting" && tr("Waiting for the customer to enter their M-Pesa PIN… The sale completes by itself once paid.")}
                  {["failed", "cancelled", "timeout"].includes(stkSale.status) && (stkSale.request?.result_desc || stkSale.error || tr("Payment not completed"))}
                  {stkSale.status === "attention" && <>{stkSale.error} {tr("— see Settings → M-Pesa & WhatsApp → Activity.")}</>}
                </span>
                {stkSale.status === "awaiting"
                  ? <button className="shrink-0 underline" disabled={stopStk.isPending} onClick={() => stopStk.mutate()}>{tr("Stop waiting")}</button>
                  : stkSale.status !== "attention" && <button className="shrink-0 underline" onClick={() => setStkSale(null)}>{tr("Retry")}</button>}
              </div>
            )}
          </div>
        )}
        {collecting && payMethod === "mpesa" && !stkMode && (
          <div className="space-y-3 rounded-xl border p-3">
            {/* Number: optional for a manual M-Pesa sale, required only to push an STK prompt. */}
            <Field label="M-Pesa number" optional hint={stkReady ? "Needed only for Push STK" : undefined}>
              <div className="flex gap-2">
                <div className="relative flex-1">
                  <Smartphone className="absolute inset-s-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input inputMode="tel" placeholder="07XXXXXXXX" value={payPhone} onChange={(e) => setPayPhone(e.target.value)} className="num ps-9" disabled={stk?.status === "pending"} />
                </div>
                <ActionButton
                  type="button"
                  variant="success"
                  online
                  busy={push.isPending}
                  blockedBy={[
                    !stkReady && REASONS.stkNotConfigured,
                    stk?.status === "pending" && "Waiting…",
                    stk?.status === "success" && "Paid",
                    payPhone.replace(/\D/g, "").length < 9 && "Enter number",
                    payAmount <= 0 && "Nothing to pay",
                  ]}
                  onAction={() => push.mutateAsync()}
                >
                  Push STK
                </ActionButton>
              </div>
            </Field>
            {!stkReady && <p className="-mt-1 text-xs text-muted-foreground">{tr("STK not configured — record the M-Pesa payment manually.")}</p>}
            {stk && (
              <div className={cn("flex items-center gap-2 rounded-lg p-2.5 text-sm", stk.status === "success" ? "bg-success/10 text-success" : stk.status === "pending" ? "bg-muted" : "bg-destructive/10 text-destructive")}>
                {stk.status === "pending" ? <SShopLoader size="sm" /> : stk.status === "success" ? <CheckCircle2 className="h-4 w-4" /> : <X className="h-4 w-4" />}
                <span className="flex-1">
                  {stk.status === "pending" && tr("Waiting for the customer to enter their M-Pesa PIN…")}
                  {stk.status === "success" && <>{tr("Paid")} · <span className="num font-semibold">{stk.mpesa_receipt}</span></>}
                  {["failed", "cancelled", "timeout"].includes(stk.status) && (stk.result_desc || tr("Payment not completed"))}
                </span>
                {stk.status !== "pending" && stk.status !== "success" && <button className="underline" onClick={() => setStk(null)}>{tr("Retry")}</button>}
              </div>
            )}
            {/* Manual M-Pesa: the confirmation code is optional. */}
            {s.sales.mpesa_manual_confirmation && stk?.status !== "success" && stk?.status !== "pending" && (
              <Field label="M-Pesa confirmation code" optional hint="From the customer's M-Pesa message, if available">
                <Input value={reference} onChange={(e) => setReference(e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ""))} placeholder="e.g. QFT1ABC2DE" className="num uppercase placeholder:normal-case" maxLength={12} />
              </Field>
            )}
          </div>
        )}
        {collecting && payMethod === "cash" && (
          <div className="grid grid-cols-2 items-end gap-3">
            <Field label="Cash received" optional><Input inputMode="decimal" className="num" value={tendered} onChange={(e) => setTendered(e.target.value.replace(/[^\d.]/g, ""))} placeholder={String(payAmount)} /></Field>
            <div className="pb-2 text-end text-sm">
              {tendered && <>{tr("Change")} <span className={cn("num block text-lg font-semibold", change < 0 && "text-destructive")}>{money(change)}</span></>}
            </div>
          </div>
        )}
      </section>

      {canRedeem && (
        <section className="space-y-2 rounded-xl border border-points/30 bg-points/5 p-3">
          <div className="flex items-center justify-between text-sm">
            <span className="font-medium">{tr("Redeem points")}</span>
            <button className="text-xs text-points underline" onClick={() => setRedeem(String(Math.min(customer!.points_available, t.cap.points)))}>{tr("Use max")}</button>
          </div>
          <Input inputMode="numeric" className="num" placeholder={`Up to ${count(Math.min(customer!.points_available, t.cap.points))} points`} value={redeem} onChange={(e) => setRedeem(e.target.value.replace(/\D/g, ""))} />
          <p className="num text-xs text-muted-foreground">
            {redeemPts > 0 && <>{tr("Worth")} {money(t.redeemValue)} · </>}{tr("Points can pay up to")} {t.cap.pct}% ({money(t.cap.value)})
          </p>
        </section>
      )}
      {!canRedeem && showPoints && (
        <p className="rounded-xl border border-points/30 bg-points/5 p-3 text-xs text-muted-foreground">
          <span className="num font-medium text-points">{count(customer!.points_available)} {tr("points")}</span> · {tr(redeemBlocked ?? `A minimum balance of ${s.loyalty.min_redemption_points} points is required to redeem`)}
        </p>
      )}

      {showSupervisor && (
        <section className="space-y-2 rounded-xl border border-warning/40 bg-warning/5 p-3">
          <p className="flex items-center gap-2 text-sm font-medium text-warning"><ShieldCheck className="h-4 w-4" /> {tr(redeemApproval && !excessive ? "Supervisor approval for points redemption" : "Supervisor approval for discount")}</p>
          <div className="grid grid-cols-2 gap-2">
            <Input type="email" placeholder="Supervisor email" value={supEmail} onChange={(e) => setSupEmail(e.target.value)} />
            <PasswordInput placeholder="PIN" autoComplete="off" value={supPin} onChange={(e) => setSupPin(e.target.value)} />
          </div>
        </section>
      )}

      {/* Totals */}
      <section className="space-y-1.5 rounded-xl bg-muted/50 p-4 text-sm">
        <Row label="Subtotal (marked)" value={money(t.gross)} />
        {t.discount !== 0 && <Row label={t.discount > 0 ? "Discounts" : "Above marked price"} value={`${t.discount > 0 ? "−" : "+"}${money(Math.abs(t.discount))}`} tone={t.discount > 0 ? "text-destructive" : "text-success"} />}
        {t.redeemValue > 0 && <Row label={`Points redeemed (${count(redeemPts)})`} value={`−${money(t.redeemValue)}`} tone="text-points" />}
        <div className="flex items-baseline justify-between border-t pt-2">
          <span className="font-semibold">{tr("Total payable")}</span>
          <span className="num text-2xl font-bold">{money(t.payable)}</span>
        </div>
        {isCredit && (
          <>
            {takingDeposit && <Row label="Deposit now" value={money(depositAmount)} tone="text-success" />}
            <Row label="On credit" value={money(Math.max(t.payable - depositAmount, 0))} tone="text-destructive" />
          </>
        )}
        {s.loyalty.enabled && t.points > 0 && (
          <div className={cn("mt-2 flex items-center justify-center gap-2 rounded-lg py-2 font-semibold animate-pop", customer || isNew ? "bg-points/15 text-points" : "bg-muted text-muted-foreground")}>
            🌼 +{count(t.points)} Loyalty Points {!(customer || isNew) && <span className="text-xs font-normal">· add a customer to earn</span>}
          </div>
        )}
      </section>

      {/* Offline sales are queued on the device (cash-style only), so this action does not require a connection. */}
      {stkMode ? (
        <ActionButton size="lg" variant="success" className="h-14 w-full text-base" online busy={sendStk.isPending || stkWaiting} busyLabel={stkWaiting ? "Waiting for payment…" : "Sending…"}
          blockedBy={[!lines.length && REASONS.emptyCart, blockers.length > 0 && lines.length > 0 && !stkWaiting && "Complete the details below"]}
          onAction={() => sendStk.mutateAsync()}>
          <>{tr("Send STK Push")} · <span className="num">{money(t.payable)}</span></>
        </ActionButton>
      ) : (
        <ActionButton size="lg" className="h-14 w-full text-base" busy={complete.isPending} busyLabel="Processing…"
          blockedBy={[!lines.length && REASONS.emptyCart, blockers.length > 0 && lines.length > 0 && "Complete the details below"]}
          onAction={() => complete.mutateAsync()}>
          <>Complete sale · <span className="num">{money(t.payable)}</span></>
        </ActionButton>
      )}
      {blockers.length > 0 && lines.length > 0 && <p className="text-center text-xs text-muted-foreground">{blockers[0]}</p>}
    </div>
  );
}

function Row({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <div className="flex justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className={cn("num", tone)}>{value}</span>
    </div>
  );
}
