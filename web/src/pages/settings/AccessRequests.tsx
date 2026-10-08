import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Building2, Check, Copy, KeyRound, Mail, MapPin, MessageCircle, Phone, ReceiptText, Send, Users, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { ago, date, dateTime } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Textarea } from "@/components/ui/textarea";
import { ConfirmDialog, Field } from "@/components/Form";
import { Segments } from "@/components/Filters";
import { Pill, StatusBadge } from "@/components/Badges";
import { EmptyState, Loading } from "@/components/Page";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { SettingsPage } from "./shared";
import { EmailHistory, EmailStatus, type EmailRow } from "./EmailHistory";
import { t } from "@/lib/i18n";

interface AccessRequest {
  id: string;
  business_name: string;
  contact_name: string;
  email: string;
  phone: string;
  location: string;
  business_type: string;
  branches: number | null;
  estimated_users: number | null;
  message: string;
  status: "pending" | "approved" | "rejected";
  tenant_id: string | null;
  decided_by_name: string | null;
  decided_at: string | null;
  decision_note: string;
  public_reason: string;
  created_at: string;
  emails: Record<string, { status: string; at: string }>;
  activation: "active" | "awaiting_setup" | "pin_expired" | "deactivated" | null;
}

interface EmailOutcome { id: string; status: string; error: string }

interface Approved {
  business: string;
  name: string;
  email: string;
  phone: string;
  wa_phone: string;
  temporary_pin: string;
  pin_expires_hours: number;
  sign_in_url: string;
  message: string;
  email_status: EmailOutcome;
  request_id: string;
}

interface Details {
  request: AccessRequest;
  sign_in_url: string;
  business_status: string | null;
  slug: string | null;
  activation: { status: string; name: string; last_login_at: string | null; pin_expires_at: string | null; pin_changed_at: string | null } | null;
  wa_phone: string;
  message: string;
  emails: EmailRow[];
  email_configured: boolean;
}

const ACTIVATION: Record<string, [string, "success" | "warning" | "danger" | "neutral"]> = {
  active: ["Active", "success"],
  awaiting_setup: ["Awaiting first sign-in", "warning"],
  pin_expired: ["One-time PIN expired", "danger"],
  deactivated: ["Deactivated", "danger"],
};

/** WhatsApp Web on desktop, the app on phones: the recipient is chosen and the message composed; sending stays manual. */
const waLink = (phone: string, text: string) => `https://wa.me/${phone}?text=${encodeURIComponent(text)}`;

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.success("Copied");
  } catch {
    toast.error("Copy failed — select the text and copy it manually");
  }
}

