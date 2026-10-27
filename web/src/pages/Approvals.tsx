import { useState } from "react";
import { Link } from "@/lib/router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Check, Info, ShieldCheck, Undo2, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { ago, dateTime, money } from "@/lib/format";
import type { Approval, Paged } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Textarea } from "@/components/ui/textarea";
import { EmptyState, Loading, PageHeader } from "@/components/Page";
import { Segments } from "@/components/Filters";
import { Pill, StatusBadge } from "@/components/Badges";
import { Field } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { t } from "@/lib/i18n";

const ACTION_LABEL: Record<string, string> = {
  "product.create": "New product",
  "product.edit": "Product edit",
  "product.deactivate": "Deactivation",
  "stock.add": "Stock receipt",
  "stock.adjust": "Stock adjustment",
  "stock.write_off": "Write-off",
  "stock.transfer": "Transfer",
  "sale.discount": "Discount",
  "sale.return": "Return",
  "sale.cancel": "Cancellation",
  "credit.write_off": "Credit write-off",
  "sale.owner_change": "Sale ownership change",
  "credit.recall": "Credit recall",
  expense: "Expense",
  "data.import": "Data import",
};

/** Roadmap 148: a request nobody can decide — why, and the controlled ways forward (never approved automatically). */
function StuckNote({ a }: { a: Approval }) {
  if (a.status !== "pending" || !a.stuck) return null;
  const who = t(a.assigned_to);
  return (
    <div className="flex gap-2 rounded-lg bg-warning/10 p-2.5 text-xs text-warning">
      <AlertTriangle className="h-4 w-4 shrink-0" />
      <span>
        <b>{t("Nobody can approve this request yet.")}</b>{" "}
        {a.stuck === "only_requester"
          ? t("This step is for {who}, and the only person who qualifies made the request — nobody approves their own request.").replace("{who}", who)
          : t("This step is for {who}, and no active person qualifies.").replace("{who}", who)}{" "}
        {t("An administrator can choose another approver in Settings → Workflow engine (this request follows the change, recorded in the audit trail), or the requester can withdraw it.")}
      </span>
    </div>
  );
}

/** Why the signed-in person cannot decide a pending request (instead of a silently missing Approve button). */
function WhyNote({ a }: { a: Approval }) {
  if (a.status !== "pending" || a.can_decide || !a.why_message) return null;
  return (
    <p className="flex gap-1.5 text-xs text-muted-foreground">
      <Info className="mt-px h-3.5 w-3.5 shrink-0" /> {t(a.why_message)}
    </p>
  );
}

function entityLink(a: Approval) {
  switch (a.entity_type) {
    case "product":
      return `/products/${a.entity_id}`;
    case "sale":
      return `/sales/${a.entity_id}`;
    case "transfer":
      return `/transfers/${a.entity_id}`;
    case "credit_sale":
      return `/credit/${a.entity_id}`;
    case "expense":
      return "/expenses";
    case "stock_adjustment":
      return "/stock?tab=adjustments";
    default:
      return null;
  }
}

/** A decision shown at its step's position in the request's current chain (a workflow change can move steps). */
function decisionStep(a: Approval, d: Approval["decisions"][number]): string {
  const i = d.step_id ? a.steps.findIndex((s) => s.id === d.step_id) : -1;
  if (i >= 0) return `${t("Level")} ${i + 1}`;
  return d.step_id ? t("Earlier step") : `${t("Level")} ${d.level}`;
}

