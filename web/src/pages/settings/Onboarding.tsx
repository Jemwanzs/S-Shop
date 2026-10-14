/** Settings → Business → Onboarding (roadmap 99): Excel templates and uploads to bring a business's records into
 * S'Shop — upload → map columns → preview & validate → (approval) → post — with import history and totals.
 * Phase 1: products, customers, opening stock balances and expenses; the other datasets follow. */
import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Download, FileSpreadsheet, Info, RefreshCw, Scale, Trash2, Upload } from "lucide-react";
import { api, session } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { ago, count, money } from "@/lib/format";
import { download } from "@/lib/receipt";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";
import { Select } from "@/components/Select";
import { Pill, type Tone } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { ImportProgress } from "@/components/ImportProgress";
import { Card, SettingsPage } from "./shared";

interface Column { key: string; header: string; required: boolean; help?: string }
interface Dataset { key: string; label: string; columns: Column[]; may_skip_invalid: boolean }
interface BatchRow {
  id: string; dataset: string; mode: string; status: string; file_name: string; total_rows: number; valid_rows: number; warning_rows: number;
  invalid_rows: number; posted_rows: number; created_at: string; posted_at: string | null; uploaded_by: string | null; decided_by: string | null; error: string | null;
  reference?: string | null;
}
interface Overview {
  cutover: string | null; completed_at: string | null; datasets: Dataset[]; batches: BatchRow[]; branches: number;
  totals: {
    products: number; customers: number; opening_quantity: number; opening_value: string | number; expenses_count: number; expenses_amount: string | number;
    sales_count: number; sales_value: string | number; credit_outstanding: string | number; loyalty_points: number;
    orders_history: number; orders_outstanding: number; transfers: number; transfers_in_transit: number;
  };
}
interface RecLine {
  batch_id: string; dataset: string; label: string; file_name: string; posted_at: string; rows_posted: number; rows_left_out: number;
  measure: string; source: string | number; imported: string | number; difference: string | number;
}
interface Reconciliation {
  lines: RecLine[]; unfinished: { id: string; dataset: string; file_name: string; status: string; created_at: string }[];
  differences: number; rows_left_out: number; reconciled: boolean; completed_at: string | null;
}
interface Msg { level: "error" | "warning"; field: string; message: string }
interface Row { row_no: number; status: "valid" | "warning" | "invalid" | "posted" | "skipped"; data: Record<string, unknown>; source: Record<string, string>; messages: Msg[] }
interface BatchView { batch: BatchRow & { mapping: Record<string, string>; summary: Record<string, number> }; rows: Row[]; columns: Column[]; may_skip_invalid: boolean }

const STATUS: Record<string, [Tone, string]> = {
  validated: ["info", "Ready to post"], pending_approval: ["warning", "Awaiting approval"], posting: ["info", "Importing"], posted: ["success", "Imported"],
  rejected: ["danger", "Rejected"], cancelled: ["neutral", "Discarded"], failed: ["danger", "Failed"],
};
const ROW_TONE: Record<string, Tone> = { valid: "success", warning: "warning", invalid: "danger", posted: "success", skipped: "neutral" };
const ORDER = [
  "Confirm branches (codes) and settings", "Products", "Customers", "Sales history and expenses", "Credit sales & opening balances",
  "Loyalty opening points", "Opening stock at the cutover date", "Orders and stock transfers", "Reconcile, then trade in S'Shop",
];

async function fetchFile(path: string, name: string) {
  const res = await fetch(`/api${path}`, { headers: { Authorization: `Bearer ${session.token}` } });
  if (!res.ok) throw new Error(t("The file could not be downloaded"));
  download(await res.blob(), name);
}

