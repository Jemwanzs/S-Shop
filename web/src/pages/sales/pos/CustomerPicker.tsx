/** Roadmap 184: the till's customer field — live search by any part of the name or the mobile number (07…, 01…, 254…,
 * +254…) from the first letter or digit, a short list of matches to pick from, and *Add new customer* when nothing
 * matches. Only a picked customer is attached to the sale: typed text never is. People who cannot create customers keep
 * the earlier way — the customer is created with the sale from a full mobile number and a first name. */
import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, Plus, UserRound, X } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { useDebounced } from "@/lib/hooks";
import { count, initials, money, phone as fmtPhone, toNum } from "@/lib/format";
import type { Customer } from "@/lib/types";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/Form";
import { PointsPill } from "@/components/Badges";
import { CustomerForm } from "@/pages/customers/CustomerForm";

/** A customer created together with the sale (no `customers.create`): the earlier inline way. */
export interface NewCustomerDraft { mobile: string; first_name: string; nickname: string }

const PHONE = /^[\d+\s\-()]+$/;
const fullName = (c: Customer) => `${c.first_name} ${c.other_names}`.trim();

export function CustomerPicker({ customer, onChange, draft, onDraft }: {
  customer: Customer | null;
  onChange: (c: Customer | null) => void;
  draft: NewCustomerDraft | null;
  onDraft: (d: NewCustomerDraft | null) => void;
}) {
  const { can } = useSession();
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [form, setForm] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const dq = useDebounced(q.trim(), 200);
  const search = useQuery({
    queryKey: ["customer-search", dq],
    queryFn: () => api<{ q: string; items: Customer[]; more: boolean }>("/customers/search", { query: { q: dq } }),
    enabled: !!dq && !customer && !draft,
    staleTime: 30_000,
    retry: false,
  });
  // Only the answer to what is typed now counts (an older, slower answer never replaces it).
  const current = !!dq && dq === q.trim() && search.data?.q === dq && !search.isFetching;
  const items = current ? search.data!.items : [];
  const nothing = current && items.length === 0;
  const phoneLike = PHONE.test(q.trim());
  const digits = q.replace(/\D/g, "");
  useEffect(() => setActive(0), [dq]);
  useEffect(() => {
    const close = (e: PointerEvent) => { if (box.current && !box.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);

  const pick = (c: Customer) => {
    onChange(c);
    setOpen(false);
    setQ("");
  };
  const clear = () => {
    onChange(null);
    onDraft(null);
    setQ("");
    setTimeout(() => input.current?.focus(), 0);
  };
  const canCreate = can("customers.create");
  const add = () => {
    if (canCreate) setForm(true);
    else if (phoneLike && digits.length >= 9) { onDraft({ mobile: digits, first_name: "", nickname: "" }); setOpen(false); }
  };
  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") { e.preventDefault(); setOpen(true); setActive((a) => Math.min(a + 1, Math.max(items.length - 1, 0))); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setActive((a) => Math.max(a - 1, 0)); }
    else if (e.key === "Enter" && open && items[active]) { e.preventDefault(); pick(items[active]); }
    else if (e.key === "Escape") setOpen(false);
  };

  if (customer) {
    return (
      <div className="flex items-center gap-3 rounded-xl bg-accent/60 p-3 animate-fade-up" data-testid="sale-customer">
        <CheckCircle2 className="h-5 w-5 shrink-0 text-success" />
        <div className="min-w-0 flex-1">
          <div className="truncate font-medium">{fullName(customer)}{customer.nickname && <span className="text-muted-foreground"> “{customer.nickname}”</span>}</div>
          <div className="truncate text-xs text-muted-foreground">
            <span className="num">{fmtPhone(customer.mobile)}</span> · {customer.tier || "Member"} · {count(customer.purchase_count)} visits
            {toNum(customer.credit_balance) > 0 && <span className="text-destructive"> · owes {money(customer.credit_balance)}</span>}
          </div>
        </div>
        <PointsPill own={customer.points_available} />
        <button className="shrink-0 p-1.5 text-muted-foreground" onClick={clear} aria-label={t("Remove customer")}><X className="h-4 w-4" /></button>
      </div>
    );
  }
  if (draft) {
    return (
      <div className="space-y-2 rounded-xl border p-3 animate-fade-up">
        <div className="flex items-center gap-2 text-sm">
          <UserRound className="h-4 w-4 text-primary" />
          <span className="flex-1">{t("New customer")} · <span className="num">{fmtPhone(draft.mobile)}</span></span>
          <button className="p-1 text-muted-foreground" onClick={clear} aria-label={t("Remove customer")}><X className="h-4 w-4" /></button>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <Field label="First name"><Input value={draft.first_name} onChange={(e) => onDraft({ ...draft, first_name: e.target.value })} autoFocus /></Field>
          <Field label="Nickname" optional><Input value={draft.nickname} onChange={(e) => onDraft({ ...draft, nickname: e.target.value })} placeholder="Optional" /></Field>
        </div>
      </div>
    );
  }

  const names = q.trim().split(/\s+/);
  return (
    <div ref={box} className="relative">
      <UserRound className="pointer-events-none absolute inset-s-3 top-[1.15rem] h-4 w-4 -translate-y-1/2 text-muted-foreground" />
      <Input ref={input} value={q} onChange={(e) => { setQ(e.target.value); setOpen(true); }} onFocus={() => setOpen(true)} onKeyDown={onKey}
        placeholder={t("Search customer by name or mobile number")} className="ps-9 pe-9" autoComplete="off" enterKeyHint="search"
        role="combobox" aria-expanded={open && !!q.trim()} aria-controls="sale-customer-results" aria-autocomplete="list" />
      {q && <button className="absolute inset-e-2 top-[1.15rem] -translate-y-1/2 p-1.5 text-muted-foreground" onClick={clear} aria-label={t("Clear customer search")}><X className="h-4 w-4" /></button>}
      {open && q.trim() && (
        <div id="sale-customer-results" role="listbox" className="absolute inset-x-0 top-full z-30 mt-1 max-h-72 overflow-y-auto rounded-xl border bg-popover p-1 shadow-lg animate-fade-up">
          {search.isError && <p className="p-2.5 text-xs text-destructive">{errorMessage(search.error)}</p>}
          {!current && !search.isError && <p className="p-2.5 text-xs text-muted-foreground">{t("Searching customers…")}</p>}
          {items.map((c, i) => (
            <button key={c.id} role="option" aria-selected={i === active} onPointerDown={(e) => e.preventDefault()} onClick={() => pick(c)} onMouseEnter={() => setActive(i)}
              className={cn("flex w-full items-center gap-2.5 rounded-lg p-2 text-start", i === active && "bg-accent")}>
              <span className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-primary/10 text-xs font-semibold text-primary">{initials(fullName(c))}</span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">{fullName(c)}{c.nickname && <span className="font-normal text-muted-foreground"> “{c.nickname}”</span>}</span>
                <span className="num block truncate text-xs text-muted-foreground">{fmtPhone(c.mobile)}</span>
              </span>
            </button>
          ))}
          {current && search.data!.more && <p className="px-2 pb-1 pt-1.5 text-[11px] text-muted-foreground">{t("More customers match — keep typing to narrow the list")}</p>}
          {nothing && (
            <div className="space-y-2 p-2">
              <p className="text-sm text-muted-foreground">{t("No matching customer found.")}</p>
              <Button size="sm" variant="outline" className="w-full" disabled={!canCreate && !(phoneLike && digits.length >= 9)} onPointerDown={(e) => e.preventDefault()} onClick={add}>
                <Plus /> {t("Add new customer")}
              </Button>
              {!canCreate && !(phoneLike && digits.length >= 9) && <p className="text-[11px] text-muted-foreground">{t("Type the full mobile number to add this customer with the sale")}</p>}
            </div>
          )}
        </div>
      )}
      <CustomerForm open={form} onOpenChange={setForm}
        initial={phoneLike ? { mobile: q.trim() } : { first_name: names[0] ?? "", other_names: names.slice(1).join(" ") }}
        onSaved={async (_id, mobile) => {
          try {
            const r = await api<{ customer: Customer | null }>("/customers/lookup", { query: { mobile: mobile ?? "" } });
            if (r.customer) pick(r.customer);
          } catch (e) {
            toast.error(e);
          }
        }} />
    </div>
  );
}
