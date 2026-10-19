/** Stock balances (roadmap 142): download a sheet prefilled with this business's products, branches and today's
 * balances, count and edit it in Excel, upload it, review Current → Uploaded → Difference for every row and apply it
 * (approval may apply). One panel, shown in Stock → Stock balances and in Settings → Onboarding (roadmap 144), on the
 * same import engine and screens as the other onboarding datasets. */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, FileSpreadsheet, Info } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { ago, count } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field, Select } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { ImportProgress } from "@/components/ImportProgress";
import { BatchDialog, fetchFile, UploadButton, type BatchRow, type Dataset } from "@/pages/settings/Onboarding";

const STATUS: Record<string, [Tone, string]> = {
  validated: ["info", "Ready to apply"], pending_approval: ["warning", "Awaiting approval"], posting: ["info", "Applying"], posted: ["success", "Applied"],
  rejected: ["danger", "Rejected"], cancelled: ["neutral", "Discarded"], failed: ["danger", "Failed"],
};

type Batches = { batches: (BatchRow & { summary?: Record<string, number> })[]; dataset: Dataset };

/** The controls and recent uploads; opening an upload (preview) or its progress is the host's job. */
export function StockBalancesPanel({ onOpen, onProgress }: { onOpen: (id: string) => void; onProgress: (id: string) => void }) {
  const qc = useQueryClient();
  const { profile, can } = useSession();
  const [branch, setBranch] = useState("");
  const q = useQuery({ queryKey: ["stock-balance-batches"], queryFn: () => api<Batches>("/onboarding/stock_balances/batches") });
  const refresh = () => { qc.invalidateQueries({ queryKey: ["stock-balance-batches"] }); qc.invalidateQueries({ queryKey: ["stock"] }); };
  const branches = profile?.branches ?? [];
  const today = new Date().toLocaleDateString("en-CA"); // local yyyy-mm-dd (not UTC)
  return (
    <div className="space-y-4 text-sm">
      <p className="flex gap-2 rounded-lg bg-primary/5 p-2.5 text-xs text-primary">
        <Info className="h-4 w-4 shrink-0" />
        {t("The quantity you upload is the balance you counted — S'Shop records only the difference (12 → 15 adds 3). Individually tracked products are reconciled item by item (stock take by scanning).")}
      </p>
      {branches.length > 1 && (
        <Field label="Branches in the sheet">
          <Select value={branch} onChange={setBranch}>
            <option value="">{t("All my branches")}</option>
            {branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
          </Select>
        </Field>
      )}
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={() => fetchFile(`/onboarding/stock_balances/template${branch ? `?branch_id=${branch}` : ""}`, `SShop_Stock_Balances_${today}.xlsx`).catch((e) => toast.error(e))}>
          <Download /> {t("Download template")}
        </Button>
        <UploadButton dataset="stock_balances" canUpdate={false} label="Upload stock balances" onDone={(id) => { refresh(); onOpen(id); }} />
        {can("reports.export") && (
          <Button variant="ghost" onClick={() => fetchFile(`/reports/stock_position?format=xlsx&period=today${branch ? `&branch_id=${branch}` : ""}`, `SShop_Current_Balances_${today}.xlsx`).catch((e) => toast.error(e))}>
            <FileSpreadsheet /> {t("Download current balances")}
          </Button>
        )}
      </div>
      <div>
        <p className="label-caps mb-1">{t("Recent uploads")}</p>
        <ul className="divide-y rounded-xl border">
          {(q.data?.batches ?? []).slice(0, 8).map((b) => (
            <li key={b.id}>
              <button className="flex w-full flex-wrap items-center gap-2 px-3 py-2.5 text-start hover:bg-muted/40" onClick={() => (b.status === "posting" ? onProgress(b.id) : onOpen(b.id))}>
                <Pill tone={STATUS[b.status]?.[0] ?? "neutral"}>{t(STATUS[b.status]?.[1] ?? b.status)}</Pill>
                <span className="min-w-0 flex-1 truncate">{b.file_name}{b.reference ? ` · ${b.reference}` : ""}</span>
                <span className="num text-xs text-muted-foreground">
                  {b.status === "posted" ? `${count(b.summary?.updated ?? 0)} ${t("updated")} · ${count(b.summary?.unchanged ?? 0)} ${t("unchanged")}` : `${count(b.total_rows)} ${t("rows")} · ${count(b.invalid_rows)} ${t("invalid")}`} · {b.uploaded_by} · {ago(b.created_at)}
                </span>
              </button>
            </li>
          ))}
          {q.data && !q.data.batches.length && <li className="px-3 py-3 text-muted-foreground">{t("No uploads yet.")}</li>}
        </ul>
      </div>
    </div>
  );
}

/** The preview / progress screens for a stock balance upload (shared by both hosts). */
export function StockBalanceScreens({ open, progress, setOpen, setProgress }: {
  open: string | null; progress: string | null; setOpen: (id: string | null) => void; setProgress: (id: string | null) => void;
}) {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["stock-balance-batches"], queryFn: () => api<Batches>("/onboarding/stock_balances/batches") });
  const refresh = () => { qc.invalidateQueries({ queryKey: ["stock-balance-batches"] }); qc.invalidateQueries({ queryKey: ["stock"] }); qc.invalidateQueries({ queryKey: ["onboarding"] }); };
  return (
    <>
      {open && q.data && (
        <BatchDialog id={open} datasets={[q.data.dataset]} onClose={() => { setOpen(null); refresh(); }} onProgress={(id) => { setOpen(null); setProgress(id); refresh(); }} />
      )}
      {progress && (
        <ImportProgress id={progress} label="Stock balances" onClose={() => { setProgress(null); refresh(); }}
          onCheckAgain={() => { const id = progress; setProgress(null); setOpen(id); }}
          errorReport={() => fetchFile(`/onboarding/batches/${progress}/errors`, "Fix_stock_balances.xlsx").catch((e) => toast.error(e))} />
      )}
    </>
  );
}

/** Stock → Stock balances. */
export function StockBalancesDialog({ onClose }: { onClose: () => void }) {
  const [open, setOpen] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  return (
    <>
      <ResponsiveDialog open={!open && !progress} onOpenChange={(o) => !o && onClose()} title="Stock balances" wide
        description="Download your balances, count and correct them in Excel, upload, review every difference, then apply.">
        <StockBalancesPanel onOpen={setOpen} onProgress={setProgress} />
      </ResponsiveDialog>
      <StockBalanceScreens open={open} progress={progress} setOpen={setOpen} setProgress={setProgress} />
    </>
  );
}