export default function Approvals() {
  
  const qc = useQueryClient();
  const [status, setStatus] = useState<"pending" | "mine" | "approved" | "rejected">("pending");
  const [deciding, setDeciding] = useState<{ approval: Approval; approve: boolean } | null>(null);
  const [details, setDetails] = useState<Approval | null>(null);
  const [comments, setComments] = useState("");
  const query = status === "mine" ? { status: "all", mine: true } : { status };
  const { data, isLoading } = useQuery({ queryKey: ["approvals", query], queryFn: () => api<Paged<Approval>>("/approvals", { query }) });
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["approvals"] });
    qc.invalidateQueries({ queryKey: ["notifications"] });
  };
  const decide = useMutation({
    // Roadmap 150: the level this person saw — if another approver decided it first, the server answers "Already handled".
    mutationFn: () => api<unknown>(`/approvals/${deciding!.approval.id}/${deciding!.approve ? "approve" : "reject"}`, { body: { comments, level: deciding!.approval.level } }),
    onSuccess: (res) => {
      const r = res as { status?: string; level?: number; levels?: number };
      toast.success(!deciding!.approve ? "Rejected" : r.status === "pending" ? `Approved — passed to level ${r.level} of ${r.levels}` : "Approved — action carried out");
      setDeciding(null);
      setComments("");
      refresh();
    },
    onError: (e) => { toast.error(e); setDeciding(null); refresh(); },
  });
  const withdraw = useMutation({
    mutationFn: (id: string) => api(`/approvals/${id}/withdraw`, { method: "POST" }),
    onSuccess: () => { toast.success("Request withdrawn"); refresh(); },
    onError: (e) => toast.error(e),
  });

  return (
    <>
      <PageHeader eyebrow="Workflow" title="Approvals" description="Sensitive actions wait here for a second person (maker-checker)." />
      <div className="mb-4">
        <Segments value={status} onChange={setStatus} options={[{ value: "pending", label: "Waiting" }, { value: "mine", label: "My requests" }, { value: "approved", label: "Approved" }, { value: "rejected", label: "Rejected" }]} />
      </div>
      {isLoading ? (
        <Loading />
      ) : !data?.items.length ? (
        <div className="surface"><EmptyState icon={ShieldCheck} title={status === "pending" ? "Nothing waiting for approval" : "Nothing here"} /></div>
      ) : (
        <div className="grid gap-3 lg:grid-cols-2 2xl:grid-cols-3">
          {data.items.map((a) => {
            const link = entityLink(a);
            return (
              <div key={a.id} className="surface card-body flex flex-col gap-3 animate-fade-up">
                <div className="flex items-start justify-between gap-2">
                  <Pill tone="primary">{ACTION_LABEL[a.action] ?? a.action}</Pill>
                  <span className="flex items-center gap-1.5">
                    {a.status === "pending" && a.levels > 1 && <Pill tone="info">{t("Level")} {a.level} of {a.levels}</Pill>}
                    <StatusBadge status={a.status} />
                  </span>
                </div>
                <div>
                  <p className="font-medium">{a.summary}</p>
                  {a.amount !== null && <p className="num mt-1 text-lg font-semibold">{money(a.amount)}</p>}
                </div>
                <p className="text-xs text-muted-foreground">
                  {a.requested_by_name} · {ago(a.created_at)}{a.branch_name && ` · ${a.branch_name}`}
                  {a.decided_by_name && <><br />{a.status} by {a.decided_by_name} · {dateTime(a.decided_at)}{a.comments && ` — “${a.comments}”`}</>}
                </p>
                {a.status === "pending" && (
                  <p className="text-xs">
                    <span className="text-muted-foreground">{t("Assigned to")}:</span> <span className="font-medium">{t(a.assigned_to)}</span>
                    {a.next_approvers.length > 0 && <><span className="text-muted-foreground"> · {t("Next approver")}:</span> <span className="font-medium">{a.next_approvers.join(", ")}</span></>}
                  </p>
                )}
                <StuckNote a={a} />
                <WhyNote a={a} />
                {a.status === "pending" && a.sync_note && (
                  <p className="rounded-lg bg-warning/10 p-2 text-xs text-warning">{t("Workflow changed")}: {t(a.sync_note)}</p>
                )}
                {a.decisions.length > 0 && (
                  <ol className="space-y-1 border-s-2 ps-3 text-xs">
                    {a.decisions.map((d, n) => (
                      <li key={n} className={d.decision === "approved" ? "text-success" : "text-destructive"}>
                        {decisionStep(a, d)} {d.decision} by {d.user_name}{d.self_approved && ` · ${t("self-approved (Tenant Administrator)")}`} · {ago(d.at)}{d.comments && <span className="text-muted-foreground"> — “{d.comments}”</span>}
                      </li>
                    ))}
                  </ol>
                )}
                <div className="mt-auto flex flex-wrap gap-2">
                  <Button variant="ghost" size="sm" onClick={() => setDetails(a)}>{t("View")}</Button>
                  {link && <Button variant="ghost" size="sm" asChild><Link to={link}>{t("Open")}</Link></Button>}
                  {a.can_decide && (
                    <>
                      <Button size="sm" variant="outline" className="ms-auto text-destructive" onClick={() => setDeciding({ approval: a, approve: false })}><X /> {t("Reject")}</Button>
                      <Button size="sm" variant="success" onClick={() => setDeciding({ approval: a, approve: true })}><Check /> {t("Approve")}</Button>
                    </>
                  )}
                  {status === "mine" && a.status === "pending" && (
                    <ActionButton size="sm" variant="outline" className="ms-auto" online busy={withdraw.isPending && withdraw.variables === a.id} disabled={withdraw.isPending}
                      busyLabel="Withdrawing…" onAction={() => withdraw.mutateAsync(a.id)}><Undo2 /> {t("Withdraw")}</ActionButton>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
      <ResponsiveDialog
        open={!!details}
        onOpenChange={(o) => !o && setDetails(null)}
        title={details ? t(ACTION_LABEL[details.action] ?? details.action) : ""}
        description={details?.summary}
        footer={details?.can_decide ? (
          <div className="flex w-full gap-2 md:w-auto">
            <Button variant="outline" className="flex-1 text-destructive md:flex-none" onClick={() => { setDeciding({ approval: details, approve: false }); setDetails(null); }}><X /> {t("Reject")}</Button>
            <Button variant="success" className="flex-1 md:flex-none" onClick={() => { setDeciding({ approval: details, approve: true }); setDetails(null); }}><Check /> {t("Approve")}</Button>
          </div>
        ) : undefined}
      >
        {details && (
          <div className="space-y-3 text-sm">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
              <dt className="text-muted-foreground">{t("Request")}</dt><dd>{t(ACTION_LABEL[details.action] ?? details.action)}</dd>
              <dt className="text-muted-foreground">{t("Requested by")}</dt><dd>{details.requested_by_name ?? "—"}</dd>
              <dt className="text-muted-foreground">{t("Submitted")}</dt><dd>{dateTime(details.created_at)}</dd>
              {details.branch_name && <><dt className="text-muted-foreground">{t("Branch")}</dt><dd>{details.branch_name}</dd></>}
              {details.amount !== null && <><dt className="text-muted-foreground">{t("Amount")}</dt><dd className="num">{money(details.amount)}</dd></>}
              <dt className="text-muted-foreground">{t("Status")}</dt><dd><StatusBadge status={details.status} /></dd>
              {details.status === "pending" && <>
                <dt className="text-muted-foreground">{t("Current level")}</dt><dd>{details.level} {t("of")} {details.levels}</dd>
                <dt className="text-muted-foreground">{t("Assigned to")}</dt><dd>{t(details.assigned_to)}{details.next_approvers.length > 0 && ` — ${details.next_approvers.join(", ")}`}</dd>
              </>}
              {details.decided_by_name && <><dt className="text-muted-foreground">{t("Decided by")}</dt><dd>{details.decided_by_name} · {dateTime(details.decided_at)}</dd></>}
            </dl>
            <StuckNote a={details} />
            <WhyNote a={details} />
            <div>
              <p className="label-caps mb-1">{t("Approval history")}</p>
              {details.decisions.length ? (
                <ol className="space-y-1 border-s-2 ps-3 text-xs">
                  {details.decisions.map((d, n) => (
                    <li key={n} className={d.decision === "approved" ? "text-success" : "text-destructive"}>
                      {decisionStep(details, d)} {t(d.decision)} {t("by")} {d.user_name}{d.self_approved && ` · ${t("self-approved (Tenant Administrator)")}`} · {dateTime(d.at)}{d.comments && <span className="text-muted-foreground"> — “{d.comments}”</span>}
                    </li>
                  ))}
                </ol>
              ) : <p className="text-xs text-muted-foreground">{t("No decisions yet.")}</p>}
            </div>
            {entityLink(details) && <Button variant="outline" size="sm" asChild><Link to={entityLink(details)!}>{t("Open record")}</Link></Button>}
          </div>
        )}
      </ResponsiveDialog>
      <ResponsiveDialog
        open={!!deciding}
        onOpenChange={(o) => !o && setDeciding(null)}
        title={deciding?.approve ? "Approve request" : "Reject request"}
        description={deciding?.approval.summary}
        footer={
          <ActionButton className="w-full md:w-auto" variant={deciding?.approve ? "success" : "destructive"} online busy={decide.isPending}
            busyLabel={deciding?.approve ? "Approving…" : "Rejecting…"} onAction={() => decide.mutateAsync()}>
            {deciding?.approve ? "Approve" : "Reject"}
          </ActionButton>
        }
      >
        <Field label="Comment" optional><Textarea value={comments} onChange={(e) => setComments(e.target.value)} /></Field>
      </ResponsiveDialog>
    </>
  );
}
