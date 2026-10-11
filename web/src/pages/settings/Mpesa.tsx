/** Settings → M-Pesa & WhatsApp → M-Pesa (roadmap 91): the business's own Till or Paybill accounts. Credentials are
 * encrypted by the server and never shown again; STK Push stays off until Safaricom accepts a test prompt. Activity
 * shows every M-Pesa request with its outcome, and sales paid but not recorded (needs attention). */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Pencil, Plus, Send, ShieldCheck, Trash2, Wifi } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { ago, money } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { PasswordInput } from "@/components/PasswordInput";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ConfirmDialog, Field, ToggleRow } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { Card } from "./shared";

interface Account {
  id: string;
  name: string;
  kind: "till" | "paybill";
  shortcode: string;
  till_number: string;
  account_reference: string;
  environment: "sandbox" | "production";
  consumer_key_hint: string;
  has_secret: boolean;
  has_passkey: boolean;
  is_active: boolean;
  stk_enabled: boolean;
  verified_at: string | null;
  last_tested_at: string | null;
  last_error: string | null;
  last_error_at: string | null;
  all_branches: boolean;
  branch_ids: string[];
}
interface Accounts { items: Account[]; stk: { available: boolean; platform_config: boolean } }
type Draft = Omit<Account, "id" | "consumer_key_hint" | "has_secret" | "has_passkey" | "stk_enabled" | "verified_at" | "last_tested_at" | "last_error" | "last_error_at"> & {
  consumer_key: string; consumer_secret: string; passkey: string;
};
const blank: Draft = {
  name: "Main Till", kind: "till", shortcode: "", till_number: "", account_reference: "", environment: "sandbox", is_active: true,
  all_branches: true, branch_ids: [], consumer_key: "", consumer_secret: "", passkey: "",
};

