/** A sale's receipts with every channel (roadmap 65–67): view, PDF, print, image, email, WhatsApp, secure link.
 * Original and updated (adjustment) receipts are kept side by side; nothing is ever re-issued by sharing. */
import { useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Copy, Download, ImageDown, Loader2, Mail, MessageCircle, Printer } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { cn } from "@/lib/utils";
import { download, fileName, printReceipt, receiptLink, receiptPdf, receiptPng, shareMessage, toBase64, waNumber, type ReceiptRecord } from "@/lib/receipt";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";
import { Loading } from "@/components/Page";
import { Receipt } from "./Receipt";
import { t } from "@/lib/i18n";

interface Receipts { items: ReceiptRecord[]; customer: { mobile: string; email: string } | null }

export function useReceipts(saleId: string | undefined) {
  return useQuery({ queryKey: ["receipts", saleId], queryFn: () => api<Receipts>(`/sales/${saleId}/receipts`), enabled: !!saleId });
}

export function ReceiptPanel({ saleId, zoom = 1.6, compact }: { saleId: string; zoom?: number; compact?: boolean }) {
  const { can, profile } = useSession();
  const { data, isLoading } = useReceipts(saleId);
  const [pick, setPick] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [emailOpen, setEmailOpen] = useState(false);
  const [to, setTo] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const items = data?.items ?? [];
  // The newest receipt is shown first after a return / exchange (the original stays one tap away).
  const current = items.find((r) => r.id === pick) ?? items[items.length - 1];
  const snap = current?.snapshot;
  const run = async (key: string, fn: () => Promise<void>) => {
    setBusy(key);
    try {
      await fn();
    } catch (e) {
      if (!(e instanceof DOMException && e.name === "AbortError")) toast.error(e);
    } finally {
      setBusy(null);
    }
  };
  const email = useMutation({
    mutationFn: async () => {
      const pdf = await receiptPdf(snap!);
      return api<{ email_status: { status: string; error: string } }>(`/receipts/${current!.id}/email`, { body: { to, pdf: await toBase64(pdf) } });
    },
    onSuccess: (r) => {
      if (r.email_status.status === "sent") {
        toast.success("Receipt emailed");
        setEmailOpen(false);
      } else toast.error(r.email_status.error || "The email could not be sent");
    },
    onError: (e) => toast.error(e),
  });

  if (isLoading) return <Loading />;
  if (!current || !snap) return null;
  const mobile = data?.customer?.mobile;

  const whatsapp = () => run("wa", async () => {
    const link = await receiptLink(current.id);
    const msg = shareMessage(snap, link);
    // A configured WhatsApp Business number sends it directly (confirmed by WhatsApp).
    if (profile?.integrations.whatsapp && mobile) {
      const r = await api<{ sent: boolean }>(`/sales/${saleId}/share`, { method: "POST" });
      if (r.sent) {
        toast.success("Receipt link sent on WhatsApp");
        return;
      }
    }
    const pdf = await receiptPdf(snap);
    const file = new File([pdf], fileName(snap, "pdf"), { type: "application/pdf" });
    // Phones: share the actual PDF (choose WhatsApp in the share sheet). Otherwise WhatsApp opens with the message.
    if (navigator.canShare?.({ files: [file] })) {
      await navigator.share({ files: [file], text: msg, title: `${snap.business.name} ${snap.number}` });
      return;
    }
    window.open(`https://wa.me/${waNumber(mobile)}?text=${encodeURIComponent(msg)}`, "_blank", "noopener");
    toast.info("WhatsApp is open with the message — attach the downloaded PDF if you want to send the file too.");
  });

  return (
    <div className="space-y-3">
      {items.length > 1 && (
        <div className="scrollbar-none flex gap-1.5 overflow-x-auto">
          {items.map((r) => (
            <button key={r.id} type="button" onClick={() => setPick(r.id)}
              className={cn("h-8 shrink-0 rounded-full border px-3 text-xs", r.id === current.id ? "border-primary bg-primary text-primary-foreground" : "bg-card")}>
              {r.kind === "original" ? t("Original") : t("Updated")} · <span className="num">{r.number}</span>
            </button>
          ))}
        </div>
      )}
      <div className="overflow-x-auto rounded-xl bg-muted/50 p-3">
        <Receipt ref={ref} snapshot={snap} zoom={zoom} />
      </div>
      {can("sales.print") && (
        <div className={cn("grid gap-2", compact ? "grid-cols-3" : "grid-cols-3 sm:grid-cols-6")}>
          <Button variant="outline" size="sm" disabled={!!busy} onClick={() => run("pdf", async () => download(await receiptPdf(snap), fileName(snap, "pdf")))}>
            {busy === "pdf" ? <Loader2 className="animate-spin" /> : <Download />} PDF
          </Button>
          <Button variant="outline" size="sm" disabled={!!busy} onClick={() => run("print", () => printReceipt(snap))}><Printer /> {t("Print")}</Button>
          <Button variant="outline" size="sm" disabled={!!busy} onClick={() => run("img", async () => { if (ref.current) download(await receiptPng(ref.current), fileName(snap, "png")); })}>
            {busy === "img" ? <Loader2 className="animate-spin" /> : <ImageDown />} {t("Image")}
          </Button>
          <Button variant="outline" size="sm" disabled={!!busy} onClick={() => { setTo(data?.customer?.email ?? ""); setEmailOpen(true); }}><Mail /> {t("Email")}</Button>
          <Button variant="success" size="sm" disabled={!!busy} onClick={whatsapp}>{busy === "wa" ? <Loader2 className="animate-spin" /> : <MessageCircle />} WhatsApp</Button>
          <Button variant="outline" size="sm" disabled={!!busy} onClick={() => run("link", async () => { await navigator.clipboard.writeText(await receiptLink(current.id)); toast.success("Secure receipt link copied"); })}>
            <Copy /> {t("Link")}
          </Button>
        </div>
      )}
      <ResponsiveDialog open={emailOpen} onOpenChange={setEmailOpen} title="Email receipt" description={`${snap.number} — ${t("the PDF is attached")}`}
        footer={<ActionButton online busy={email.isPending} busyLabel="Sending…" blockedBy={[!/^\S+@\S+\.\S+$/.test(to.trim()) && "Enter an email address"]} onAction={() => email.mutateAsync()}>Send receipt</ActionButton>}>
        <Field label="Customer email"><Input type="email" value={to} onChange={(e) => setTo(e.target.value)} placeholder="customer@email.com" autoFocus /></Field>
      </ResponsiveDialog>
    </div>
  );
}
