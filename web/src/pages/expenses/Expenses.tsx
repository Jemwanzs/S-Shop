import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, Paperclip, Plus, Wallet } from "lucide-react";
import { toast } from "sonner";
import { api, errorMessage, saveBlob, session } from "@/lib/api";
import { useSession } from "@/lib/session";
import { date, methodLabel, money, todayIso, toNum } from "@/lib/format";
import { fileToDataUrl, optimizeImage } from "@/lib/image";
import type { Money, Outcome } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { PageHeader, EmptyState, Section } from "@/components/Page";
import { DataList, Pager, CardRow } from "@/components/DataList";
import { PeriodFilter, Chip, type PeriodValue } from "@/components/Filters";
import { ConfirmDialog, Field, NativeSelect } from "@/components/Form";
import { StatusBadge } from "@/components/Badges";
import { StatCard } from "@/components/Stat";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

interface Expense {
  id: string; expense_date: string; branch_name: string; category_id: string; category_name: string; amount: Money; description: string;
  payee: string; payment_method: string; has_attachment: boolean; status: string; user_name: string | null; created_at: string;
}
interface Category { id: string; name: string; is_active: boolean }
const LIMIT = 50;

export default function Expenses() {
  const { currency, can, profile } = useSession();
  const qc = useQueryClient();
  const [period, setPeriod] = useState<PeriodValue>({ period: "month" });
  const [category, setCategory] = useState("");
  const [offset, setOffset] = useState(0);
  const [adding, setAdding] = useState(false);
  const [voiding, setVoiding] = useState<Expense | null>(null);
  const query = { ...period, category_id: category, limit: LIMIT, offset };
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["expenses", query],
    queryFn: () => api<{ items: Expense[]; total: number; summary: { approved_total: Money; by_category: { category: string; amount: Money }[] } }>("/expenses", { query }),
    placeholderData: (p) => p,
  });
  const categories = useQuery({ queryKey: ["expense-categories"], queryFn: () => api<Category[]>("/expense-categories") });
  const voidIt = useMutation({
    mutationFn: ({ id, reason }: { id: string; reason: string }) => api(`/expenses/${id}/void`, { body: { reason } }),
    onSuccess: () => { toast.success("Expense voided"); setVoiding(null); qc.invalidateQueries({ queryKey: ["expenses"] }); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const openAttachment = async (id: string) => {
    const res = await fetch(`/api/expenses/${id}/attachment`, { headers: { Authorization: `Bearer ${session.token}`, "X-Branch-Id": session.branchId ?? "" } });
    if (!res.ok) return toast.error("Attachment unavailable");
    const blob = await res.blob();
    if (blob.type.startsWith("image/") || blob.type === "application/pdf") window.open(URL.createObjectURL(blob), "_blank");
    else saveBlob(blob, "receipt");
  };
  const total = toNum(data?.summary.approved_total);

  return (
    <>
      <PageHeader eyebrow="Finance" title="Expenses" actions={can("expenses.create") && <Button onClick={() => setAdding(true)}><Plus /> Record expense</Button>} />
      <div className="mb-4 space-y-3">
        <PeriodFilter value={period} onChange={(v) => { setPeriod(v); setOffset(0); }} />
        <NativeSelect value={category} onChange={(v) => { setCategory(v); setOffset(0); }} className="sm:w-56">
          <option value="">All categories</option>
          {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
        </NativeSelect>
      </div>
      <div className="mb-5 grid gap-3 md:grid-cols-[260px_minmax(0,1fr)]">
        <StatCard label="Approved spend" value={money(total, currency)} icon={Wallet} tone="warning" />
        <Section title="By category">
          <div className="space-y-2">
            {data?.summary.by_category.map((c) => (
              <div key={c.category} className="space-y-1">
                <div className="flex justify-between text-sm"><span>{c.category}</span><span className="num">{money(c.amount, currency)}</span></div>
                <div className="h-1.5 overflow-hidden rounded-full bg-muted"><div className="h-full rounded-full bg-warning" style={{ width: `${total ? (toNum(c.amount) / total) * 100 : 0}%` }} /></div>
              </div>
            ))}
            {!data?.summary.by_category.length && <p className="text-sm text-muted-foreground">Nothing yet</p>}
          </div>
        </Section>
      </div>
      <DataList
        rows={data?.items}
        loading={isLoading}
        error={error}
        retry={refetch}
        rowKey={(r) => r.id}
        empty={<EmptyState icon={Wallet} title="No expenses in this period" />}
        columns={[
          { key: "date", header: "Date", cell: (r) => <span className="whitespace-nowrap">{date(r.expense_date)}</span> },
          { key: "category", header: "Category", cell: (r) => r.category_name },
          { key: "description", header: "Description", cell: (r) => <div><div>{r.description || "—"}</div>{r.payee && <div className="text-xs text-muted-foreground">{r.payee}</div>}</div> },
          { key: "branch", header: "Branch", cell: (r) => r.branch_name, hideBelow: "xl" },
          { key: "method", header: "Paid by", cell: (r) => methodLabel(r.payment_method), hideBelow: "lg" },
          { key: "user", header: "Recorded by", cell: (r) => r.user_name ?? "—", hideBelow: "xl" },
          { key: "status", header: "Status", cell: (r) => <StatusBadge status={r.status} /> },
          { key: "amount", header: "Amount", align: "right", cell: (r) => <span className="num font-semibold">{money(r.amount, currency)}</span> },
          {
            key: "actions",
            header: "",
            align: "right",
            cell: (r) => (
              <div className="flex justify-end gap-1">
                {r.has_attachment && <Button variant="ghost" size="icon-sm" onClick={() => openAttachment(r.id)} aria-label="Attachment"><Paperclip /></Button>}
                {r.status !== "void" && can("expenses.create") && <Button variant="ghost" size="icon-sm" onClick={() => setVoiding(r)} aria-label="Void"><Ban /></Button>}
              </div>
            ),
          },
        ]}
        mobile={(r) => (
          <div className="flex items-center gap-2">
            <div className="min-w-0 flex-1"><CardRow title={r.description || r.category_name} subtitle={`${r.category_name} · ${date(r.expense_date)}`} value={money(r.amount, currency)} meta={r.status !== "approved" ? <StatusBadge status={r.status} /> : undefined} /></div>
            {r.has_attachment && <Button variant="ghost" size="icon-sm" onClick={() => openAttachment(r.id)} aria-label="Attachment"><Paperclip /></Button>}
          </div>
        )}
        footer={data && <Pager total={data.total} limit={LIMIT} offset={offset} onChange={setOffset} />}
      />
      <NewExpense open={adding} onOpenChange={setAdding} categories={categories.data ?? []} methods={profile?.settings.sales.payment_methods.filter((m) => m.enabled && m.key !== "credit") ?? []} />
      <ConfirmDialog open={!!voiding} onOpenChange={(o) => !o && setVoiding(null)} title="Void this expense?" description="It stays on record but no longer counts." destructive requireReason confirmLabel="Void" busy={voidIt.isPending} onConfirm={(reason) => voiding && voidIt.mutate({ id: voiding.id, reason })} />
    </>
  );
}

function NewExpense({ open, onOpenChange, categories, methods }: { open: boolean; onOpenChange: (o: boolean) => void; categories: Category[]; methods: { key: string; label: string }[] }) {
  const { profile } = useSession();
  const qc = useQueryClient();
  const [categoryId, setCategoryId] = useState("");
  const [amount, setAmount] = useState("");
  const [day, setDay] = useState(todayIso());
  const [description, setDescription] = useState("");
  const [payee, setPayee] = useState("");
  const [method, setMethod] = useState("cash");
  const [file, setFile] = useState<File | null>(null);
  const s = profile!.settings.expenses;
  const save = useMutation({
    mutationFn: async () => {
      let attachment: string | undefined;
      if (file) attachment = await fileToDataUrl(file.type.startsWith("image/") ? await optimizeImage(file, 1800) : file);
      return api<Outcome<{ id: string }>>("/expenses", { body: { category_id: categoryId, amount: Number(amount), expense_date: day, description, payee, payment_method: method, attachment } });
    },
    onSuccess: (r) => {
      toast.success(r.pending_approval ? "Expense sent for approval" : "Expense recorded");
      qc.invalidateQueries({ queryKey: ["expenses"] });
      onOpenChange(false);
      setAmount("");
      setDescription("");
      setPayee("");
      setFile(null);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const valid = categoryId && toNum(amount) > 0 && (!s.require_description || description.trim()) && (!s.require_attachment || file);
  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} title="Record expense" footer={<Button className="w-full md:w-auto" disabled={!valid || save.isPending} onClick={() => save.mutate()}>Save expense</Button>}>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Category">
          <NativeSelect value={categoryId} onChange={setCategoryId}>
            <option value="">Choose…</option>
            {categories.filter((c) => c.is_active).map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </NativeSelect>
        </Field>
        <Field label="Amount"><Input inputMode="decimal" className="num" value={amount} onChange={(e) => setAmount(e.target.value.replace(/[^\d.]/g, ""))} /></Field>
        <Field label="Date"><Input type="date" value={day} max={todayIso()} onChange={(e) => setDay(e.target.value)} /></Field>
        <Field label="Supplier / payee" optional><Input value={payee} onChange={(e) => setPayee(e.target.value)} /></Field>
        <Field label="Description" optional={!s.require_description} className="sm:col-span-2"><Textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} /></Field>
        <div className="space-y-2 sm:col-span-2">
          <span className="text-sm font-medium">Paid by</span>
          <div className="flex flex-wrap gap-2">{methods.map((m) => <Chip key={m.key} active={method === m.key} onClick={() => setMethod(m.key)}>{m.label}</Chip>)}</div>
        </div>
        <Field label="Receipt / attachment" optional={!s.require_attachment} className="sm:col-span-2">
          <Input type="file" accept="image/*,application/pdf" capture="environment" onChange={(e) => setFile(e.target.files?.[0] ?? null)} />
        </Field>
      </div>
    </ResponsiveDialog>
  );
}