export function MpesaAccounts() {
  const qc = useQueryClient();
  const { profile, can } = useSession();
  const q = useQuery({ queryKey: ["mpesa-accounts"], queryFn: () => api<Accounts>("/mpesa/accounts") });
  const [edit, setEdit] = useState<{ id: string | null; d: Draft } | null>(null);
  const [prompt, setPrompt] = useState<{ id: string; phone: string } | null>(null);
  const [remove, setRemove] = useState<Account | null>(null);
  const manage = can("settings.integrations") && !profile?.acting && !profile?.quick;
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["mpesa-accounts"] });
    qc.invalidateQueries({ queryKey: ["me"] });
    qc.invalidateQueries({ queryKey: ["mpesa-activity"] });
  };
  const save = useMutation({
    mutationFn: (e: { id: string | null; d: Draft }): Promise<{ needs_verification?: boolean; id?: string }> =>
      e.id ? api(`/mpesa/accounts/${e.id}`, { method: "PUT", body: e.d }) : api("/mpesa/accounts", { body: e.d }),
    onSuccess: (r) => {
      toast.success(r.needs_verification ? "Saved — send a test prompt to verify the new details" : "M-Pesa account saved");
      setEdit(null);
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const stk = useMutation({
    mutationFn: (v: { id: string; enabled: boolean }) => api(`/mpesa/accounts/${v.id}/stk`, { method: "PUT", body: { enabled: v.enabled } }),
    onSuccess: (_, v) => { toast.success(v.enabled ? "STK Push switched on" : "STK Push switched off"); refresh(); },
    onError: (e) => toast.error(e),
  });
  const result = (r: { ok: boolean; message: string }) => {
    if (r.ok) toast.success(r.message);
    else toast.error(r.message);
    refresh();
  };
  if (!q.data) return <Card title="M-Pesa"><Loading /></Card>;
  const branches = profile?.branches ?? [];

  return (
    <Card title="M-Pesa" action={manage && <Button size="sm" variant="outline" onClick={() => setEdit({ id: null, d: { ...blank } })}><Plus /> {t("Add account")}</Button>}>
      <div className="space-y-3 py-2 text-sm">
        <p className="text-xs text-muted-foreground">{t("Your own Buy Goods Till or Paybill, connected with your Daraja app. Credentials are encrypted and never shown again. Manual M-Pesa codes work without it.")}</p>
        {q.data.stk.platform_config && <p className="rounded-lg bg-muted/60 p-2.5 text-xs">{t("STK Push uses S'Shop's own configuration for this business until you add an account.")}</p>}
        {!q.data.items.length && !q.data.stk.platform_config && <p className="text-muted-foreground">{t("No M-Pesa account yet — STK Push is off.")}</p>}
        {q.data.items.map((a) => {
          const status: [Tone, string] = !a.is_active ? ["neutral", "Off"] : a.verified_at ? ["success", "Verified"] : a.last_error ? ["danger", "Error"] : ["warning", "Not verified"];
          return (
            <div key={a.id} className={cn("space-y-2 rounded-xl border p-3", !a.is_active && "opacity-70")}>
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-semibold">{a.name}</span>
                <Pill tone={status[0]}>{status[1]}</Pill>
                <Pill tone={a.environment === "production" ? "primary" : "info"}>{a.environment === "production" ? "Production" : "Sandbox"}</Pill>
                <span className="ms-auto flex gap-1">
                  {manage && <Button size="icon" variant="ghost" aria-label={t("Edit")} onClick={() => setEdit({ id: a.id, d: { ...a, consumer_key: "", consumer_secret: "", passkey: "" } })}><Pencil /></Button>}
                  {manage && <Button size="icon" variant="ghost" aria-label={t("Remove")} onClick={() => setRemove(a)}><Trash2 /></Button>}
                </span>
              </div>
              <p className="num text-xs text-muted-foreground">
                {a.kind === "till" ? <>{t("Till")} {a.till_number} · {t("Store")} {a.shortcode}</> : <>{t("Paybill")} {a.shortcode}{a.account_reference && <> · {t("Account")} {a.account_reference}</>}</>}
                {" · "}{a.all_branches ? t("All branches") : branches.filter((b) => a.branch_ids.includes(b.id)).map((b) => b.name).join(", ")}
                {" · "}{t("Key")} {a.consumer_key_hint || "—"}
              </p>
              <div className="flex items-center justify-between gap-3 rounded-lg bg-muted/50 p-2.5">
                <span>
                  <span className="font-medium">{t("STK Push")}</span>
                  <span className="block text-xs text-muted-foreground">
                    {a.stk_enabled ? t("Cashiers can send payment prompts to customers' phones.") : a.verified_at ? t("Verified — switch on to use it at the till.") : t("Off until a test prompt is accepted by Safaricom.")}
                  </span>
                </span>
                <Switch checked={a.stk_enabled} disabled={!manage || !a.verified_at || !a.is_active || stk.isPending} aria-label={t("STK Push")}
                  onCheckedChange={(v) => stk.mutate({ id: a.id, enabled: v })} />
              </div>
              {a.last_error && <p className="rounded-lg bg-destructive/10 p-2 text-xs text-destructive">{a.last_error} {a.last_error_at && <span className="opacity-70">· {ago(a.last_error_at)}</span>}</p>}
              {manage && a.is_active && (
                <div className="flex flex-wrap gap-2">
                  <ActionButton size="sm" variant="outline" online onAction={async () => result(await api(`/mpesa/accounts/${a.id}/test`, { method: "POST" }))}><Wifi /> {t("Test connection")}</ActionButton>
                  <Button size="sm" variant="outline" onClick={() => setPrompt({ id: a.id, phone: "" })}><Send /> {t("Send test prompt")}</Button>
                </div>
              )}
            </div>
          );
        })}
        {!manage && <p className="text-xs text-muted-foreground">{t("Only people allowed to manage M-Pesa, fully signed in, change these accounts.")}</p>}
      </div>

      {edit && <AccountEditor value={edit} onChange={setEdit} branches={branches} busy={save.isPending} onSave={() => save.mutate(edit)} />}
      <ResponsiveDialog open={!!prompt} onOpenChange={(o) => !o && setPrompt(null)} title="Send a test prompt"
        description="Safaricom sends a KSh 1 prompt to this phone. Accepting it verifies the account; nobody has to pay — cancel it on the phone."
        footer={<ActionButton className="w-full sm:w-auto" online blockedBy={[(prompt?.phone.replace(/\D/g, "").length ?? 0) < 9 && "Enter a phone number"]}
          onAction={async () => { result(await api(`/mpesa/accounts/${prompt!.id}/test-prompt`, { body: { phone: prompt!.phone } })); setPrompt(null); }}><Send /> {t("Send")}</ActionButton>}>
        <Field label="Phone number"><Input inputMode="tel" className="num" placeholder="07XXXXXXXX" value={prompt?.phone ?? ""} onChange={(e) => prompt && setPrompt({ ...prompt, phone: e.target.value })} /></Field>
      </ResponsiveDialog>
      <ConfirmDialog open={!!remove} onOpenChange={(o) => !o && setRemove(null)} title="Remove this M-Pesa account?" destructive confirmLabel="Remove"
        description="An account that has taken payments is switched off and kept for the records; STK Push stops at once."
        onConfirm={async () => { const r = await api<{ deactivated: boolean }>(`/mpesa/accounts/${remove!.id}`, { method: "DELETE" }); toast.success(r.deactivated ? "Account switched off (kept for the records)" : "Account removed"); setRemove(null); refresh(); }} />
    </Card>
  );
}

function AccountEditor({ value, onChange, branches, busy, onSave }: {
  value: { id: string | null; d: Draft }; onChange: (v: { id: string | null; d: Draft } | null) => void;
  branches: { id: string; name: string }[]; busy: boolean; onSave: () => void;
}) {
  const { id, d } = value;
  const set = (p: Partial<Draft>) => onChange({ id, d: { ...d, ...p } });
  const isNew = !id;
  const missing = (!d.name.trim() && "Name") || (!/^\d{5,10}$/.test(d.shortcode) && (d.kind === "till" ? "Store number" : "Paybill number"))
    || (d.kind === "till" && !/^\d{5,10}$/.test(d.till_number) && "Till number")
    || (isNew && (!d.consumer_key.trim() || !d.consumer_secret.trim() || !d.passkey.trim()) && "Credentials")
    || (!d.all_branches && !d.branch_ids.length && "Branches");
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onChange(null)} title={isNew ? "Add M-Pesa account" : "Edit M-Pesa account"} wide
      description="From the Safaricom Daraja portal (Lipa na M-Pesa Online). New or changed details must be verified again before STK Push is used."
      footer={<Button className="w-full sm:w-auto" disabled={!!missing || busy} onClick={onSave}>{busy ? t("Saving…") : missing ? `${t("Complete")}: ${t(missing)}` : t("Save")}</Button>}>
      <div className="space-y-3">
        <Field label="Name"><Input value={d.name} maxLength={60} onChange={(e) => set({ name: e.target.value })} placeholder="Main Till" /></Field>
        <Field group label="Type">
          <div className="grid grid-cols-2 gap-1 rounded-xl bg-muted p-1">
            {([["till", "Buy Goods Till"], ["paybill", "Paybill"]] as const).map(([k, l]) => (
              <button key={k} type="button" onClick={() => set({ kind: k })} className={cn("h-9 rounded-lg text-sm font-medium", d.kind === k ? "bg-card shadow-sm" : "text-muted-foreground")}>{t(l)}</button>
            ))}
          </div>
        </Field>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={d.kind === "till" ? "Store / Head Office number" : "Paybill number"} hint="The Lipa na M-Pesa Online shortcode">
            <Input inputMode="numeric" className="num" value={d.shortcode} onChange={(e) => set({ shortcode: e.target.value.replace(/\D/g, "") })} />
          </Field>
          {d.kind === "till"
            ? <Field label="Till number" hint="The number customers pay"><Input inputMode="numeric" className="num" value={d.till_number} onChange={(e) => set({ till_number: e.target.value.replace(/\D/g, "") })} /></Field>
            : <Field label="Account reference" optional hint="Shown to customers (up to 12 characters)"><Input value={d.account_reference} maxLength={12} onChange={(e) => set({ account_reference: e.target.value })} /></Field>}
        </div>
        <Field group label="Environment">
          <div className="grid grid-cols-2 gap-1 rounded-xl bg-muted p-1">
            {([["sandbox", "Sandbox (testing)"], ["production", "Production"]] as const).map(([k, l]) => (
              <button key={k} type="button" onClick={() => set({ environment: k })} className={cn("h-9 rounded-lg text-sm font-medium", d.environment === k ? "bg-card shadow-sm" : "text-muted-foreground")}>{t(l)}</button>
            ))}
          </div>
        </Field>
        <div className="space-y-3 rounded-xl border p-3">
          <p className="flex items-center gap-2 text-xs text-muted-foreground"><ShieldCheck className="h-4 w-4 text-success" /> {isNew ? t("Encrypted when saved and never shown again.") : t("Saved encrypted. Leave empty to keep what is saved.")}</p>
          <Field label="Consumer key"><Input autoComplete="off" value={d.consumer_key} onChange={(e) => set({ consumer_key: e.target.value })} placeholder={isNew ? "" : t("Saved")} /></Field>
          <Field label="Consumer secret"><PasswordInput autoComplete="new-password" value={d.consumer_secret} onChange={(e) => set({ consumer_secret: e.target.value })} placeholder={isNew ? "" : t("Saved")} /></Field>
          <Field label="Passkey"><PasswordInput autoComplete="new-password" value={d.passkey} onChange={(e) => set({ passkey: e.target.value })} placeholder={isNew ? "" : t("Saved")} /></Field>
        </div>
        <ToggleRow label="All branches" hint="Off: choose the branches this account takes payments for" checked={d.all_branches} onChange={(v) => set({ all_branches: v })} />
        {!d.all_branches && (
          <div className="grid gap-2 rounded-xl bg-muted/50 p-3 sm:grid-cols-2">
            {branches.map((b) => (
              <label key={b.id} className="flex items-center gap-2 text-sm">
                <Checkbox checked={d.branch_ids.includes(b.id)} onCheckedChange={(c) => set({ branch_ids: c ? [...d.branch_ids, b.id] : d.branch_ids.filter((x) => x !== b.id) })} /> {b.name}
              </label>
            ))}
          </div>
        )}
        {!isNew && <ToggleRow label="Active" hint="Off stops payments through this account" checked={d.is_active} onChange={(v) => set({ is_active: v })} />}
      </div>
    </ResponsiveDialog>
  );
}

