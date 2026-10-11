/** Settings → Business → Onboarding (roadmap 99): Excel templates and uploads to bring a business's records into
 * S'Shop — upload → map columns → preview & validate → (approval) → post — with import history and totals.
 * Phase 1: products, customers, opening stock balances and expenses; the other datasets follow. */
import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Download, FileSpreadsheet, Info, RefreshCw, Trash2, Upload } from "lucide-react";
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
import { Card, SettingsPage } from "./shared";

interface Column { key: string; header: string; required: boolean; help?: string }
interface Dataset { key: string; label: string; columns: Column[]; may_skip_invalid: boolean }
interface BatchRow {
  id: string; dataset: string; mode: string; status: string; file_name: string; total_rows: number; valid_rows: number; warning_rows: number;
  invalid_rows: number; posted_rows: number; created_at: string; posted_at: string | null; uploaded_by: string | null; decided_by: string | null; error: string | null;
}
interface Overview {
  cutover: string | null; datasets: Dataset[]; batches: BatchRow[]; branches: number;
  totals: { products: number; customers: number; opening_quantity: number; opening_value: string | number; expenses_count: number; expenses_amount: string | number };
}
interface Msg { level: "error" | "warning"; field: string; message: string }
interface Row { row_no: number; status: "valid" | "warning" | "invalid" | "posted" | "skipped"; data: Record<string, unknown>; source: Record<string, string>; messages: Msg[] }
interface BatchView { batch: BatchRow & { mapping: Record<string, string>; summary: Record<string, number> }; rows: Row[]; columns: Column[]; may_skip_invalid: boolean }

const STATUS: Record<string, [Tone, string]> = {
  validated: ["info", "Ready to post"], pending_approval: ["warning", "Awaiting approval"], posted: ["success", "Imported"],
  rejected: ["danger", "Rejected"], cancelled: ["neutral", "Discarded"], failed: ["danger", "Failed"],
};
const ROW_TONE: Record<string, Tone> = { valid: "success", warning: "warning", invalid: "danger", posted: "success", skipped: "neutral" };
const LATER = ["Sales history", "Credit sales & balances", "Orders", "Stock transfers", "Loyalty & rewards"];
const ORDER = [
  "Confirm branches (codes) and settings", "Products", "Customers", "Expenses (history)", "Opening stock at the cutover date", "Reconcile, then trade in S'Shop",
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
  const [cutover, setCutover] = useState<string | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["onboarding"] });
  if (!q.data) return <SettingsPage title="Onboarding" loading><span /></SettingsPage>;
  const o = q.data;
  const cut = cutover ?? o.cutover ?? "";

  return (
    <SettingsPage title="Onboarding" description="Bring your products, customers, opening stock and expenses into S'Shop from Excel. Download a template, fill it in, upload it, check the preview and post it.">
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
              {last && <button className="text-xs text-primary underline" onClick={() => setOpen(last.id)}>{t("Last import")}: {last.file_name} · {ago(last.created_at)}</button>}
            </section>
          );
        })}
      </div>
      <Card title="Coming next">
        <p className="py-2 text-xs text-muted-foreground">{t("Next onboarding phases (historical records that never deduct stock again or duplicate revenue):")} {LATER.map((l) => t(l)).join(" · ")}</p>
      </Card>
      <Card title="Import history">
        <ul className="divide-y text-sm">
          {o.batches.map((b) => (
            <li key={b.id}>
              <button className="flex w-full flex-wrap items-center gap-2 py-2.5 text-start hover:bg-muted/40" onClick={() => setOpen(b.id)}>
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
      {open && <BatchDialog id={open} datasets={o.datasets} onClose={() => { setOpen(null); refresh(); }} />}
    </SettingsPage>
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

function BatchDialog({ id, datasets, onClose }: { id: string; datasets: Dataset[]; onClose: () => void }) {
  const qc = useQueryClient();
  const [filter, setFilter] = useState<string>("");
  const [skip, setSkip] = useState(false);
  const [mapping, setMapping] = useState<Record<string, string> | null>(null);
  const q = useQuery({ queryKey: ["onboarding-batch", id, filter], queryFn: () => api<BatchView>(`/onboarding/batches/${id}`, { query: { status: filter || undefined } }) });
  const reload = (v?: BatchView) => { if (v) qc.setQueryData(["onboarding-batch", id, filter], v); else void q.refetch(); qc.invalidateQueries({ queryKey: ["onboarding"] }); };
  const v = q.data;
  const b = v?.batch;
  const label = datasets.find((d) => d.key === b?.dataset)?.label ?? "";
  const editable = b?.status === "validated";
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
                const r = await api<{ status: string; summary?: Record<string, number> }>(`/onboarding/batches/${id}/post`, { body: { skip_invalid: skip } });
                toast.success(r.status === "pending_approval" ? "Sent for approval" : `Imported: ${r.summary?.posted ?? 0} row(s)`);
                reload();
              }}><CheckCircle2 /> {t("Import")} {count(b.total_rows - (skip ? b.invalid_rows : 0))} {t("rows")}</ActionButton>
          )}
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
                      return <td key={c.key} className={cn("max-w-[12rem] truncate whitespace-nowrap px-2 py-1.5", bad && "font-semibold text-destructive")}>{val(r, c)}</td>;
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
