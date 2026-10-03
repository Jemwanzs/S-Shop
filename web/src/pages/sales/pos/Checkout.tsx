import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { CheckCircle2, Loader2, Pencil, ShieldCheck, Smartphone, Trash2, UserRound, X } from "lucide-react";
import { toast } from "sonner";
import { api, ApiError, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { count, money, phone as fmtPhone, todayIso, toNum } from "@/lib/format";
import type { Customer, SaleDetail } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Form";
import { Chip } from "@/components/Filters";
import { PointsPill } from "@/components/Badges";
import { exceedsMax, lineTotal, linePoints, marked, totals, type CartLine } from "./cart";

interface MpesaReq {
  id: string;
  status: "pending" | "success" | "failed" | "cancelled" | "timeout";
  result_desc: string | null;
  mpesa_receipt: string | null;
  amount: string;
}

export function CartLines({ lines, onEdit, onRemove }: { lines: CartLine[]; onEdit: (l: CartLine) => void; onRemove: (key: string) => void }) {
  const { profile, currency } = useSession();
  const s = profile!.settings;
  return (
    <ul className="divide-y">
      {lines.map((l) => {
        const disc = marked(l) - l.unitPrice;
        const pts = linePoints(l, s);
        return (
          <li key={l.key} className="flex gap-3 py-3">
            <div className="min-w-0 flex-1">
              <div className="truncate font-medium">{l.product.name}</div>
              <div className="num text-xs text-muted-foreground">
                {count(l.quantity)} × {money(l.unitPrice, currency)}
                {disc !== 0 && <span className={disc > 0 ? "text-destructive" : "text-success"}> ({disc > 0 ? "−" : "+"}{count(Math.abs(disc))})</span>}
              </div>
              <div className="mt-1 flex flex-wrap gap-1.5 text-[11px]">
                {l.barcode && <span className="num rounded bg-success/10 px-1.5 py-0.5 text-success">✓ {l.barcode}</span>}
                {pts > 0 && <span className="rounded bg-points/10 px-1.5 py-0.5 text-points">🌼 +{pts}</span>}
                {exceedsMax(l) && <span className="rounded bg-warning/15 px-1.5 py-0.5 text-warning">needs approval</span>}
              </div>
            </div>
            <div className="flex flex-col items-end justify-between">
              <span className="num font-semibold">{money(lineTotal(l), currency)}</span>
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

export function Checkout({ lines, onDone, clientRef }: { lines: CartLine[]; onDone: (sale: SaleDetail) => void; clientRef: string }) {
  const { profile, currency, can } = useSession();
  const s = profile!.settings;
  const methods = s.sales.payment_methods.filter((m) => m.enabled && (m.key !== "credit" || s.sales.credit_enabled));
  const [method, setMethod] = useState(methods[0]?.key ?? "cash");

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
  const [stk, setStk] = useState<MpesaReq | null>(null);
  const [supEmail, setSupEmail] = useState("");
  const [supPin, setSupPin] = useState("");
  const [needSupervisor, setNeedSupervisor] = useState(false);

  useEffect(() => {
    if (customer && !payPhone) setPayPhone(fmtPhone(customer.mobile));
  }, [customer, payPhone]);

  const redeemPts = Math.max(0, parseInt(redeem) || 0);
  const t = useMemo(() => totals(lines, s, redeemPts), [lines, s, redeemPts]);
  const excessive = lines.some(exceedsMax) && !can("sales.discount_override");
  const showSupervisor = excessive || needSupervisor;
  const canRedeem = s.loyalty.enabled && s.loyalty.redemption_enabled && can("customers.redeem_points") && !!customer && customer.points_available >= s.loyalty.min_redemption_points;

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

  const push = useMutation({
    mutationFn: () => api<{ id: string; message: string }>("/mpesa/stk", { body: { phone: payPhone, amount: t.payable, reference: profile!.tenant.name.slice(0, 12) } }),
    onSuccess: (r) => {
      setStk({ id: r.id, status: "pending", result_desc: r.message, mpesa_receipt: null, amount: String(t.payable) });
      toast.success(r.message);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const complete = useMutation({
    mutationFn: () =>
      api<SaleDetail>("/sales", {
        body: {
          customer_id: customer?.id,
          customer: !customer && digits.length >= 9 ? { mobile: digits, first_name: firstName, nickname } : undefined,
          items: lines.map((l) => ({ product_id: l.product.id, quantity: l.quantity, unit_price: l.unitPrice, barcode: l.barcode })),
          payment: {
            method,
            reference: method === "mpesa" && !stk ? reference : "",
            mpesa_request_id: method === "mpesa" && stk?.status === "success" ? stk.id : undefined,
            phone: payPhone.replace(/\D/g, ""),
          },
          redeem_points: canRedeem ? redeemPts : 0,
          due_date: method === "credit" ? dueDate : undefined,
          supervisor: showSupervisor && supEmail ? { email: supEmail, pin: supPin } : undefined,
          client_ref: clientRef,
        },
      }),
    onSuccess: onDone,
    onError: (e) => {
      if (e instanceof ApiError && /supervisor/i.test(e.message)) setNeedSupervisor(true);
      toast.error(errorMessage(e));
    },
  });

  const change = toNum(tendered) - t.payable;
  const blockers: string[] = [];
  if (!lines.length) blockers.push("Add items");
  if (method === "credit" && !customer && !(isNew && firstName.trim())) blockers.push("Credit needs a customer");
  if (isNew && digits.length >= 9 && !firstName.trim()) blockers.push("Enter the customer's first name");
  if (method === "mpesa" && stk?.status !== "success" && reference.trim().length < 8) blockers.push(s.sales.mpesa_manual_confirmation ? "Push STK or enter the M-Pesa code" : "Push STK to collect payment");
  if (showSupervisor && (!supEmail || !supPin)) blockers.push("Supervisor approval needed");
  if (redeemPts > 0 && customer && redeemPts > customer.points_available) blockers.push("Not enough points");

  return (
    <div className="space-y-5">
      {/* Customer */}
      <section className="space-y-2">
        <p className="label-caps">Customer {method === "credit" ? "" : "· optional"}</p>
        <div className="relative">
          <UserRound className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input inputMode="tel" placeholder="Mobile number, e.g. 0712 345 678" value={mobile} onChange={(e) => setMobile(e.target.value)} className="num pl-9 pr-9" />
          {mobile && <button className="absolute right-2 top-1/2 -translate-y-1/2 p-1.5 text-muted-foreground" onClick={() => { setMobile(""); setFirstName(""); setNickname(""); setRedeem(""); }} aria-label="Clear customer"><X className="h-4 w-4" /></button>}
        </div>
        {lookup.isFetching && <p className="text-xs text-muted-foreground">Checking customer book…</p>}
        {lookup.error && <p className="text-xs text-destructive">{errorMessage(lookup.error)}</p>}
        {customer && (
          <div className="flex items-center gap-3 rounded-xl bg-accent/60 p-3 animate-fade-up">
            <CheckCircle2 className="h-5 w-5 text-success" />
            <div className="min-w-0 flex-1">
              <div className="truncate font-medium">{customer.first_name} {customer.other_names}{customer.nickname && <span className="text-muted-foreground"> “{customer.nickname}”</span>}</div>
              <div className="text-xs text-muted-foreground">{customer.tier || "Member"} · {count(customer.purchase_count)} visits{toNum(customer.credit_balance) > 0 && <span className="text-destructive"> · owes {money(customer.credit_balance, currency)}</span>}</div>
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
        <p className="label-caps">Payment</p>
        <div className="flex flex-wrap gap-2">
          {methods.map((m) => (
            <Chip key={m.key} active={method === m.key} onClick={() => setMethod(m.key)} className="h-10">{m.label}</Chip>
          ))}
        </div>
        {method === "mpesa" && (
          <div className="space-y-3 rounded-xl border p-3">
            <div className="flex gap-2">
              <div className="relative flex-1">
                <Smartphone className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input inputMode="tel" placeholder="M-Pesa number (optional)" value={payPhone} onChange={(e) => setPayPhone(e.target.value)} className="num pl-9" />
              </div>
              {profile!.integrations.mpesa_stk && (
                <Button variant="success" disabled={push.isPending || payPhone.replace(/\D/g, "").length < 9 || t.payable <= 0 || stk?.status === "pending"} onClick={() => push.mutate()}>
                  {push.isPending ? <Loader2 className="animate-spin" /> : "Push STK"}
                </Button>
              )}
            </div>
            {stk && (
              <div className={cn("flex items-center gap-2 rounded-lg p-2.5 text-sm", stk.status === "success" ? "bg-success/10 text-success" : stk.status === "pending" ? "bg-muted" : "bg-destructive/10 text-destructive")}>
                {stk.status === "pending" ? <Loader2 className="h-4 w-4 animate-spin" /> : stk.status === "success" ? <CheckCircle2 className="h-4 w-4" /> : <X className="h-4 w-4" />}
                <span className="flex-1">
                  {stk.status === "pending" && "Waiting for the customer to enter their M-Pesa PIN…"}
                  {stk.status === "success" && <>Paid · <span className="num font-semibold">{stk.mpesa_receipt}</span></>}
                  {["failed", "cancelled", "timeout"].includes(stk.status) && (stk.result_desc || "Payment not completed")}
                </span>
                {stk.status !== "pending" && stk.status !== "success" && <button className="underline" onClick={() => setStk(null)}>Retry</button>}
              </div>
            )}
            {s.sales.mpesa_manual_confirmation && stk?.status !== "success" && (
              <Field label="M-Pesa confirmation code" hint="Or confirm manually from the M-Pesa message">
                <Input value={reference} onChange={(e) => setReference(e.target.value.toUpperCase())} placeholder="e.g. QFT1ABC2DE" className="num uppercase" />
              </Field>
            )}
          </div>
        )}
        {method === "cash" && (
          <div className="grid grid-cols-2 items-end gap-3">
            <Field label="Cash received" optional><Input inputMode="decimal" className="num" value={tendered} onChange={(e) => setTendered(e.target.value.replace(/[^\d.]/g, ""))} placeholder={String(t.payable)} /></Field>
            <div className="pb-2 text-right text-sm">
              {tendered && <>Change <span className={cn("num block text-lg font-semibold", change < 0 && "text-destructive")}>{money(change, currency)}</span></>}
            </div>
          </div>
        )}
        {method === "credit" && (
          <Field label="Due date" hint="Credit is recorded against the customer and tracked until paid">
            <Input type="date" min={todayIso()} value={dueDate} onChange={(e) => setDueDate(e.target.value)} />
          </Field>
        )}
      </section>

      {canRedeem && (
        <section className="space-y-2 rounded-xl border border-points/30 bg-points/5 p-3">
          <div className="flex items-center justify-between text-sm">
            <span className="font-medium">Redeem points</span>
            <button className="text-xs text-points underline" onClick={() => setRedeem(String(Math.min(customer!.points_available, Math.floor(t.net / Math.max(toNum(s.loyalty.point_value), 0.0001)))))}>Use max</button>
          </div>
          <Input inputMode="numeric" className="num" placeholder={`Up to ${count(customer!.points_available)} points`} value={redeem} onChange={(e) => setRedeem(e.target.value.replace(/\D/g, ""))} />
          {redeemPts > 0 && <p className="num text-xs text-muted-foreground">Worth {money(t.redeemValue, currency)}</p>}
        </section>
      )}

      {showSupervisor && (
        <section className="space-y-2 rounded-xl border border-warning/40 bg-warning/5 p-3">
          <p className="flex items-center gap-2 text-sm font-medium text-warning"><ShieldCheck className="h-4 w-4" /> Supervisor approval for discount</p>
          <div className="grid grid-cols-2 gap-2">
            <Input type="email" placeholder="Supervisor email" value={supEmail} onChange={(e) => setSupEmail(e.target.value)} />
            <Input type="password" inputMode="numeric" placeholder="PIN" value={supPin} onChange={(e) => setSupPin(e.target.value)} />
          </div>
        </section>
      )}

      {/* Totals */}
      <section className="space-y-1.5 rounded-xl bg-muted/50 p-4 text-sm">
        <Row label="Subtotal (marked)" value={money(t.gross, currency)} />
        {t.discount !== 0 && <Row label={t.discount > 0 ? "Discounts" : "Above marked price"} value={`${t.discount > 0 ? "−" : "+"}${money(Math.abs(t.discount), currency)}`} tone={t.discount > 0 ? "text-destructive" : "text-success"} />}
        {t.redeemValue > 0 && <Row label={`Points redeemed (${count(redeemPts)})`} value={`−${money(t.redeemValue, currency)}`} tone="text-points" />}
        <div className="flex items-baseline justify-between border-t pt-2">
          <span className="font-semibold">Total payable</span>
          <span className="num text-2xl font-bold">{money(t.payable, currency)}</span>
        </div>
        {s.loyalty.enabled && t.points > 0 && (
          <div className={cn("mt-2 flex items-center justify-center gap-2 rounded-lg py-2 font-semibold animate-pop", customer || isNew ? "bg-points/15 text-points" : "bg-muted text-muted-foreground")}>
            🌼 +{count(t.points)} Loyalty Points {!(customer || isNew) && <span className="text-xs font-normal">· add a customer to earn</span>}
          </div>
        )}
      </section>

      <Button size="lg" className="h-14 w-full text-base" disabled={blockers.length > 0 || complete.isPending} onClick={() => complete.mutate()}>
        {complete.isPending ? <Loader2 className="animate-spin" /> : <>Complete sale · <span className="num">{money(t.payable, currency)}</span></>}
      </Button>
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
