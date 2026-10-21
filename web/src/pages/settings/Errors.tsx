/** Roadmap 147: Settings → Platform → Errors & alerts — failures that need the platform owner's attention (server
 * faults, outside services refusing, failed background jobs, restarts, the database not answering), grouped, with
 * Resolve and a test alert. Business answers (wrong PIN, validation, permissions) are never listed here. */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { BellRing, CheckCircle2, Mail } from "lucide-react";
import { api } from "@/lib/api";
import { ago, count, dateTime } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { ActionButton } from "@/components/ActionButton";
import { Segments } from "@/components/Filters";
import { Pill, type Tone } from "@/components/Badges";
import { EmptyState, Loading } from "@/components/Page";
import { SettingsPage } from "./shared";

interface ErrorEvent {
  id: string;
  source: "server" | "job" | "health" | "test";
  kind: string;
  message: string;
  method: string;
  path: string;
  count: number;
  first_seen: string;
  last_seen: string;
  notified_at: string | null;
  resolved_at: string | null;
  resolved_by: string | null;
}
interface ErrorList { items: ErrorEvent[]; open: number; email: boolean; recipients: string[]; last_start: string | null }
interface TestResult { id: string; email: { status: string; error: string } | null }

const SOURCE: Record<ErrorEvent["source"], [Tone, string]> = {
  server: ["danger", "Server"],
  job: ["warning", "Background job"],
  health: ["danger", "Health"],
  test: ["info", "Test"],
};

export function ErrorsSettings() {
  const qc = useQueryClient();
  const [status, setStatus] = useState<"open" | "resolved" | "all">("open");
  const q = useQuery({ queryKey: ["platform-errors", status], queryFn: () => api<ErrorList>(`/platform/errors?status=${status}`) });
  const refresh = () => qc.invalidateQueries({ queryKey: ["platform-errors"] });
  const resolve = useMutation({
    mutationFn: (id: string) => api(`/platform/errors/${id}/resolve`, { method: "POST" }),
    onSuccess: () => { toast.success("Marked resolved"); refresh(); },
    onError: (e) => toast.error(e),
  });
  const test = useMutation({
    mutationFn: () => api<TestResult>("/platform/errors/test", { method: "POST" }),
    onSuccess: (r) => {
      refresh();
      qc.invalidateQueries({ queryKey: ["notifications"] });
      if (r.email?.status === "sent") toast.success("Test alert sent — check the bell and your email");
      else toast.warning(`${t("Test alert in the app; email not sent")}: ${r.email?.error || t("no platform admin email")}`);
    },
    onError: (e) => toast.error(e),
  });
  const d = q.data;
  return (
    <SettingsPage title="Errors & alerts" loading={q.isLoading}
      description="Failures that need your attention reach you here, in the bell and by email. Wrong PINs, validation and permission answers are normal and never listed.">
      <section className="surface card-body space-y-3 text-sm">
        <div className="flex flex-wrap items-center gap-2">
          <BellRing className="h-4 w-4 text-primary" />
          <span className="min-w-0 flex-1">
            {t("Alerts go to the platform administrators in the app")}
            {d?.email && d.recipients.length ? <> {t("and by email to")} <b className="num">{d.recipients.join(", ")}</b></> : <> — {t("email is not configured on the server")}</>}.
          </span>
          <ActionButton size="sm" variant="outline" online busy={test.isPending} busyLabel="Sending…" onAction={() => test.mutateAsync()}>
            <Mail /> {t("Send test alert")}
          </ActionButton>
        </div>
        <p className="text-xs text-muted-foreground">
          {t("The first time an error happens you are alerted; repeats are counted and alert again after 6 hours, or if it comes back after you marked it resolved.")}
          {d?.last_start && <> {t("Service last started")} {ago(d.last_start)}.</>}
        </p>
      </section>
      <Segments value={status} onChange={setStatus} options={[
        { value: "open", label: "Open", count: d?.open },
        { value: "resolved", label: "Resolved" },
        { value: "all", label: "All" },
      ]} />
      {!d ? <Loading /> : !d.items.length ? (
        <EmptyState icon={CheckCircle2} title={status === "open" ? "No open errors" : "Nothing here"} hint={status === "open" ? "Everything that needed attention has been handled." : undefined} />
      ) : (
        <ul className="space-y-2">
          {d.items.map((e) => (
            <li key={e.id} className="surface card-body space-y-1.5 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <Pill tone={SOURCE[e.source][0]}>{t(SOURCE[e.source][1])}</Pill>
                <b className="min-w-0 flex-1">{t(e.kind)}</b>
                {e.count > 1 && <Pill tone="neutral"><span className="num">{count(e.count)}×</span></Pill>}
                {e.resolved_at ? (
                  <Pill tone="success">{t("Resolved")}</Pill>
                ) : (
                  <ActionButton size="sm" variant="outline" online busy={resolve.isPending && resolve.variables === e.id} busyLabel="Saving…" onAction={() => resolve.mutateAsync(e.id)}>
                    <CheckCircle2 /> {t("Resolve")}
                  </ActionButton>
                )}
              </div>
              <p className="break-words">{e.message}</p>
              {e.path && <p className="num break-all text-xs text-muted-foreground">{e.method} {e.path}</p>}
              <p className="text-xs text-muted-foreground">
                {e.count > 1 ? <>{t("First")} {dateTime(e.first_seen)} · {t("last")} {ago(e.last_seen)}</> : dateTime(e.first_seen)}
                {e.resolved_at && <> · {t("resolved")} {ago(e.resolved_at)}{e.resolved_by ? ` ${t("by")} ${e.resolved_by}` : ""}</>}
              </p>
            </li>
          ))}
        </ul>
      )}
    </SettingsPage>
  );
}