interface Activity {
  id: string; created_at: string; status: string; purpose: "payment" | "sale" | "test"; amount: string; paid_amount: string | null;
  receipt: string | null; result: string | null; phone: string; account: string | null; branch: string | null; cashier: string | null;
  sale_id: string | null; receipt_no: string | null; pending_id: string | null; pending_status: string | null; pending_error: string | null;
}
const STATUS_TONE: Record<string, Tone> = { success: "success", pending: "warning", failed: "danger", cancelled: "neutral", timeout: "neutral" };
const STATUS_LABEL: Record<string, string> = { success: "Successful", pending: "Pending", failed: "Failed", cancelled: "Cancelled", timeout: "Timed out" };

/** Every M-Pesa request with its outcome; paid sales that could not be recorded are completed from here. */
export function MpesaActivity() {
  const qc = useQueryClient();
  const { can } = useSession();
  const [filter, setFilter] = useState<"" | "attention">("");
  const q = useQuery({ queryKey: ["mpesa-activity", filter], queryFn: () => api<{ items: Activity[]; attention: number }>("/mpesa/activity", { query: { status: filter || undefined } }) });
  if (!q.data) return <Card title="M-Pesa activity"><Loading /></Card>;
  return (
    <Card title="M-Pesa activity" action={
      <div className="flex gap-1">
        {([["", "All"], ["attention", `Needs attention (${q.data.attention})`]] as const).map(([k, l]) => (
          <Button key={k} size="sm" variant={filter === k ? "secondary" : "ghost"} onClick={() => setFilter(k)}>{t(l.replace(/ \(\d+\)$/, ""))}{k === "attention" && ` (${q.data.attention})`}</Button>
        ))}
      </div>
    }>
      <ul className="divide-y text-sm">
        {q.data.items.map((a) => (
          <li key={a.id} className="space-y-1 py-2.5">
            <div className="flex flex-wrap items-center gap-2">
              <Pill tone={STATUS_TONE[a.status] ?? "neutral"}>{STATUS_LABEL[a.status] ?? a.status}</Pill>
              {a.pending_status === "attention" && <Pill tone="warning">Needs attention</Pill>}
              <span className="num font-semibold">{money(a.paid_amount ?? a.amount)}</span>
              <span className="text-xs text-muted-foreground">{a.purpose === "sale" ? t("STK sale") : a.purpose === "test" ? t("Test prompt") : t("Payment")}</span>
              <span className="ms-auto text-xs text-muted-foreground">{ago(a.created_at)}</span>
            </div>
            <p className="num text-xs text-muted-foreground">
              {a.phone}{a.receipt && <> · {a.receipt}</>}{a.receipt_no && <> · {a.receipt_no}</>}{a.cashier && <> · {a.cashier}</>}{a.branch && <> · {a.branch}</>}{a.account && <> · {a.account}</>}
            </p>
            {(a.pending_error || (a.status !== "success" && a.result)) && <p className={cn("text-xs", a.pending_status === "attention" ? "text-warning" : "text-muted-foreground")}>{a.pending_error || a.result}</p>}
            {a.pending_status === "attention" && a.status === "success" && can("sales.create") && (
              <ActionButton size="sm" variant="outline" online onAction={async () => {
                await api(`/sales/stk/${a.pending_id}/complete`, { method: "POST" });
                toast.success("Sale completed");
                qc.invalidateQueries({ queryKey: ["mpesa-activity"] });
              }}><CheckCircle2 /> {t("Complete the sale")}</ActionButton>
            )}
          </li>
        ))}
        {!q.data.items.length && <li className="py-3 text-muted-foreground">{t("No M-Pesa activity yet.")}</li>}
      </ul>
      {q.data.attention > 0 && <p className="pt-2 text-xs text-muted-foreground">{t("A payment that cannot be completed as a sale is refunded from your M-Pesa portal; it stays here for the record.")}</p>}
    </Card>
  );
}

