import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Building2, Check, Copy, Mail, MapPin, MessageCircle, Phone, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, errorMessage } from "@/lib/api";
import { ago, dateTime } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Field } from "@/components/Form";
import { Segments } from "@/components/Filters";
import { StatusBadge } from "@/components/Badges";
import { EmptyState, Loading } from "@/components/Page";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { SettingsPage } from "./shared";

interface AccessRequest {
  id: string;
  business_name: string;
  contact_name: string;
  email: string;
  phone: string;
  location: string;
  business_type: string;
  branches: number | null;
  message: string;
  status: "pending" | "approved" | "rejected";
  decided_by_name: string | null;
  decided_at: string | null;
  decision_note: string;
  email_sent: boolean;
  created_at: string;
}

interface Approved {
  business: string;
  email: string;
  phone: string;
  temporary_pin: string;
  sign_in_url: string;
  message: string;
}

/** Platform admins review requests from businesses that want S'Shop; approving creates the business. */
export function AccessRequests() {
  const qc = useQueryClient();
  const [status, setStatus] = useState<"pending" | "approved" | "rejected" | "all">("pending");
  const [rejecting, setRejecting] = useState<AccessRequest | null>(null);
  const [note, setNote] = useState("");
  const [approved, setApproved] = useState<Approved | null>(null);
  const { data, isLoading } = useQuery({
    queryKey: ["access-requests", status],
    queryFn: () => api<{ items: AccessRequest[]; pending: number; email_configured: boolean }>("/platform/access-requests", { query: { status } }),
  });
  const refresh = () => qc.invalidateQueries({ queryKey: ["access-requests"] });

  const approve = useMutation({
    mutationFn: (r: AccessRequest) => api<Omit<Approved, "business">>(`/platform/access-requests/${r.id}/approve`, { method: "POST" }).then((res) => ({ ...res, business: r.business_name })),
    onSuccess: (res) => { setApproved(res); refresh(); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const reject = useMutation({
    mutationFn: () => api(`/platform/access-requests/${rejecting!.id}/reject`, { body: { note } }),
    onSuccess: () => { toast.success("Request rejected"); setRejecting(null); setNote(""); refresh(); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success("Copied");
    } catch {
      toast.error("Copy failed — select the text and copy it manually");
    }
  };
  const waLink = (a: Approved) => `https://wa.me/${a.phone.replace(/\D/g, "").replace(/^0/, "254")}?text=${encodeURIComponent(a.message)}`;

  return (
    <SettingsPage title="Access requests" description="Businesses asking to use S'Shop. Nothing is activated until you approve: approving creates the business and its administrator.">
      {data && !data.email_configured && (
        <p className="rounded-lg bg-warning/10 p-3 text-xs text-warning">Email alerts are off — set RESEND_API_KEY on the server to be emailed about new requests. Requests are still saved here.</p>
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
          {data.items.map((r) => (
            <div key={r.id} className="surface space-y-2.5 p-3.5">
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="truncate font-semibold">{r.business_name}</p>
                  <p className="text-xs text-muted-foreground">{[r.business_type, r.branches ? `${r.branches} branch${r.branches > 1 ? "es" : ""}` : null].filter(Boolean).join(" · ") || "—"} · {ago(r.created_at)}</p>
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
                <p className="text-xs text-muted-foreground">{r.status} by {r.decided_by_name ?? "—"} · {dateTime(r.decided_at)}{r.decision_note && ` — “${r.decision_note}”`}</p>
              )}
              {r.status === "pending" && (
                <div className="flex gap-2 pt-1">
                  <Button size="sm" variant="outline" className="text-destructive" onClick={() => setRejecting(r)}><X /> Reject</Button>
                  <Button size="sm" variant="success" className="ms-auto" disabled={approve.isPending} onClick={() => approve.mutate(r)}><Check /> Approve & create</Button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      <ResponsiveDialog
        open={!!rejecting}
        onOpenChange={(o) => !o && setRejecting(null)}
        title="Reject request"
        description={rejecting?.business_name}
        footer={<Button variant="destructive" className="w-full md:w-auto" disabled={reject.isPending} onClick={() => reject.mutate()}>Reject</Button>}
      >
        <Field label="Reason" optional><Textarea value={note} onChange={(e) => setNote(e.target.value)} /></Field>
      </ResponsiveDialog>

      <ResponsiveDialog open={!!approved} onOpenChange={(o) => !o && setApproved(null)} title="Business activated" description={approved?.business}>
        {approved && (
          <div className="space-y-3 text-sm">
            <p>Send these sign-in details to the new administrator. <b>The temporary PIN is shown only once.</b></p>
            <div className="space-y-1 rounded-xl bg-muted/60 p-3">
              <div className="flex justify-between gap-3"><span className="text-muted-foreground">Email</span><span className="truncate">{approved.email}</span></div>
              <div className="flex justify-between gap-3"><span className="text-muted-foreground">Temporary PIN</span><span className="num font-semibold tracking-wider">{approved.temporary_pin}</span></div>
              <div className="flex justify-between gap-3"><span className="text-muted-foreground">Sign in</span><span className="truncate">{approved.sign_in_url}</span></div>
            </div>
            <div className="grid grid-cols-2 gap-2">
              <Button variant="outline" onClick={() => copy(approved.message)}><Copy /> Copy message</Button>
              <Button variant="success" asChild><a href={waLink(approved)} target="_blank" rel="noreferrer"><MessageCircle /> WhatsApp</a></Button>
            </div>
          </div>
        )}
      </ResponsiveDialog>
    </SettingsPage>
  );
}
