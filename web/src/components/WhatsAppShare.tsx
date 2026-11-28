/** Roadmap 201: sharing a receipt (or a repayment confirmation) on WhatsApp, straight into the customer's chat.
 * The customer's own number comes from the sale — never the branch's, the salesperson's or the business's — and is put
 * in international form; without one the person types it here. The message is reviewed (and can be edited) first.
 * Phones open the WhatsApp app (wa.me falls back to the web when it is not installed); computers open WhatsApp Web in
 * one reused tab (an already signed-in session goes straight to the chat) or the WhatsApp Desktop app, remembered. */
import { useEffect, useState } from "react";
import { MessageCircle, Monitor, Share2, Send } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field } from "@/components/Form";
import { useSession } from "@/lib/session";
import { toast } from "@/lib/toast";
import { callingCode, validWaNumber, waNumber } from "@/lib/receipt";
import { t } from "@/lib/i18n";

const PREF = "sshop.whatsapp.desktop";
const readPref = (): "web" | "app" => {
  try {
    return localStorage.getItem(PREF) === "app" ? "app" : "web";
  } catch {
    return "web";
  }
};
const writePref = (v: "web" | "app") => {
  try {
    localStorage.setItem(PREF, v);
  } catch {
    /* private mode — not remembered */
  }
};

/** Phones and tablets (iPadOS reports itself as a Mac with touch). */
export const isMobileDevice = () =>
  /Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent) || (navigator.maxTouchPoints > 1 && /Macintosh/.test(navigator.userAgent));

export const whatsappUrl = (kind: "mobile" | "web" | "app", number: string, text: string) => {
  const m = encodeURIComponent(text);
  if (kind === "web") return `https://web.whatsapp.com/send?phone=${number}&text=${m}`;
  if (kind === "app") return `whatsapp://send?phone=${number}&text=${m}`;
  return `https://wa.me/${number}?text=${m}`;
};

export function WhatsAppShare({ open, onOpenChange, mobile, message, title = "Send on WhatsApp", sharePdf, autoSend }: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  /** The customer's number on the record (null when the sale has no customer or the customer has no number). */
  mobile: string | null | undefined;
  message: string;
  title?: string;
  /** Optional: share the PDF through the device's share sheet (the earlier behaviour, kept as an option). */
  sharePdf?: () => Promise<void>;
  /** Optional: the business's WhatsApp Business API sends it to the customer's number on the record. */
  autoSend?: () => Promise<boolean>;
}) {
  const { profile } = useSession();
  const cc = callingCode(profile?.tenant.timezone);
  const [number, setNumber] = useState("");
  const [text, setText] = useState(message);
  const [appMissing, setAppMissing] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setNumber(mobile ? `+${waNumber(mobile, cc)}` : "");
      setText(message);
      setAppMissing(false);
    }
  }, [open, mobile, message, cc]);
  const n = waNumber(number, cc);
  const ok = validWaNumber(n);
  const mobileDevice = isMobileDevice();
  const pref = readPref();
  const onRecord = !!mobile && n === waNumber(mobile, cc);

  const go = (kind: "mobile" | "web" | "app") => {
    if (!ok) return;
    if (kind === "app") {
      writePref("app");
      // The desktop app takes the link; if nothing takes it (not installed), the page keeps focus — offer the web.
      let left = false;
      const blur = () => { left = true; };
      window.addEventListener("blur", blur, { once: true });
      window.location.href = whatsappUrl("app", n, text);
      setTimeout(() => {
        window.removeEventListener("blur", blur);
        if (left) onOpenChange(false);
        else setAppMissing(true);
      }, 1800);
      return;
    }
    if (kind === "web") writePref("web");
    // One named tab for WhatsApp Web: a signed-in session is reused instead of opening (and signing in) again.
    window.open(whatsappUrl(kind, n, text), kind === "web" ? "sshop-whatsapp-web" : "_blank", kind === "web" ? undefined : "noopener");
    onOpenChange(false);
  };

  const desktopButtons = [
    <Button key="web" variant={pref === "web" ? "success" : "outline"} disabled={!ok} onClick={() => go("web")}><MessageCircle /> {t("WhatsApp Web")}</Button>,
    <Button key="app" variant={pref === "app" ? "success" : "outline"} disabled={!ok} onClick={() => go("app")}><Monitor /> {t("Desktop app")}</Button>,
  ];
  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} title={title}
      description={mobile ? t("To the customer's number on this sale — check the message, then send it in WhatsApp.") : t("No mobile number on this sale's customer — enter the number to send to.")}
      footer={
        <div className="flex w-full flex-col-reverse gap-2 md:flex-row md:flex-wrap md:justify-end" data-testid="wa-actions">
          {sharePdf && <Button variant="ghost" className="md:mr-auto" disabled={busy} onClick={async () => { setBusy(true); try { await sharePdf(); onOpenChange(false); } catch (e) { if (!(e instanceof DOMException && e.name === "AbortError")) toast.error(e); } finally { setBusy(false); } }}><Share2 /> {t("Share PDF…")}</Button>}
          {autoSend && onRecord && <Button variant="outline" disabled={busy} onClick={async () => { setBusy(true); try { if (await autoSend()) { toast.success("Receipt link sent on WhatsApp"); onOpenChange(false); } else toast.error("WhatsApp could not send it — open the chat instead"); } catch (e) { toast.error(e); } finally { setBusy(false); } }}><Send /> {t("Send automatically")}</Button>}
          {mobileDevice
            ? <Button variant="success" disabled={!ok} onClick={() => go("mobile")}><MessageCircle /> {t("Open WhatsApp")}</Button>
            : pref === "app" ? desktopButtons : [...desktopButtons].reverse()}
        </div>
      }>
      <div className="space-y-3" data-testid="wa-share">
        <Field label="Customer's WhatsApp number" hint={!number.trim() || `+${n}` === number.replace(/\s/g, "") ? undefined : ok ? `+${n}` : t("Enter a valid mobile number, e.g. 0712345678")}>
          <Input type="tel" inputMode="tel" autoComplete="off" value={number} onChange={(e) => setNumber(e.target.value)} placeholder={`+${cc} 712 345 678`} autoFocus={!mobile} />
        </Field>
        <Field label="Message">
          <Textarea rows={5} value={text} onChange={(e) => setText(e.target.value)} />
        </Field>
        {appMissing && (
          <p className="rounded-lg bg-warning/10 p-2.5 text-xs text-warning">
            {t("WhatsApp Desktop didn't open — it may not be installed.")}{" "}
            <button type="button" className="font-medium underline" onClick={() => go("web")}>{t("Use WhatsApp Web")}</button>
          </p>
        )}
      </div>
    </ResponsiveDialog>
  );
}