interface PlatformMpesa {
  configured: boolean; environment?: string; shortcode?: string; transaction_type?: string; party_b?: string; callback_url?: string;
  callback_matches_server?: boolean; public_url: string; used_by_this_business?: boolean; problem: string | null;
}

/** Platform owner only (roadmap 92): the platform's Daraja app from the Railway variables — status and Sandbox tests. */
export function PlatformMpesaCard() {
  const qc = useQueryClient();
  const { profile } = useSession();
  const owner = !!profile?.user.platform_admin && !profile?.acting;
  const q = useQuery({ queryKey: ["mpesa-platform"], queryFn: () => api<PlatformMpesa>("/mpesa/platform"), enabled: owner });
  const [phone, setPhone] = useState("");
  if (!owner) return null;
  if (!q.data) return <Card title="Platform M-Pesa (Railway)"><Loading /></Card>;
  const p = q.data;
  const show = (r: { ok: boolean; message: string }) => {
    if (r.ok) toast.success(r.message);
    else toast.error(r.message);
    qc.invalidateQueries({ queryKey: ["mpesa-activity"] });
  };
  return (
    <Card title="Platform M-Pesa (Railway)" action={<Pill tone={p.configured ? (p.environment === "production" ? "primary" : "info") : p.problem ? "danger" : "neutral"}>
      {p.configured ? (p.environment === "production" ? "Production" : "Sandbox") : p.problem ? "Error" : "Not set"}</Pill>}>
      <div className="space-y-2 py-2 text-sm">
        <p className="text-xs text-muted-foreground">{t("S'Shop's own Daraja app from the MPESA_* variables. It serves only the platform owner's business until that business adds its own account; other businesses always use their own.")}</p>
        {p.problem && <p className="rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">{p.problem}</p>}
        {p.configured && (
          <>
            <p className="num text-xs text-muted-foreground">{t("Shortcode")} {p.shortcode} · {p.transaction_type === "CustomerBuyGoodsOnline" ? t("Buy Goods") : t("Paybill")}{p.party_b !== p.shortcode && <> · {t("Till")} {p.party_b}</>}</p>
            <div className="rounded-lg bg-muted/50 p-2.5">
              <p className="label-caps">{t("Callback URL")}</p>
              <p className="num break-all text-xs">{p.callback_url}</p>
              {!p.callback_matches_server && <p className="mt-1 text-xs text-warning">{t("This is not this server's address")} ({p.public_url}) — {t("Safaricom's confirmations would not reach S'Shop.")}</p>}
            </div>
            {!p.used_by_this_business && <p className="text-xs text-muted-foreground">{t("This business has its own M-Pesa account, which is used instead.")}</p>}
            {p.used_by_this_business && (
              <div className="flex flex-wrap items-end gap-2">
                <ActionButton size="sm" variant="outline" online onAction={async () => show(await api("/mpesa/platform/test", { method: "POST" }))}><Wifi /> {t("Test OAuth")}</ActionButton>
                <Input inputMode="tel" className="num h-9 w-40" placeholder="07XXXXXXXX" value={phone} onChange={(e) => setPhone(e.target.value)} aria-label={t("Phone number")} />
                <ActionButton size="sm" variant="outline" online blockedBy={[phone.replace(/\D/g, "").length < 9 && "Enter a phone number"]}
                  onAction={async () => show(await api("/mpesa/platform/test-prompt", { body: { phone } }))}><Send /> {t("Test STK prompt")}</ActionButton>
              </div>
            )}
          </>
        )}
        {!p.configured && !p.problem && <p className="text-xs text-muted-foreground">{t("Set MPESA_ENVIRONMENT, MPESA_CONSUMER_KEY, MPESA_CONSUMER_SECRET, MPESA_STK_PASSKEY, MPESA_SHORTCODE and MPESA_CALLBACK_URL on Railway.")}</p>}
      </div>
    </Card>
  );
}
