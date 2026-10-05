import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, ShieldCheck, Undo2, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { ago, dateTime, money } from "@/lib/format";
import type { Approval, Paged } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { EmptyState, Loading, PageHeader } from "@/components/Page";
import { Segments } from "@/components/Filters";
import { Pill, StatusBadge } from "@/components/Badges";
import { Field } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

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
  expense: "Expense",
};

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

export default function Approvals() {
  const { currency } = useSession();
  const qc = useQueryClient();
  const [status, setStatus] = useState<"pending" | "mine" | "approved" | "rejected">("pending");
  const [deciding, setDeciding] = useState<{ approval: Approval; approve: boolean } | null>(null);
  const [comments, setComments] = useState("");
  const query = status === "mine" ? { status: "all", mine: true } : { status };
  const { data, isLoading } = useQuery({ queryKey: ["approvals", query], queryFn: () => api<Paged<Approval>>("/approvals", { query }) });
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["approvals"] });
    qc.invalidateQueries({ queryKey: ["notifications"] });
  };
  const decide = useMutation({
    mutationFn: () => api<unknown>(`/approvals/${deciding!.approval.id}/${deciding!.approve ? "approve" : "reject"}`, { body: { comments } }),
    onSuccess: (res) => {
      const r = res as { status?: string; level?: number; levels?: number };
      toast.success(!deciding!.approve ? "Rejected" : r.status === "pending" ? `Approved — passed to level ${r.level} of ${r.levels}` : "Approved — action carried out");
      setDeciding(null);
      setComments("");
      refresh();
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const withdraw = useMutation({
    mutationFn: (id: string) => api(`/approvals/${id}/withdraw`, { method: "POST" }),
    onSuccess: () => { toast.success("Request withdrawn"); refresh(); },
    onError: (e) => toast.error(errorMessage(e)),
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
              <div key={a.id} className="surface flex flex-col gap-3 p-4 animate-fade-up">
                <div className="flex items-start justify-between gap-2">
                  <Pill tone="primary">{ACTION_LABEL[a.action] ?? a.action}</Pill>
                  <span className="flex items-center gap-1.5">
                    {a.status === "pending" && a.levels > 1 && <Pill tone="info">Level {a.level} of {a.levels}</Pill>}
                    <StatusBadge status={a.status} />
                  </span>
                </div>
                <div>
                  <p className="font-medium">{a.summary}</p>
                  {a.amount !== null && <p className="num mt-1 text-lg font-semibold">{money(a.amount, currency)}</p>}
                </div>
                <p className="text-xs text-muted-foreground">
                  {a.requested_by_name} · {ago(a.created_at)}{a.branch_name && ` · ${a.branch_name}`}
                  {a.decided_by_name && <><br />{a.status} by {a.decided_by_name} · {dateTime(a.decided_at)}{a.comments && ` — “${a.comments}”`}</>}
                </p>
                {a.decisions.length > 0 && (
                  <ol className="space-y-1 border-l-2 pl-3 text-xs">
                    {a.decisions.map((d, n) => (
                      <li key={n} className={d.decision === "approved" ? "text-success" : "text-destructive"}>
                        Level {d.level} {d.decision} by {d.user_name} · {ago(d.at)}{d.comments && <span className="text-muted-foreground"> — “{d.comments}”</span>}
                      </li>
                    ))}
                  </ol>
                )}
                <div className="mt-auto flex flex-wrap gap-2">
                  {link && <Button variant="ghost" size="sm" asChild><Link to={link}>View</Link></Button>}
                  {a.can_decide && (
                    <>
                      <Button size="sm" variant="outline" className="ml-auto text-destructive" onClick={() => setDeciding({ approval: a, approve: false })}><X /> Reject</Button>
                      <Button size="sm" variant="success" onClick={() => setDeciding({ approval: a, approve: true })}><Check /> Approve</Button>
                    </>
                  )}
                  {status === "mine" && a.status === "pending" && (
                    <Button size="sm" variant="outline" className="ml-auto" onClick={() => withdraw.mutate(a.id)}><Undo2 /> Withdraw</Button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
      <ResponsiveDialog
        open={!!deciding}
        onOpenChange={(o) => !o && setDeciding(null)}
        title={deciding?.approve ? "Approve request" : "Reject request"}
        description={deciding?.approval.summary}
        footer={
          <Button className="w-full md:w-auto" variant={deciding?.approve ? "success" : "destructive"} disabled={decide.isPending} onClick={() => decide.mutate()}>
            {deciding?.approve ? "Approve" : "Reject"}
          </Button>
        }
      >
        <Field label="Comment" optional><Textarea value={comments} onChange={(e) => setComments(e.target.value)} /></Field>
      </ResponsiveDialog>
    </>
  );
}
