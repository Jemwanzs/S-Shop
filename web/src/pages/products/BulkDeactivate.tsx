/** Roadmap 151: deactivate several products at once. The server previews every selected product (eligible, already
 * inactive, already waiting for approval), the person confirms with an optional reason, and every product gets its own
 * outcome — deactivated, submitted for approval (products stay active until approved), skipped or failed. Nothing is
 * skipped silently. */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Clock, Info, MinusCircle, XCircle } from "lucide-react";
import { api } from "@/lib/api";
import { count } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { ActionButton } from "@/components/ActionButton";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Field } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Loading } from "@/components/Page";

interface Preview {
  items: { id: string; name: string; status: "eligible" | "already_inactive" | "already_waiting" }[];
  not_found: number;
  needs_approval: boolean;
}
interface Result {
  bulk_ref: string;
  summary: { selected: number; deactivated: number; pending_approval: number; already_inactive: number; already_waiting: number; failed: number };
  items: { id: string; name: string | null; outcome: string; message?: string }[];
}

const OUTCOME: Record<string, [Tone, string]> = {
  eligible: ["info", "Will be deactivated"],
  deactivated: ["success", "Deactivated"],
  pending_approval: ["warning", "Waiting for approval"],
  already_inactive: ["neutral", "Already inactive"],
  already_waiting: ["neutral", "Already waiting for approval"],
  failed: ["danger", "Failed"],
};

export function BulkDeactivate({ ids, onClose, onDone }: { ids: string[]; onClose: () => void; onDone: (handled: string[]) => void }) {
  const qc = useQueryClient();
  const [reason, setReason] = useState("");
  const [result, setResult] = useState<Result | null>(null);
  const preview = useQuery({
    queryKey: ["bulk-deactivate-preview", ids],
    queryFn: () => api<Preview>("/products/bulk-preview", { body: { ids } }),
    gcTime: 0,
  });
  const eligible = preview.data?.items.filter((i) => i.status === "eligible") ?? [];
  const submit = useMutation({
    mutationFn: () => api<Result>("/products/bulk-deactivate", { body: { ids: eligible.map((i) => i.id), reason: reason.trim() } }),
    onSuccess: (r) => {
      setResult(r);
      qc.invalidateQueries({ queryKey: ["products"] });
      qc.invalidateQueries({ queryKey: ["approvals"] });
    },
    onError: (e) => toast.error(e),
  });
  useEffect(() => setResult(null), [ids]);

  const approval = !!preview.data?.needs_approval;
  const title = result ? "Bulk deactivation results" : "Deactivate products";
  return (
    <ResponsiveDialog
      open
      onOpenChange={(o) => !o && (result ? onDone(result.items.filter((i) => i.outcome !== "failed").map((i) => i.id)) : onClose())}
      title={title}
      description={result ? undefined : `${count(ids.length)} ${t(ids.length === 1 ? "product selected" : "products selected")}`}
      footer={
        result ? (
          <Button className="w-full md:w-auto" onClick={() => onDone(result.items.filter((i) => i.outcome !== "failed").map((i) => i.id))}>{t("Done")}</Button>
        ) : (
          <div className="flex w-full gap-2 md:w-auto">
            <Button variant="outline" className="flex-1 md:flex-none" onClick={onClose}>{t("Cancel")}</Button>
            <ActionButton variant="destructive" className="flex-1 md:flex-none" online busy={submit.isPending} busyLabel="Working…"
              blockedBy={[!preview.data && "Loading…", preview.data && eligible.length === 0 && "No selected product can be deactivated"]}
              onAction={() => submit.mutateAsync()}>
              {approval ? `${t("Submit for approval")} (${count(eligible.length)})` : `${t("Deactivate")} ${count(eligible.length)}`}
            </ActionButton>
          </div>
        )
      }
    >
      {result ? (
        <div className="space-y-3 text-sm">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            <Stat icon={CheckCircle2} tone="text-success" label="Deactivated" n={result.summary.deactivated} />
            <Stat icon={Clock} tone="text-warning" label="Waiting for approval" n={result.summary.pending_approval} />
            <Stat icon={MinusCircle} tone="text-muted-foreground" label="Already inactive" n={result.summary.already_inactive} />
            <Stat icon={MinusCircle} tone="text-muted-foreground" label="Already waiting" n={result.summary.already_waiting} />
            <Stat icon={XCircle} tone="text-destructive" label="Failed" n={result.summary.failed} />
          </div>
          {result.summary.pending_approval > 0 && (
            <p className="flex gap-2 rounded-lg bg-warning/10 p-2.5 text-xs text-warning">
              <Info className="h-4 w-4 shrink-0" /> {t("These products stay active until their deactivation is approved in Approvals.")}
            </p>
          )}
          <ItemList items={result.items.map((i) => ({ id: i.id, name: i.name ?? "—", status: i.outcome, message: i.message }))} />
        </div>
      ) : !preview.data ? (
        <Loading />
      ) : (
        <div className="space-y-3 text-sm">
          {approval && (
            <p className="flex gap-2 rounded-lg bg-primary/5 p-2.5 text-xs text-primary">
              <Info className="h-4 w-4 shrink-0" /> {t("Product deactivation needs approval: a request is created for each product, and the products stay active until it is approved.")}
            </p>
          )}
          <p>
            <b className="num">{count(eligible.length)}</b> {t("can be deactivated")}
            {preview.data.items.length - eligible.length > 0 && <> · <span className="text-muted-foreground"><span className="num">{count(preview.data.items.length - eligible.length)}</span> {t("will be skipped (shown below)")}</span></>}
            {preview.data.not_found > 0 && <> · <span className="text-destructive"><span className="num">{preview.data.not_found}</span> {t("not found")}</span></>}
          </p>
          <ItemList items={preview.data.items} />
          <Field label="Reason" optional>
            <Textarea value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("e.g. discontinued after the catalogue review")} />
          </Field>
        </div>
      )}
    </ResponsiveDialog>
  );
}

function Stat({ icon: Icon, tone, label, n }: { icon: typeof Info; tone: string; label: string; n: number }) {
  return (
    <div className="rounded-xl border p-2.5">
      <div className={`flex items-center gap-1.5 text-xs ${tone}`}><Icon className="h-3.5 w-3.5" /> {t(label)}</div>
      <div className="num text-lg font-semibold">{count(n)}</div>
    </div>
  );
}

function ItemList({ items }: { items: { id: string; name: string; status: string; message?: string }[] }) {
  return (
    <ul className="scroll-thin max-h-64 divide-y overflow-y-auto rounded-xl border">
      {items.map((i) => (
        <li key={i.id} className="flex flex-wrap items-center gap-2 px-3 py-2">
          <span className="min-w-0 flex-1 truncate">{i.name}</span>
          <Pill tone={OUTCOME[i.status]?.[0] ?? "neutral"}>{t(OUTCOME[i.status]?.[1] ?? i.status)}</Pill>
          {i.message && i.status === "failed" && <span className="basis-full text-xs text-destructive">{i.message}</span>}
        </li>
      ))}
    </ul>
  );
}