/** Platform admins review requests from businesses that want S'Shop; approving creates the business. */
export function AccessRequests() {
  const qc = useQueryClient();
  const [status, setStatus] = useState<"pending" | "approved" | "rejected" | "all">("pending");
  const [rejecting, setRejecting] = useState<AccessRequest | null>(null);
  const [note, setNote] = useState("");
  const [reason, setReason] = useState("");
  const [approved, setApproved] = useState<Approved | null>(null);
  const [detailsId, setDetailsId] = useState<string | null>(null);
  const [pinFor, setPinFor] = useState<AccessRequest | null>(null);
  const [newPin, setNewPin] = useState<{ business: string; name: string; email: string; temporary_pin: string; expires_at: string; sign_in_url: string; wa_phone: string } | null>(null);
  const { data, isLoading } = useQuery({
    queryKey: ["access-requests", status],
    queryFn: () => api<{ items: AccessRequest[]; pending: number; email_configured: boolean }>("/platform/access-requests", { query: { status } }),
  });
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["access-requests"] });
    qc.invalidateQueries({ queryKey: ["access-request"] });
  };

  const approve = useMutation({
    mutationFn: (r: AccessRequest) => api<Omit<Approved, "request_id">>(`/platform/access-requests/${r.id}/approve`, { method: "POST" }).then((res) => ({ ...res, request_id: r.id })),
    onSuccess: (res) => { setApproved(res); refresh(); },
    onError: (e) => toast.error(e),
  });
  const reject = useMutation({
    mutationFn: () => api<{ email_status: EmailOutcome }>(`/platform/access-requests/${rejecting!.id}/reject`, { body: { note, reason } }),
    onSuccess: (r) => {
      toast.success(r.email_status.status === "sent" ? "Request rejected — the applicant has been emailed" : "Request rejected — the email could not be sent (retry from the request's history)");
      setRejecting(null); setNote(""); setReason(""); refresh();
    },
    onError: (e) => toast.error(e),
  });
  const resend = useMutation({
    mutationFn: (id: string) => api<{ email_status: EmailOutcome }>(`/platform/access-requests/${id}/resend-welcome`, { method: "POST" }),
    onSuccess: (r) => {
      if (r.email_status.status === "sent") toast.success("Welcome email sent with a fresh set-up link");
      else toast.error(r.email_status.error || "Email not sent");
      if (approved) setApproved({ ...approved, email_status: r.email_status });
      refresh();
    },
    onError: (e) => toast.error(e),
  });
  const issuePin = useMutation({
    mutationFn: (r: AccessRequest) => api<{ email: string; name: string; temporary_pin: string; expires_at: string; sign_in_url: string; wa_phone: string }>(`/platform/access-requests/${r.id}/issue-pin`, { method: "POST" }).then((x) => ({ ...x, business: r.business_name })),
    onSuccess: (res) => { setPinFor(null); setNewPin(res); refresh(); },
    onError: (e) => toast.error(e),
  });

  return (
    <SettingsPage title="Access requests" description="Businesses asking to use S'Shop. Nothing is activated until you approve: approving creates the business and its administrator.">
      {data && !data.email_configured && (
        <p className="rounded-lg bg-warning/10 p-3 text-xs text-warning">{t("Emails are off — set RESEND_API_KEY on the server. Requests are still saved here; share sign-in details by WhatsApp or copy.")}</p>
      )}
      <Segments value={status} onChange={setStatus} options={[
        { value: "pending", label: "Pending", count: data?.pending },
        { value: "approved", label: "Approved" },
        { value: "rejected", label: "Rejected" },
        { value: "all", label: "All" },
      ]} />
      {isLoading ? <Loading /> : !data?.items.length ? (
        <div className="surface"><EmptyState icon={Building2} title={status === "pending" ? "No pending requests" : "Nothing here"} /></div>
      ) : (
        <div className="grid gap-3 xl:grid-cols-2">
          {data.items.map((r) => {
            const welcome = r.emails.welcome;
            return (
              <div key={r.id} className="surface card-body space-y-2.5">
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <p className="truncate font-semibold">{r.business_name}</p>
                    <p className="text-xs text-muted-foreground">
                      {[r.business_type, r.branches ? `${r.branches} ${t(r.branches > 1 ? "branches" : "branch")}` : null, r.estimated_users ? `${r.estimated_users} ${t("users")}` : null].filter(Boolean).join(" · ") || "—"} · {ago(r.created_at)}
                    </p>
                  </div>
                  <StatusBadge status={r.status} />
                </div>
                <div className="grid gap-1 text-sm">
                  <span className="font-medium">{r.contact_name}</span>
                  <a href={`mailto:${r.email}`} className="flex items-center gap-2 text-muted-foreground"><Mail className="h-3.5 w-3.5" />{r.email}</a>
                  <a href={`tel:${r.phone}`} className="num flex items-center gap-2 text-muted-foreground"><Phone className="h-3.5 w-3.5" />{r.phone}</a>
                  {r.location && <span className="flex items-center gap-2 text-muted-foreground"><MapPin className="h-3.5 w-3.5" />{r.location}</span>}
                </div>
                {r.message && <p className="rounded-lg bg-muted/60 p-2.5 text-sm">{r.message}</p>}
                {r.status !== "pending" && (
                  <p className="text-xs text-muted-foreground">
                    {t(r.status === "approved" ? "Approved" : "Rejected")} {t("by")} {r.decided_by_name ?? "—"} · {dateTime(r.decided_at)}
                    {r.public_reason && ` — ${t("told the applicant")}: “${r.public_reason}”`}
                    {r.decision_note && ` — ${t("internal")}: “${r.decision_note}”`}
                  </p>
                )}
                {r.status === "approved" && (
                  <div className="flex flex-wrap items-center gap-1.5 text-xs">
                    {r.activation && <Pill tone={ACTIVATION[r.activation]?.[1] ?? "neutral"}>{t(ACTIVATION[r.activation]?.[0] ?? r.activation)}</Pill>}
                    {welcome && <span className="inline-flex items-center gap-1 text-muted-foreground">{t("Welcome email")} <EmailStatus status={welcome.status} /></span>}
                  </div>
                )}
                {r.status === "pending" && (
                  <div className="flex gap-2 pt-1">
                    <Button size="sm" variant="outline" className="text-destructive" onClick={() => setRejecting(r)}><X /> {t("Reject")}</Button>
                    <ActionButton size="sm" variant="success" className="ms-auto" online busy={approve.isPending && approve.variables?.id === r.id} disabled={approve.isPending}
                      busyLabel="Creating…" onAction={() => approve.mutateAsync(r)}><Check /> {t("Approve & create")}</ActionButton>
                  </div>
                )}
                {r.status === "approved" && (
                  <div className="grid grid-cols-2 gap-1.5 pt-1 sm:flex sm:flex-wrap">
                    <Button size="sm" variant="outline" onClick={() => setDetailsId(r.id)}><ReceiptText /> {t("View Login Details")}</Button>
                    <ActionButton size="sm" variant="outline" online busy={resend.isPending && resend.variables === r.id} disabled={resend.isPending} busyLabel="Sending…"
                      onAction={() => resend.mutateAsync(r.id)}><Send /> {t("Resend Welcome Email")}</ActionButton>
                    <Button size="sm" variant="outline" onClick={() => setPinFor(r)}><KeyRound /> {t("Issue New One-Time PIN")}</Button>
                    <Button size="sm" variant="success" asChild>
                      <a href={waLink(r.phone.replace(/\D/g, "").replace(/^0/, "254"), welcomeText(r))} target="_blank" rel="noreferrer"><MessageCircle /> WhatsApp</a>
                    </Button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      <ResponsiveDialog
        open={!!rejecting}
        onOpenChange={(o) => !o && setRejecting(null)}
        title="Reject request"
        description={rejecting?.business_name}
        footer={<ActionButton variant="destructive" className="w-full md:w-auto" online busy={reject.isPending} busyLabel="Rejecting…" onAction={() => reject.mutateAsync()}>{t("Reject")}</ActionButton>}
      >
        <div className="space-y-3">
          <Field label="Reason for the applicant" optional hint="Included in the courteous email the applicant receives."><Textarea value={reason} onChange={(e) => setReason(e.target.value)} maxLength={500} /></Field>
          <Field label="Internal note" optional hint="Only visible to platform administrators — never sent."><Textarea value={note} onChange={(e) => setNote(e.target.value)} maxLength={500} /></Field>
        </div>
      </ResponsiveDialog>

      <ResponsiveDialog open={!!approved} onOpenChange={(o) => !o && setApproved(null)} title="Business activated" description={approved?.business}>
        {approved && (
          <div className="space-y-3 text-sm">
            <p>{t("A welcome email with a secure set-up link has been sent to the new administrator.")} <b>{t("The one-time PIN below is shown only once")}</b> {t("— share it only if needed, never in a group chat.")}</p>
            <div className="space-y-1 rounded-xl bg-muted/60 p-3">
              <Row label="Business">{approved.business}</Row>
              <Row label="Administrator email">{approved.email}</Row>
              <Row label="Login URL">{approved.sign_in_url}</Row>
              <Row label="One-time PIN">
                <span className="inline-flex items-center gap-1.5">
                  <span className="num font-semibold tracking-wider">{approved.temporary_pin}</span>
                  <button type="button" aria-label={t("Copy PIN")} onClick={() => copy(approved.temporary_pin)} className="text-muted-foreground hover:text-foreground"><Copy className="h-3.5 w-3.5" /></button>
                </span>
              </Row>
              <Row label="PIN expires">{t("in")} {approved.pin_expires_hours} {t("hours · must be changed at first sign-in")}</Row>
              <Row label="Email status"><EmailStatus status={approved.email_status.status} error={approved.email_status.error} /></Row>
            </div>
            {approved.email_status.status !== "sent" && approved.email_status.error && <p className="rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">{approved.email_status.error}</p>}
            <div className="grid grid-cols-3 gap-2">
              <ActionButton variant="outline" online busy={resend.isPending} busyLabel="Sending…" onAction={() => resend.mutateAsync(approved.request_id)}><Send /> {t("Resend Email")}</ActionButton>
              <Button variant="outline" onClick={() => copy(approved.message)}><Copy /> {t("Copy Message")}</Button>
              <Button variant="success" asChild><a href={waLink(approved.wa_phone, approved.message)} target="_blank" rel="noreferrer"><MessageCircle /> WhatsApp</a></Button>
            </div>
            <p className="text-xs text-muted-foreground">{t("WhatsApp opens with the administrator and the message ready — you press Send. The message does not include the PIN.")}</p>
          </div>
        )}
      </ResponsiveDialog>

      <ConfirmDialog open={!!pinFor} onOpenChange={(o) => !o && setPinFor(null)} title="Issue a new one-time PIN?"
        description={`${pinFor?.contact_name} (${pinFor?.email}) — ${t("their current PIN and any open sessions stop working. The new PIN is shown once, expires in 72 hours and must be replaced at first sign-in.")}`}
        confirmLabel="Issue PIN" busy={issuePin.isPending} onConfirm={() => pinFor && issuePin.mutate(pinFor)} />

      <ResponsiveDialog open={!!newPin} onOpenChange={(o) => !o && setNewPin(null)} title="New one-time PIN" description={newPin?.business}>
        {newPin && (
          <div className="space-y-3 text-sm">
            <p><b>{t("Shown only once.")}</b> {t("Read it to the administrator on a call — don't post it in a chat. It is not stored and cannot be shown again.")}</p>
            <div className="space-y-1 rounded-xl bg-muted/60 p-3">
              <Row label="Administrator">{newPin.name}</Row>
              <Row label="Email">{newPin.email}</Row>
              <Row label="One-time PIN"><span className="num text-base font-bold tracking-wider">{newPin.temporary_pin}</span></Row>
              <Row label="Expires">{dateTime(newPin.expires_at)}</Row>
              <Row label="Login URL">{newPin.sign_in_url}</Row>
            </div>
            <div className="grid grid-cols-2 gap-2">
              <Button variant="outline" onClick={() => copy(newPin.temporary_pin)}><Copy /> {t("Copy PIN")}</Button>
              <Button variant="success" asChild><a href={`tel:+${newPin.wa_phone}`}><Phone /> {t("Call administrator")}</a></Button>
            </div>
          </div>
        )}
      </ResponsiveDialog>

      {detailsId && <LoginDetails id={detailsId} onClose={() => setDetailsId(null)} onChanged={refresh} />}
    </SettingsPage>
  );
}

function welcomeText(r: AccessRequest) {
  const first = r.contact_name.split(" ")[0];
  return `*Welcome to S'Shop!*\n\nHello ${first},\nYour business, *${r.business_name}*, has been successfully activated on S'Shop.\n\nYou can now access your account using:\n*Login:* ${location.origin}/login\n*Email:* ${r.email}\n\nPlease use the secure account setup instructions sent to your email to complete your first login.\n\nWelcome aboard!\n*S'Shop Team | SyncScore*`;
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return <div className="flex items-start justify-between gap-3"><span className="shrink-0 text-muted-foreground">{t(label)}</span><span className="min-w-0 break-all text-end">{children}</span></div>;
}

/** Receipt-style login details for an approved request (like an ETR receipt) — never the PIN, which is not stored. */
function LoginDetails({ id, onClose, onChanged }: { id: string; onClose: () => void; onChanged: () => void }) {
  const { data, refetch } = useQuery({ queryKey: ["access-request", id], queryFn: () => api<Details>(`/platform/access-requests/${id}`) });
  const r = data?.request;
  const act = data?.activation;
  const welcome = data?.emails.find((e) => e.kind === "welcome");
  const lines: [string, React.ReactNode][] = r ? [
    ["Business", r.business_name],
    ["Administrator", r.contact_name],
    ["Email", r.email],
    ["Mobile", r.phone],
    ["Login URL", data!.sign_in_url],
    ["Approved", date(r.decided_at)],
    ["Activation", act ? t(ACTIVATION[act.status]?.[0] ?? act.status) : "—"],
    ["Last sign-in", act?.last_login_at ? dateTime(act.last_login_at) : t("Never")],
    ["Email delivery", welcome ? <EmailStatus status={welcome.status} error={welcome.error} /> : "—"],
  ] : [];
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Login details" description={r?.business_name}>
      {!data || !r ? <Loading /> : (
        <div className="space-y-4">
          <div className="mx-auto max-w-sm rounded-md border border-dashed bg-card px-4 py-3 font-mono text-[13px] shadow-sm">
            <p className="text-center text-xs font-semibold uppercase tracking-[0.2em]">S'Shop</p>
            <p className="text-center text-[11px] text-muted-foreground">{t("Account access slip")}</p>
            <div className="my-2 border-t border-dashed" />
            {lines.map(([k, v]) => (
              <div key={k} className="flex justify-between gap-3 py-0.5"><span className="shrink-0 text-muted-foreground">{t(k)}</span><span className="min-w-0 break-all text-end">{v}</span></div>
            ))}
            <div className="my-2 border-t border-dashed" />
            <p className="text-center text-[11px] text-muted-foreground">{t("PINs are never stored or shown again. Issue a new one-time PIN if needed.")}</p>
          </div>
          <div>
            <p className="mb-1 flex items-center gap-1.5 text-sm font-semibold"><Users className="h-4 w-4" /> {t("Notification history")}</p>
            <EmailHistory items={data.emails} onChanged={() => { refetch(); onChanged(); }} />
          </div>
          <div className="grid grid-cols-2 gap-2">
            <Button variant="outline" onClick={() => copy(`${r.business_name}\n${t("Administrator")}: ${r.contact_name}\n${t("Email")}: ${r.email}\n${t("Login URL")}: ${data.sign_in_url}`)}><Copy /> {t("Copy details")}</Button>
            <Button variant="success" asChild><a href={waLink(data.wa_phone, data.message)} target="_blank" rel="noreferrer"><MessageCircle /> WhatsApp</a></Button>
          </div>
        </div>
      )}
    </ResponsiveDialog>
  );
}
