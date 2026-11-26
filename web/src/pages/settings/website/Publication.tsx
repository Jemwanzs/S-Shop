/** Roadmap 188: website publication requests — the summary of what would be published (generated from the real
 * differences), the Request Publication dialog, the publication history, and the request as an approver sees it. */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Eye, Send } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { toast } from "@/lib/toast";
import { t } from "@/lib/i18n";
import { count, dateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { Segments } from "@/components/Filters";
import { Card } from "../shared";
import { PreviewDialog } from "./Website";

export interface PubField { field: string; before: string; after: string }
export interface PubItem { ref: unknown; title: string | null; kind: string; fields: PubField[] }
export interface PubSection { part: string; label: string; headline: string; count: number; items: PubItem[] }
export type PubStatus = "unpublished" | "awaiting" | "returned" | "rejected" | "published" | "withdrawn";
export interface PubCurrent {
  status: PubStatus; summary: PubSection[]; total: number; version: number;
  waiting: { id: string; requested_by: string | null; requested_at: string; outdated: boolean } | null;
  last_comments: string | null; can_publish: boolean; can_request: boolean;
}

export const PUB_STATUS: Record<string, [string, Tone]> = {
  unpublished: ["Unpublished changes", "warning"],
  awaiting: ["Awaiting approval", "info"],
  returned: ["Returned for correction", "warning"],
  rejected: ["Rejected", "danger"],
  published: ["Published", "success"],
  withdrawn: ["Withdrawn", "neutral"],
};

export const usePublication = (enabled = true) =>
  useQuery({ queryKey: ["website-publication"], queryFn: () => api<PubCurrent>("/website/publication"), enabled });

/** The sections, each expandable to its records and their previous → proposed values. */
export function PublicationSummary({ sections }: { sections: PubSection[] }) {
  const [open, setOpen] = useState<string | null>(null);
  if (!sections.length) return <p className="text-sm text-muted-foreground">{t("No unpublished changes.")}</p>;
  return (
    <ul className="divide-y rounded-xl border" data-testid="publication-summary">
      {sections.map((s) => (
        <li key={s.part}>
          <button type="button" className="flex w-full items-center gap-2 p-2.5 text-start text-sm" aria-expanded={open === s.part} onClick={() => setOpen(open === s.part ? null : s.part)}>
            <span className="min-w-0 flex-1"><b>{t(s.label)}</b> — {s.headline}</span>
            <ChevronDown className={cn("h-4 w-4 shrink-0 text-muted-foreground transition-transform", open === s.part && "rotate-180")} />
          </button>
          {open === s.part && (
            <ul className="space-y-2 px-3 pb-3 text-xs">
              {s.items.map((i, n) => (
                <li key={n} className="rounded-lg bg-muted/50 p-2">
                  <div className="flex flex-wrap items-center gap-1.5"><span className="font-medium">{i.title ?? t("Entry")}</span><Pill>{t(i.kind)}</Pill></div>
                  {i.fields.length > 0 && (
                    <dl className="mt-1 space-y-0.5">
                      {i.fields.slice(0, 12).map((f, k) => (
                        <div key={k} className="break-words"><dt className="inline text-muted-foreground">{f.field}: </dt><dd className="inline"><span className="line-through opacity-70">{f.before}</span> → <span className="font-medium">{f.after}</span></dd></div>
                      ))}
                    </dl>
                  )}
                </li>
              ))}
            </ul>
          )}
        </li>
      ))}
    </ul>
  );
}

/** Request Website Publication: the generated summary, who and when, comments, preview — then submit. */
export function RequestPublicationDialog({ open, onClose, slug, draftUpdatedAt }: { open: boolean; onClose: () => void; slug: string; draftUpdatedAt: string | null }) {
  const { profile } = useSession();
  const qc = useQueryClient();
  const pub = usePublication(open);
  const [comments, setComments] = useState("");
  const [preview, setPreview] = useState(false);
  const d = pub.data;
  return (
    <>
      <ResponsiveDialog open={open} onOpenChange={(o) => !o && onClose()} wide title="Request website publication"
        description="The following changes will be submitted for review. The live website does not change until the request is approved."
        footer={
          <div className="flex w-full flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            <Button variant="outline" onClick={onClose}>{t("Cancel")}</Button>
            <ActionButton online blockedBy={[!d?.total && "No unpublished changes", !!d?.waiting && "A request is already waiting"]}
              onAction={async () => {
                await api("/website/publication/requests", { body: { comments, draft_updated_at: draftUpdatedAt } });
                toast.success("Publication request submitted — the approver has been notified");
                setComments("");
                qc.invalidateQueries({ queryKey: ["website-publication"] });
                qc.invalidateQueries({ queryKey: ["website-publication-history"] });
                onClose();
              }}><Send /> {t("Submit request")}</ActionButton>
          </div>
        }>
        {!d ? <p className="py-4 text-sm text-muted-foreground">{t("Loading…")}</p> : (
          <div className="space-y-3">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
              <dt className="text-muted-foreground">{t("Requested by")}</dt><dd>{profile?.user.name}</dd>
              <dt className="text-muted-foreground">{t("Date and time")}</dt><dd>{dateTime(new Date().toISOString())}</dd>
              <dt className="text-muted-foreground">{t("Total changes")}</dt><dd className="num font-semibold">{count(d.total)}</dd>
            </dl>
            <PublicationSummary sections={d.summary} />
            <Button variant="outline" size="sm" onClick={() => setPreview(true)}><Eye /> {t("Preview proposed changes")}</Button>
            <Field label="Additional comments" optional><Textarea value={comments} maxLength={1000} onChange={(e) => setComments(e.target.value)} /></Field>
          </div>
        )}
      </ResponsiveDialog>
      <PreviewDialog open={preview} onOpenChange={setPreview} slug={slug} />
    </>
  );
}

interface HistoryRow {
  id: string; requested_at: string; requested_by: string | null; comments: string; sections: { label: string; headline: string }[];
  total: number; status: PubStatus; decided_by: string | null; decided_at: string | null; decision_comments: string; published_version: number | null;
}

/** Publication History: every request (publishers and approvers) or the person's own. */
export function PublicationHistory() {
  const [mine, setMine] = useState("all");
  const q = useQuery({
    queryKey: ["website-publication-history", mine],
    queryFn: () => api<{ items: HistoryRow[]; only_mine: boolean }>("/website/publication/requests", { query: { mine: mine === "mine" } }),
  });
  const items = q.data?.items ?? [];
  return (
    <Card title="Publication history" action={q.data && !q.data.only_mine || mine === "mine" ? <Segments value={mine} onChange={setMine} options={[{ value: "all", label: "All" }, { value: "mine", label: "My requests" }]} /> : undefined}>
      {!items.length ? <p className="py-3 text-sm text-muted-foreground">{t("No publication requests yet.")}</p> : (
        <ul className="divide-y" data-testid="publication-history">
          {items.map((r) => {
            const [label, tone] = PUB_STATUS[r.status] ?? [r.status, "neutral"];
            return (
              <li key={r.id} className="space-y-1 py-3 text-sm">
                <div className="flex flex-wrap items-center gap-2"><Pill tone={tone}>{t(label)}</Pill><span className="num text-xs text-muted-foreground">{dateTime(r.requested_at)}</span><span className="text-xs text-muted-foreground">· {r.requested_by ?? "—"} · {count(r.total)} {t("changes")}</span></div>
                <p className="text-xs">{r.sections.map((s) => `${t(s.label)} — ${s.headline}`).join(" · ")}</p>
                {r.comments && <p className="text-xs text-muted-foreground">“{r.comments}”</p>}
                {r.decided_by && <p className="text-xs text-muted-foreground">{t(label)} {t("by")} {r.decided_by} · {r.decided_at && dateTime(r.decided_at)}{r.published_version ? ` · v${r.published_version}` : ""}{r.decision_comments && ` — “${r.decision_comments}”`}</p>}
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

/** The request as an approver reviews it (Approvals): its sections, the comments and a preview of the exact snapshot. */
export function PublicationApprovalLines({ id }: { id: string }) {
  const q = useQuery({
    queryKey: ["website-publication-request", id],
    queryFn: () => api<{ summary: PubSection[]; total: number; comments: string; outdated: boolean; preview_query: string; slug: string; status: PubStatus }>(`/website/publication/requests/${id}`),
  });
  const [preview, setPreview] = useState(false);
  if (!q.data) return null;
  const d = q.data;
  return (
    <div className="space-y-2">
      {d.outdated && <p className="rounded-lg bg-warning/10 p-2 text-xs text-warning">{t("The website was published again since this request — return it for correction so it can be resubmitted.")}</p>}
      <PublicationSummary sections={d.summary} />
      {d.comments && <p className="text-xs text-muted-foreground">“{d.comments}”</p>}
      <Button variant="outline" size="sm" onClick={() => setPreview(true)}><Eye /> {t("Preview proposed website")}</Button>
      <PreviewDialog open={preview} onOpenChange={setPreview} slug={d.slug} query={d.preview_query} />
    </div>
  );
}
