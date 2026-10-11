/**
 * Import progress and result (roadmap 103) — one screen for every bulk import.
 *
 * Follows the real import on the server (`/onboarding/batches/{id}/progress`): stage, records processed, percentage.
 * Success is shown only once the server reports the import committed; a quick import gets a short completion
 * moment on screen (the import itself is never slowed). Closing it does not stop the import: reopening the import
 * (or the page) shows its latest state.
 */
import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { AlertTriangle, Check, CheckCircle2, Clock, Download, RefreshCw } from "lucide-react";
import { api, session } from "@/lib/api";
import { download } from "@/lib/receipt";
import { toast } from "@/lib/toast";
import { count, dateTime } from "@/lib/format";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { SShopLoader } from "@/components/SShopLoader";

export interface ImportSummary {
  total?: number; posted?: number; created?: number; updated?: number; matched?: number; skipped?: number; failed?: number;
}
interface ProgressView {
  id: string; dataset: string; mode: string; status: string; file_name: string; reference: string | null;
  total_rows: number; invalid_rows: number; posted_rows: number; summary: ImportSummary | null; error: string | null;
  started_at: string | null; posted_at: string | null; stage: Stage | null; done: number; total: number;
}
type Stage = "preparing" | "validating" | "importing" | "finalizing" | "completed" | "failed";

const STAGES: [Stage, string][] = [["preparing", "Preparing"], ["validating", "Validating"], ["importing", "Importing"], ["finalizing", "Finalizing"], ["completed", "Completed"]];
/** Where the imported records live. */
export const RECORDS_PAGE: Record<string, string> = {
  products: "/products", customers: "/customers", opening_stock: "/stock", expenses: "/expenses", sales: "/sales",
  credit: "/credit", loyalty: "/loyalty", orders: "/orders", transfers: "/transfers",
};
/** How long the completed state stays on screen before the summary (display only — the data is already saved). */
const COMPLETION_MS = 1100;

/** Real milestones mapped onto the bar: validation is the first stretch, rows imported fill most of it. */
function percent(v: ProgressView): number {
  if (v.status === "posted") return 100;
  switch (v.stage) {
    case "preparing": return 2;
    case "validating": return 8;
    case "importing": return 10 + Math.floor((85 * Math.min(v.done, v.total)) / Math.max(v.total, 1));
    case "finalizing": return 97;
    default: return 0;
  }
}

export function ImportProgress({ id, label, onClose, onCheckAgain, errorReport }: {
  id: string;
  /** Dataset name, e.g. "Products". */
  label: string;
  onClose: () => void;
  /** Back to the preview of this import (after a failure). */
  onCheckAgain?: () => void;
  /** Download the rows left out (to fix and upload again). */
  errorReport?: () => void;
}) {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ["onboarding-progress", id],
    queryFn: () => api<ProgressView>(`/onboarding/batches/${id}/progress`),
    refetchInterval: (query) => (query.state.data?.status === "posting" ? 400 : false),
    refetchIntervalInBackground: true,
  });
  const v = q.data;
  const sawRunning = useRef(false);
  const shown = useRef(0);
  const [celebrating, setCelebrating] = useState(false);
  if (v?.status === "posting") sawRunning.current = true;

  // The server says it is committed: a brief completed state when the person watched it run, then the summary.
  useEffect(() => {
    if (v?.status !== "posted" || !sawRunning.current) return;
    setCelebrating(true);
    const timer = setTimeout(() => setCelebrating(false), COMPLETION_MS);
    return () => clearTimeout(timer);
  }, [v?.status]);
  useEffect(() => {
    if (v && v.status !== "posting") void qc.invalidateQueries({ queryKey: ["onboarding"] });
  }, [v, qc]);

  const running = v?.status === "posting" || celebrating;
  const pct = v ? Math.max(shown.current, percent(v)) : 0;
  shown.current = v?.status === "posting" || v?.status === "posted" ? pct : 0;
  const stageNow: Stage = v?.status === "posted" ? "completed" : (v?.stage ?? "preparing");
  const reportUrl = `/onboarding/batches/${id}/report`;

  const title = !v ? `${t("Importing")} ${t(label)}`
    : running ? `${t("Importing")} ${t(label)}`
    : v.status === "posted" ? ((v.summary?.skipped ?? 0) > 0 ? t("Partially completed") : `${t(label)} ${t("imported successfully")}`)
    : v.status === "failed" ? t("Import failed")
    : v.status === "pending_approval" ? t("Sent for approval")
    : `${t(label)}`;

  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title={title} description={v ? `${v.file_name}${v.reference ? ` · ${v.reference}` : ""}` : undefined}
      footer={!running && v && (
        <div className="flex w-full flex-wrap items-center justify-end gap-2">
          {v.status === "failed" && onCheckAgain && <Button variant="outline" size="sm" onClick={onCheckAgain}><RefreshCw /> {t("Check again")}</Button>}
          {v.status === "posted" && (v.summary?.skipped ?? 0) > 0 && errorReport && (
            <Button variant="outline" size="sm" onClick={errorReport}><Download /> {t("Rows to fix")}</Button>
          )}
          {(v.status === "posted" || v.status === "failed") && (
            <Button variant="outline" size="sm" onClick={() => downloadReport(reportUrl, v.file_name)}><Download /> {t("Download import report")}</Button>
          )}
          {v.status === "posted" && RECORDS_PAGE[v.dataset] && (
            <Button asChild variant="outline" size="sm"><Link to={RECORDS_PAGE[v.dataset]} onClick={onClose}>{t("View imported records")}</Link></Button>
          )}
          <Button size="sm" onClick={onClose}>{t("Done")}</Button>
        </div>
      )}>
      {!v ? (
        <div className="flex justify-center py-8"><SShopLoader size="lg" /></div>
      ) : running ? (
        <div className="flex flex-col items-center gap-4 py-4 text-center" aria-live="polite">
          <div className="relative">
            {v.status === "posted" ? (
              <span className="flex h-20 w-20 items-center justify-center rounded-full bg-success/15 text-success animate-fade-up">
                <Check className="h-10 w-10" strokeWidth={2.5} />
              </span>
            ) : <SShopLoader size="lg" label={`${t("Importing")} ${t(label)}`} />}
          </div>
          <div className="w-full max-w-sm space-y-2">
            <div className="h-2.5 w-full overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
              <div className="h-full rounded-full transition-[width] duration-500 ease-out motion-reduce:transition-none"
                style={{ width: `${pct}%`, background: "linear-gradient(90deg, hsl(var(--brand-from)), hsl(var(--brand-to)))" }} />
            </div>
            <div className="flex items-baseline justify-between text-sm">
              <span className="num text-muted-foreground">{count(Math.min(v.status === "posted" ? v.total : v.done, v.total))} {t("of")} {count(v.total)} {t("records processed")}</span>
              <span className="num font-semibold">{pct}% {t("Complete")}</span>
            </div>
          </div>
          <ol className="flex w-full max-w-sm items-center justify-between text-[11px]">
            {STAGES.map(([k, l], i) => {
              const at = STAGES.findIndex(([s]) => s === stageNow);
              return (
                <li key={k} className={cn("flex flex-col items-center gap-1", i < at ? "text-success" : i === at ? "font-semibold text-primary" : "text-muted-foreground")}>
                  <span className={cn("h-1.5 w-1.5 rounded-full", i < at ? "bg-success" : i === at ? "bg-primary" : "bg-muted-foreground/40")} />
                  {t(l)}
                </li>
              );
            })}
          </ol>
          {v.status === "posting" && <p className="text-xs text-muted-foreground">{t("You can close this — the import continues and you can reopen it from Import history.")}</p>}
        </div>
      ) : v.status === "failed" ? (
        <div className="space-y-3 py-2 text-sm">
          <p className="flex items-start gap-2 rounded-lg bg-destructive/10 p-3 text-destructive"><AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" /> {v.error ?? t("The import failed.")}</p>
          <p className="text-muted-foreground">{t("Nothing from this file was saved. Check it again (the rows show what changed), fix the file if needed, then import it again.")}</p>
        </div>
      ) : v.status === "pending_approval" ? (
        <p className="flex items-start gap-2 py-2 text-sm"><Clock className="mt-0.5 h-4 w-4 shrink-0 text-warning" /> {t("Waiting for approval in Approvals. Nothing is saved until it is approved.")}</p>
      ) : (
        <Summary v={v} />
      )}
    </ResponsiveDialog>
  );
}