export function OnboardingSettings() {
  const qc = useQueryClient();
  const { can } = useSession();
  const q = useQuery({ queryKey: ["onboarding"], queryFn: () => api<Overview>("/onboarding") });
  const [open, setOpen] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [cutover, setCutover] = useState<string | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["onboarding"] });
  // An import still running (e.g. after a refresh) opens its progress again — once.
  const running = q.data?.batches.find((b) => b.status === "posting")?.id;
  const reopened = useRef(false);
  useEffect(() => {
    if (running && !reopened.current) {
      reopened.current = true;
      setProgress(running);
    }
  }, [running]);
  const showBatch = (b: BatchRow) => (b.status === "posting" ? setProgress(b.id) : setOpen(b.id));
  if (!q.data) return <SettingsPage title="Onboarding" loading><span /></SettingsPage>;
  const o = q.data;
  const cut = cutover ?? o.cutover ?? "";

  return (
    <SettingsPage title="Onboarding" description="Bring your records into S'Shop from Excel — products, customers, opening stock, sales history, credit balances, loyalty points, expenses, orders and transfers. Download a template, fill it in, upload it, check the preview and import it.">
      {o.completed_at && (
        <p className="surface flex items-center gap-2 p-3 text-sm"><CheckCircle2 className="h-4 w-4 text-success" /> {t("Onboarding completed")} · {ago(o.completed_at)}</p>
      )}
      <Card title="Migration cutover">
        <div className="space-y-2 py-2 text-sm">
          <p className="text-xs text-muted-foreground">{t("The day S'Shop takes over: records before it are history, opening stock balances are as of it, and S'Shop records everything after it.")}</p>
          <div className="flex flex-wrap items-end gap-2">
            <Input type="date" className="w-44" value={cut} onChange={(e) => setCutover(e.target.value)} aria-label={t("Migration cutover")} />
            <ActionButton size="sm" online blockedBy={[cut === (o.cutover ?? "") && "Nothing to save"]}
              onAction={async () => { await api("/onboarding/cutover", { method: "PUT", body: { date: cut || null } }); toast.success("Cutover date saved"); setCutover(null); refresh(); }}>{t("Save")}</ActionButton>
          </div>
        </div>
      </Card>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Tile label="Products imported" value={count(o.totals.products)} />
        <Tile label="Customers imported" value={count(o.totals.customers)} />
        <Tile label="Opening stock" value={`${count(o.totals.opening_quantity)} · ${money(o.totals.opening_value)}`} />
        <Tile label="Historical expenses" value={money(o.totals.expenses_amount)} />
        <Tile label="Historical sales" value={`${count(o.totals.sales_count)} · ${money(o.totals.sales_value)}`} />
        <Tile label="Outstanding credit" value={money(o.totals.credit_outstanding)} />
        <Tile label="Loyalty points imported" value={count(o.totals.loyalty_points)} />
        <Tile label="Orders imported" value={`${count(o.totals.orders_history + o.totals.orders_outstanding)} · ${count(o.totals.orders_outstanding)} ${t("open")}`} />
        <Tile label="Transfers imported" value={`${count(o.totals.transfers)} · ${count(o.totals.transfers_in_transit)} ${t("in transit")}`} />
      </div>
      <Card title="Recommended order">
        <ol className="grid gap-1 py-2 text-sm sm:grid-cols-2">
          {ORDER.map((s, i) => <li key={s} className="flex gap-2"><span className="num font-semibold text-primary">{i + 1}.</span> {t(s)}</li>)}
        </ol>
        {o.branches === 0 && <p className="text-xs text-destructive">{t("Add your branches first (Settings → Branches): stock and expenses refer to their codes.")}</p>}
      </Card>
      <div className="grid gap-3 md:grid-cols-2">
        {o.datasets.map((d) => {
          const last = o.batches.find((b) => b.dataset === d.key);
          return (
            <section key={d.key} className="surface card-body space-y-3">
              <div className="flex items-center justify-between gap-2">
                <h3 className="flex items-center gap-2 font-semibold"><FileSpreadsheet className="h-4 w-4 text-primary" /> {t(d.label)}</h3>
                {last && <Pill tone={STATUS[last.status]?.[0] ?? "neutral"}>{STATUS[last.status]?.[1] ?? last.status}</Pill>}
              </div>
              <p className="text-xs text-muted-foreground">{t("Required")}: {d.columns.filter((c) => c.required).map((c) => c.header).join(", ")}</p>
              <div className="flex flex-wrap gap-2">
                <Button size="sm" variant="outline" onClick={() => fetchFile(`/onboarding/${d.key}/template`, `SShop_${d.label.replace(/ /g, "_")}_template.xlsx`).catch((e) => toast.error(e))}>
                  <Download /> {t("Template")}
                </Button>
                <UploadButton dataset={d.key} canUpdate={d.key === "products" && can("products.edit")} onDone={(id) => { refresh(); setOpen(id); }} />
              </div>
              {last && <button className="text-xs text-primary underline" onClick={() => showBatch(last)}>{t("Last import")}: {last.file_name} · {ago(last.created_at)}</button>}
            </section>
          );
        })}
      </div>
      <ReconciliationCard datasets={o.datasets} onChange={refresh} />
      <Card title="Import history">
        <ul className="divide-y text-sm">
          {o.batches.map((b) => (
            <li key={b.id}>
              <button className="flex w-full flex-wrap items-center gap-2 py-2.5 text-start hover:bg-muted/40" onClick={() => showBatch(b)}>
                <Pill tone={STATUS[b.status]?.[0] ?? "neutral"}>{STATUS[b.status]?.[1] ?? b.status}</Pill>
                <span className="font-medium">{t(o.datasets.find((d) => d.key === b.dataset)?.label ?? b.dataset)}</span>
                <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{b.file_name}</span>
                <span className="num text-xs text-muted-foreground">{count(b.total_rows)} {t("rows")} · {b.status === "posted" ? `${count(b.posted_rows)} ${t("imported")}` : `${count(b.invalid_rows)} ${t("invalid")}`} · {b.uploaded_by} · {ago(b.created_at)}</span>
              </button>
            </li>
          ))}
          {!o.batches.length && <li className="py-3 text-muted-foreground">{t("No imports yet.")}</li>}
        </ul>
      </Card>
      {open && <BatchDialog id={open} datasets={o.datasets} onClose={() => { setOpen(null); refresh(); }} onProgress={(id) => { setOpen(null); setProgress(id); refresh(); }} />}
      {progress && (() => {
        const b = o.batches.find((x) => x.id === progress);
        const label = o.datasets.find((d) => d.key === b?.dataset)?.label ?? "Data";
        return (
          <ImportProgress id={progress} label={label} onClose={() => { setProgress(null); refresh(); }}
            onCheckAgain={() => { const id = progress; setProgress(null); setOpen(id); }}
            errorReport={() => fetchFile(`/onboarding/batches/${progress}/errors`, `Fix_${b?.file_name ?? "import.xlsx"}`).catch((e) => toast.error(e))} />
        );
      })()}
    </SettingsPage>
  );
}

