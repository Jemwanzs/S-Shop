/** Notification history for onboarding emails (roadmap 58): type, recipient, time, delivery status, retry. */
import { useMutation } from "@tanstack/react-query";
import { RotateCw } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { dateTime } from "@/lib/format";
import { Pill, type Tone } from "@/components/Badges";
import { ActionButton } from "@/components/ActionButton";
import { t } from "@/lib/i18n";

export interface EmailRow {
  id: string;
  kind: string;
  recipient: string;
  subject: string;
  status: string;
  error: string;
  attempts: number;
  created_at: string;
  updated_at: string;
  created_by_name: string | null;
  retryable: boolean;
}

export const EMAIL_KIND: Record<string, string> = {
  access_request_received: "Access Request Received",
  access_request_ack: "Applicant Acknowledgement",
  welcome: "Welcome Email",
  access_rejected: "Account Rejected",
  pin_reset: "PIN Reset Link",
  pin_changed: "PIN Changed",
  request_status: "Request Status Link",
  login_details: "Login Details",
};

const STATUS: Record<string, [string, Tone]> = {
  queued: ["Pending", "neutral"],
  sent: ["Sent", "info"],
  delivered: ["Delivered", "success"],
  delayed: ["Delayed", "warning"],
  bounced: ["Bounced", "danger"],
  complained: ["Marked as spam", "danger"],
  failed: ["Failed", "danger"],
  skipped: ["Not sent", "warning"],
};

export function EmailStatus({ status, error }: { status: string; error?: string }) {
  const [label, tone] = STATUS[status] ?? [status, "neutral" as Tone];
  return <span title={error || undefined}><Pill tone={tone}>{t(label)}</Pill></span>;
}

export function EmailHistory({ items, onChanged }: { items: EmailRow[]; onChanged: () => void }) {
  const retry = useMutation({
    mutationFn: (id: string) => api<{ email_status: { status: string; error: string } }>(`/platform/emails/${id}/retry`, { method: "POST" }),
    onSuccess: (r) => {
      if (r.email_status.status === "sent") toast.success("Email sent");
      else toast.error(r.email_status.error || "Email not sent");
      onChanged();
    },
    onError: (e) => toast.error(e),
  });
  if (!items.length) return <p className="py-2 text-sm text-muted-foreground">{t("No emails yet.")}</p>;
  return (
    <ul className="divide-y">
      {items.map((e) => (
        <li key={e.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2 text-sm">
          <div className="min-w-0 flex-1">
            <p className="truncate font-medium">{t(EMAIL_KIND[e.kind] ?? e.kind)}</p>
            <p className="truncate text-xs text-muted-foreground">{e.recipient} · {dateTime(e.created_at)}{e.created_by_name && ` · ${e.created_by_name}`}</p>
            {e.error && <p className="text-xs text-destructive">{e.error}</p>}
          </div>
          <EmailStatus status={e.status} error={e.error} />
          {e.retryable && (
            <ActionButton size="sm" variant="ghost" online quietReason busy={retry.isPending && retry.variables === e.id} disabled={retry.isPending}
              onAction={() => retry.mutateAsync(e.id)} aria-label={t("Retry")}><RotateCw /> {t("Retry")}</ActionButton>
          )}
        </li>
      ))}
    </ul>
  );
}