function Summary({ v }: { v: ProgressView }) {
  const s = v.summary ?? {};
  const partial = (s.skipped ?? 0) > 0;
  const lines: [string, number | undefined, boolean][] = [
    ["Records processed", s.total ?? v.total_rows, true],
    ["Successfully imported", s.posted ?? v.posted_rows, true],
    ["Created", s.created, true],
    ["Updated", s.updated, v.dataset === "products" && v.mode === "update"],
    ["Matched existing (not duplicated)", s.matched, v.dataset === "customers" || (s.matched ?? 0) > 0],
    ["Left out (invalid)", s.skipped, true],
    ["Failed", s.failed ?? 0, true],
  ];
  return (
    <div className="space-y-3 py-1 text-sm">
      <div className={cn("flex items-center gap-2 rounded-lg p-3 font-medium", partial ? "bg-warning/10 text-warning" : "bg-success/10 text-success")}>
        {partial ? <AlertTriangle className="h-4 w-4 shrink-0" /> : <CheckCircle2 className="h-4 w-4 shrink-0" />}
        {partial ? t("Imported, with some rows left out — fix them and upload them again (already imported rows are never duplicated).") : t("Everything in the file was imported.")}
      </div>
      <ul className="space-y-1.5">
        {lines.filter(([, n, show]) => show && n !== undefined).map(([l, n]) => {
          const warn = (l === "Left out (invalid)" || l === "Failed") && (n ?? 0) > 0;
          return (
            <li key={l} className="flex items-center gap-2">
              <span className={cn("flex h-5 w-5 items-center justify-center rounded-full", warn ? "bg-warning/15 text-warning" : "bg-success/15 text-success")}>
                {warn ? <AlertTriangle className="h-3 w-3" /> : <Check className="h-3 w-3" strokeWidth={3} />}
              </span>
              <span className="num w-14 text-end font-semibold">{count(n ?? 0)}</span>
              <span>{t(l)}</span>
            </li>
          );
        })}
      </ul>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 rounded-lg border p-3 text-xs">
        <dt className="text-muted-foreground">{t("Imported at")}</dt><dd className="num">{dateTime(v.posted_at)}</dd>
        <dt className="text-muted-foreground">{t("Import reference")}</dt><dd className="num">{v.reference ?? "—"}</dd>
      </dl>
    </div>
  );
}

async function downloadReport(path: string, file: string) {
  try {
    const res = await fetch(`/api${path}`, { headers: { Authorization: `Bearer ${session.token}` } });
    if (!res.ok) throw new Error(t("The file could not be downloaded"));
    download(await res.blob(), `Import_report_${file.replace(/\.xlsx$/i, "")}.xlsx`);
  } catch (e) {
    toast.error(e);
  }
}