const MEASURE: Record<string, string> = { records: "Records", units: "Units", amount: "Amount", balance: "Balance", points: "Points" };

/** Migration reconciliation: what each imported workbook said against what S'Shop holds for it, and completion. */
function ReconciliationCard({ datasets, onChange }: { datasets: Dataset[]; onChange: () => void }) {
  const [f, setF] = useState({ dataset: "", branch_id: "", from: "", to: "" });
  const [confirm, setConfirm] = useState(false);
  const query = { dataset: f.dataset || undefined, branch_id: f.branch_id || undefined, from: f.from || undefined, to: f.to || undefined };
  const q = useQuery({ queryKey: ["onboarding-reconciliation", f], queryFn: () => api<Reconciliation>("/onboarding/reconciliation", { query }) });
  const branches = useQuery({ queryKey: ["branches"], queryFn: () => api<{ id: string; name: string; is_active: boolean }[]>("/branches") });
  const qs = new URLSearchParams(Object.entries(query).filter((e): e is [string, string] => !!e[1])).toString();
  const r = q.data;
  const filtered = !!(f.dataset || f.branch_id || f.from || f.to);
  const isMoney = (m: string) => m === "amount" || m === "balance";
  const show = (m: string, v: string | number) => (isMoney(m) ? money(v) : count(Number(v)));
  return (
    <Card title="Migration reconciliation">
      <div className="space-y-3 py-2 text-sm">
        <p className="text-xs text-muted-foreground">{t("Each imported workbook's totals against what S'Shop holds for it now. Every difference must be zero and every import finished before onboarding is complete.")}</p>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Select value={f.dataset} onChange={(v) => setF({ ...f, dataset: v })} label={t("Dataset")}>
            <option value="">{t("All datasets")}</option>
            {datasets.map((d) => <option key={d.key} value={d.key}>{t(d.label)}</option>)}
          </Select>
          <Select value={f.branch_id} onChange={(v) => setF({ ...f, branch_id: v })} label={t("Branch")}>
            <option value="">{t("All branches")}</option>
            {(branches.data ?? []).map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
          </Select>
          <Input type="date" value={f.from} onChange={(e) => setF({ ...f, from: e.target.value })} aria-label={t("From")} />
          <Input type="date" value={f.to} onChange={(e) => setF({ ...f, to: e.target.value })} aria-label={t("To")} />
        </div>
        {!r ? <Loading /> : (
          <>
            <div className="-mx-1 overflow-x-auto">
              <table className="w-full min-w-136 text-xs">
                <thead className="text-muted-foreground">
                  <tr className="text-start">
                    <th className="p-1 text-start">{t("Dataset")}</th><th className="p-1 text-start">{t("File")}</th><th className="p-1 text-end">{t("Rows")}</th>
                    <th className="p-1 text-end">{t("Workbook")}</th><th className="p-1 text-end">{t("In S'Shop")}</th><th className="p-1 text-end">{t("Difference")}</th>
                  </tr>
                </thead>
                <tbody className="divide-y">
                  {r.lines.map((l) => {
                    const off = Number(l.difference) !== 0;
                    return (
                      <tr key={l.batch_id}>
                        <td className="p-1">{t(l.label)}<span className="block text-muted-foreground">{t(MEASURE[l.measure] ?? l.measure)}</span></td>
                        <td className="max-w-40 truncate p-1">{l.file_name}</td>
                        <td className="num p-1 text-end">{count(l.rows_posted)}{l.rows_left_out > 0 && <span className="block text-warning">{count(l.rows_left_out)} {t("left out")}</span>}</td>
                        <td className="num p-1 text-end">{show(l.measure, l.source)}</td>
                        <td className="num p-1 text-end">{show(l.measure, l.imported)}</td>
                        <td className={cn("num p-1 text-end font-semibold", off ? "text-destructive" : "text-success")}>{show(l.measure, l.difference)}</td>
                      </tr>
                    );
                  })}
                  {!r.lines.length && <tr><td colSpan={6} className="p-2 text-muted-foreground">{t("No imports yet.")}</td></tr>}
                </tbody>
              </table>
            </div>
            {r.unfinished.length > 0 && (
              <div className="rounded-md border border-warning/40 bg-warning/10 p-2 text-xs">
                <p className="font-medium">{t("Imports not finished")} ({count(r.unfinished.length)})</p>
                <p className="text-muted-foreground">{r.unfinished.map((u) => `${t(datasets.find((d) => d.key === u.dataset)?.label ?? u.dataset)} — ${u.file_name} (${t(STATUS[u.status]?.[1] ?? u.status)})`).join(" · ")}</p>
                <p className="text-muted-foreground">{t("Import them, or discard them from Import history.")}</p>
              </div>
            )}
            <div className="flex flex-wrap items-center gap-2">
              <Pill tone={r.reconciled ? "success" : "warning"}>{r.reconciled ? t("Reconciled") : `${count(r.differences)} ${t("difference(s)")}`}</Pill>
              <Button size="sm" variant="outline" onClick={() => fetchFile(`/onboarding/reconciliation/export${qs ? `?${qs}` : ""}`, "SShop_Migration_Reconciliation.xlsx").catch((e) => toast.error(e))}>
                <Download /> {t("Export")}
              </Button>
              {!r.completed_at && (
                <Button size="sm" disabled={filtered || !r.reconciled} title={filtered ? t("Clear the filters to complete onboarding") : undefined} onClick={() => setConfirm(true)}>
                  <Scale /> {t("Complete onboarding")}
                </Button>
              )}
            </div>
          </>
        )}
      </div>
      {confirm && (
        <ResponsiveDialog open onOpenChange={(v) => !v && setConfirm(false)} title={t("Complete onboarding")}>
          <div className="space-y-3 text-sm">
            <p>{t("Every import reconciles. From now on S'Shop is the record: sell, order, transfer and collect credit here.")}</p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setConfirm(false)}>{t("Cancel")}</Button>
              <ActionButton online onAction={async () => { await api("/onboarding/complete", { method: "POST" }); toast.success("Onboarding completed"); setConfirm(false); q.refetch(); onChange(); }}>
                {t("Complete onboarding")}
              </ActionButton>
            </div>
          </div>
        </ResponsiveDialog>
      )}
    </Card>
  );
}

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="surface p-3">
      <p className="label-caps truncate">{t(label)}</p>
      <p className="num mt-0.5 truncate font-semibold">{value}</p>
    </div>
  );
}

function UploadButton({ dataset, canUpdate, onDone }: { dataset: string; canUpdate: boolean; onDone: (id: string) => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [mode, setMode] = useState("create");
  const send = async (file: File) => {
    setBusy(true);
    try {
      const fd = new FormData();
      fd.append("mode", mode);
      fd.append("file", file);
      const r = await api<BatchView>(`/onboarding/${dataset}/upload`, { method: "POST", body: fd });
      onDone(r.batch.id);
    } catch (e) {
      toast.error(e);
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };
  return (
    <>
      {canUpdate && (
        <Select value={mode} onChange={setMode} label={t("Import mode")} className="h-9 w-48">
          <option value="create">{t("Add new products")}</option>
          <option value="update">{t("Update existing products")}</option>
        </Select>
      )}
      <Button size="sm" disabled={busy} onClick={() => input.current?.click()}><Upload /> {busy ? t("Checking…") : t("Upload Excel")}</Button>
      <input ref={input} type="file" accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" className="hidden"
        onChange={(e) => { const f = e.target.files?.[0]; if (f) void send(f); }} />
    </>
  );
}

function BatchDialog({ id, datasets, onClose, onProgress }: { id: string; datasets: Dataset[]; onClose: () => void; onProgress: (id: string) => void }) {
  const qc = useQueryClient();
  const [filter, setFilter] = useState<string>("");
  const [skip, setSkip] = useState(false);
  const [mapping, setMapping] = useState<Record<string, string> | null>(null);
  const q = useQuery({ queryKey: ["onboarding-batch", id, filter], queryFn: () => api<BatchView>(`/onboarding/batches/${id}`, { query: { status: filter || undefined } }) });
  const reload = (v?: BatchView) => { if (v) qc.setQueryData(["onboarding-batch", id, filter], v); else void q.refetch(); qc.invalidateQueries({ queryKey: ["onboarding"] }); };
  const v = q.data;
  const b = v?.batch;
  const label = datasets.find((d) => d.key === b?.dataset)?.label ?? "";
  const editable = b?.status === "validated" || b?.status === "failed";
  const map = mapping ?? b?.mapping ?? {};
  const mapDirty = !!mapping && JSON.stringify(mapping) !== JSON.stringify(b?.mapping);
  const val = (r: Row, c: Column) => {
    const x = r.data[c.key];
    if (x === null || x === undefined) return "";
    if (typeof x === "object") return String((x as { invalid?: string }).invalid ?? "");
    if (typeof x === "boolean") return x ? "YES" : "NO";
    return String(x);
  };
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} wide title={`${t(label)} · ${b?.file_name ?? ""}`}
      description={b ? `${t(b.mode === "update" ? "Update existing products" : "Add new records")} · ${b.uploaded_by ?? ""} · ${ago(b.created_at)}` : undefined}
      footer={b && (
        <div className="flex w-full flex-wrap items-center justify-end gap-2">
          {(b.invalid_rows > 0 || b.warning_rows > 0) && (
            <Button variant="outline" size="sm" onClick={() => fetchFile(`/onboarding/batches/${id}/errors`, `Fix_${b.file_name}`).catch((e) => toast.error(e))}><Download /> {t("Error report")}</Button>
          )}
          {editable && <Button variant="ghost" size="sm" onClick={async () => { await api(`/onboarding/batches/${id}/cancel`, { method: "POST" }); toast.success("Import discarded"); onClose(); }}><Trash2 /> {t("Discard")}</Button>}
          {editable && <Button variant="outline" size="sm" onClick={async () => reload(await api<BatchView>(`/onboarding/batches/${id}/revalidate`, { method: "POST" }))}><RefreshCw /> {t("Check again")}</Button>}
          {editable && (
            <ActionButton size="sm" online blockedBy={[b.invalid_rows > 0 && !(skip && v?.may_skip_invalid) && "Fix the invalid rows first", b.invalid_rows === b.total_rows && "No valid rows", mapDirty && "Apply the column mapping first"]}
              onAction={async () => {
                await api<{ status: string }>(`/onboarding/batches/${id}/post`, { body: { skip_invalid: skip } });
                onProgress(id);
              }}><CheckCircle2 /> {t("Import")} {count(b.total_rows - (skip ? b.invalid_rows : 0))} {t("rows")}</ActionButton>
          )}
          {(b.status === "posted" || b.status === "posting") && <Button size="sm" onClick={() => onProgress(id)}><CheckCircle2 /> {t(b.status === "posted" ? "Import summary" : "View progress")}</Button>}
        </div>
      )}>
      {!v || !b ? <Loading /> : (
        <div className="space-y-4">
          <div className="flex flex-wrap gap-2 text-xs">
            {([["", `${t("All")} ${count(b.total_rows)}`, "neutral"], ["valid", `${t("Valid")} ${count(b.valid_rows)}`, "success"], ["warning", `${t("Warnings")} ${count(b.warning_rows)}`, "warning"], ["invalid", `${t("Invalid")} ${count(b.invalid_rows)}`, "danger"]] as const).map(([k, l, tone]) => (
              <button key={k} onClick={() => setFilter(k)} className={cn("rounded-full border px-3 py-1", filter === k ? "border-primary bg-primary/5 font-medium" : "bg-card")}>
                <Pill tone={tone as Tone} className="me-1 px-1.5">●</Pill>{l}
              </button>
            ))}
            <Pill tone={STATUS[b.status]?.[0] ?? "neutral"} className="ms-auto">{STATUS[b.status]?.[1] ?? b.status}</Pill>
          </div>
          {b.status === "posted" && (
            <p className="rounded-lg bg-success/10 p-2.5 text-sm text-success">
              {t("Imported")} {count(b.posted_rows)} {t("row(s)")}{b.summary?.created ? ` · ${count(b.summary.created)} ${t("created")}` : ""}{b.summary?.matched ? ` · ${count(b.summary.matched)} ${t("matched (not duplicated)")}` : ""}{b.decided_by ? ` · ${t("approved by")} ${b.decided_by}` : ""}
            </p>
          )}
          {b.status === "failed" && b.error && (
            <p className="rounded-lg bg-destructive/10 p-2.5 text-sm text-destructive">{b.error} · {t("Nothing was saved — check again, then import again.")}</p>
          )}
          {b.status === "pending_approval" && <p className="rounded-lg bg-warning/10 p-2.5 text-sm text-warning">{t("Waiting for approval in Approvals. Nothing is saved until it is approved.")}</p>}
          {editable && b.invalid_rows > 0 && v.may_skip_invalid && (
            <label className="flex items-center gap-2 text-sm"><Checkbox checked={skip} onCheckedChange={(c) => setSkip(!!c)} /> {t("Import only the valid rows (invalid rows stay out; fix them and upload them again)")}</label>
          )}
          {editable && b.invalid_rows > 0 && !v.may_skip_invalid && <p className="flex gap-2 text-xs text-muted-foreground"><Info className="h-4 w-4 shrink-0" /> {t("Stock and money import complete or not at all: fix the invalid rows (error report), then upload again.")}</p>}
          {editable && (
            <details className="rounded-xl border p-3 text-sm">
              <summary className="cursor-pointer font-medium">{t("Column mapping")}</summary>
              <div className="mt-3 grid gap-2 sm:grid-cols-2">
                {Object.keys(map).map((h) => (
                  <Field key={h} label={h}>
                    <Select value={map[h] ?? ""} onChange={(k) => setMapping({ ...map, [h]: k })}>
                      <option value="">{t("Ignore this column")}</option>
                      {v.columns.map((c) => <option key={c.key} value={c.key}>{c.header}{c.required ? " *" : ""}</option>)}
                    </Select>
                  </Field>
                ))}
              </div>
              {mapDirty && (
                <ActionButton size="sm" className="mt-3" online onAction={async () => { reload(await api<BatchView>(`/onboarding/batches/${id}/remap`, { body: { mapping: map } })); setMapping(null); }}>
                  {t("Apply mapping")}
                </ActionButton>
              )}
            </details>
          )}
          <div className="overflow-x-auto rounded-xl border">
            <table className="w-full text-xs">
              <thead className="bg-muted/60">
                <tr>
                  <th className="px-2 py-2 text-start">{t("Row")}</th>
                  {v.columns.map((c) => <th key={c.key} className="whitespace-nowrap px-2 py-2 text-start">{c.header}</th>)}
                  <th className="px-2 py-2 text-start">{t("Check")}</th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {v.rows.map((r) => (
                  <tr key={r.row_no} className={cn(r.status === "invalid" && "bg-destructive/5", r.status === "warning" && "bg-warning/5")}>
                    <td className="num px-2 py-1.5">{r.row_no}</td>
                    {v.columns.map((c) => {
                      const bad = r.messages.some((m) => m.field === c.key && m.level === "error");
                      return <td key={c.key} className={cn("max-w-48 truncate whitespace-nowrap px-2 py-1.5", bad && "font-semibold text-destructive")}>{val(r, c)}</td>;
                    })}
                    <td className="min-w-[16rem] px-2 py-1.5">
                      <Pill tone={ROW_TONE[r.status] ?? "neutral"}>{r.status}</Pill>
                      {r.messages.map((m, i) => <p key={i} className={m.level === "error" ? "text-destructive" : "text-warning"}>{m.message}</p>)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {v.rows.length >= 200 && <p className="text-xs text-muted-foreground">{t("Showing the first 200 rows of this view — use the filters or the error report for the rest.")}</p>}
        </div>
      )}
    </ResponsiveDialog>
  );
}
