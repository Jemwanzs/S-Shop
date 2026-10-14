/**
 * Roadmap 112 — Settings → Business → Branding & White Labelling: the business logo (the one uploaded for the
 * business — never a second copy) and where it replaces S'Shop's, with previews. Saving shows *Business Branding
 * Updated* with a single Log Out (the new branding is picked up at the next sign-in).
 */
import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Eye, ImagePlus, LogOut, Moon, Sun, Trash2 } from "lucide-react";
import mark from "@/assets/sshop-mark.png";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { optimizeImage } from "@/lib/image";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { Settings } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { ConfirmDialog, ToggleRow } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Card, SettingsPage, useSettings, useSettingsDraft } from "./shared";

type Branding = NonNullable<Settings["branding"]>;
const DEFAULT_BRANDING: Branding = {
  enabled: false, login: true, login_marketing: false, sidebar: true, dashboard: true, pos: true, documents: true, customer: true, website: true, favicon: false,
};
const PLACES: [keyof Branding, string, string][] = [
  ["login", "Login & authentication", "The icon above the sign-in form and Quick PIN keypad, where your business is known (trusted device, your own sign-in address)"],
  ["login_marketing", "Large logo on your own sign-in address", "Replaces the big S'Shop logo only on your business's sign-in address — the shared sign-in page stays S'Shop"],
  ["sidebar", "App sidebar & header", "Desktop sidebar, tablet and mobile navigation header"],
  ["dashboard", "Dashboard & My Dashboard", "Where a brand mark appears as page branding (never repeated on cards or charts)"],
  ["pos", "POS / Record Sale", "The application header while selling — not product cards, the cart or payment"],
  ["documents", "Receipts & business documents", "Sales receipts, credit statements, invoices and order receipts"],
  ["customer", "Customer orders & notifications", "Order confirmations, order tracking, payment confirmations and emails"],
  ["website", "Public website & online store", "Follows your business logo unless the website has its own logo"],
  ["favicon", "Browser tab & installed app icon (advanced)", "Uses your logo as the favicon and app icon where supported — S'Shop by default"],
];

/** SVG and other formats become a 512 px WebP in the browser (scripts in an SVG never run in an image). */
async function toLogo(file: File): Promise<Blob> {
  if (file.type !== "image/svg+xml") return optimizeImage(file, 512, 0.9);
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    await new Promise<void>((ok, fail) => { img.onload = () => ok(); img.onerror = () => fail(new Error(t("That SVG could not be read"))); img.src = url; });
    const w = img.naturalWidth || 512, h = img.naturalHeight || 512, scale = 512 / Math.max(w, h);
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(w * scale);
    canvas.height = Math.round(h * scale);
    canvas.getContext("2d")!.drawImage(img, 0, 0, canvas.width, canvas.height);
    const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, "image/webp", 0.92));
    if (!blob) throw new Error(t("That SVG could not be read"));
    return blob;
  } finally {
    URL.revokeObjectURL(url);
  }
}

export function BrandingSettings() {
  const qc = useQueryClient();
  const { signOut } = useSession();
  const [updated, setUpdated] = useState(false);
  const d = useSettingsDraft({
    onSaved: (before, after) => {
      // Only branding changes ask for a new sign-in; anything else just confirms.
      if (JSON.stringify(before.branding ?? DEFAULT_BRANDING) !== JSON.stringify(after.branding ?? DEFAULT_BRANDING)) setUpdated(true);
      else toast.success("Settings saved");
    },
  });
  const profile = useSettings();
  const [version, setVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [preview, setPreview] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const logo = profile.data?.profile.logo_url ? `${profile.data.profile.logo_url}?v=${version}` : null;
  const name = profile.data?.profile.name ?? "";
  const b: Branding = d.draft?.branding ?? DEFAULT_BRANDING;
  const set = (patch: Partial<Branding>) => d.update((x) => { x.branding = { ...(x.branding ?? DEFAULT_BRANDING), ...patch }; });

  const upload = async (file: File) => {
    setBusy(true);
    try {
      const fd = new FormData();
      fd.append("file", await toLogo(file), "logo.webp");
      await api("/settings/logo", { body: fd });
      await qc.invalidateQueries({ queryKey: ["settings"] });
      await qc.invalidateQueries({ queryKey: ["me"] });
      setVersion((v) => v + 1);
      if (b.enabled) setUpdated(true);
      else toast.success("Logo updated");
    } catch (e) {
      toast.error(e);
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };
  const remove = async () => {
    setBusy(true);
    try {
      await api("/settings/logo", { method: "DELETE" });
      await qc.invalidateQueries({ queryKey: ["settings"] });
      await qc.invalidateQueries({ queryKey: ["me"] });
      setConfirmRemove(false);
      if (b.enabled) setUpdated(true);
      else toast.success("Logo removed");
    } catch (e) {
      toast.error(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsPage title="Branding & White Labelling" description="Show your business logo in place of S'Shop's where you choose. S'Shop stays as a subtle “Powered by S'Shop”."
      loading={!d.draft} dirty={d.dirty} saving={d.saving} onSave={d.save} onReset={d.reset}>
      <Card>
        <ToggleRow label="Enable business branding" hint="Off: S'Shop branding as it is today" checked={b.enabled} onChange={(v) => set({ enabled: v })} />
      </Card>
      <Card title="Business logo">
        <div className="flex flex-wrap items-center gap-4 py-3">
          <div className="flex gap-2">
            {[["light", Sun, "bg-[#faf5ee]"], ["dark", Moon, "bg-[#16110d]"]].map(([k, Icon, bg]) => (
              <div key={k as string} className={cn("flex h-24 w-24 flex-col items-center justify-center gap-1 rounded-xl border", bg as string)}>
                {logo ? <img src={logo} alt={name} className="h-14 w-14 object-contain" /> : <img src={mark} alt="S'Shop" className="h-12 w-12 object-contain opacity-60" />}
                {(() => { const I = Icon as typeof Sun; return <I className={cn("h-3 w-3", k === "dark" ? "text-white/60" : "text-black/40")} aria-label={t(k === "dark" ? "Dark background" : "Light background")} />; })()}
              </div>
            ))}
          </div>
          <div className="space-y-2 text-sm">
            <p className="text-xs text-muted-foreground">{t(logo ? "This is the logo uploaded for your business. Transparent backgrounds are kept." : "No logo yet — S'Shop branding is used until you upload one.")}</p>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" variant="outline" disabled={busy} onClick={() => input.current?.click()}><ImagePlus /> {t(logo ? "Replace logo" : "Upload logo")}</Button>
              {logo && <Button size="sm" variant="ghost" className="text-destructive" disabled={busy} onClick={() => setConfirmRemove(true)}><Trash2 /> {t("Remove logo")}</Button>}
              <Button size="sm" variant="ghost" onClick={() => setPreview(true)}><Eye /> {t("Preview branding")}</Button>
            </div>
            <p className="text-[11px] text-muted-foreground">{t("PNG, SVG, WebP or JPEG, up to 2 MB. Square logos look best.")}</p>
            <input ref={input} type="file" accept="image/png,image/svg+xml,image/webp,image/jpeg" className="hidden" onChange={(e) => { const f = e.target.files?.[0]; if (f) void upload(f); }} />
          </div>
        </div>
      </Card>
      <Card title="Where to display your business logo">
        <div className={cn(!b.enabled && "opacity-50")} aria-disabled={!b.enabled}>
          {PLACES.map(([k, label, hint]) => (
            <ToggleRow key={k} label={label} hint={hint} checked={!!b[k]} disabled={!b.enabled} onChange={(v) => set({ [k]: v } as Partial<Branding>)} />
          ))}
        </div>
        <p className="py-2 text-xs text-muted-foreground">{t("The platform's own administration always shows S'Shop. “Powered by S'Shop” stays on sign-in, the app and customer pages.")}</p>
      </Card>

      <ConfirmDialog open={confirmRemove} onOpenChange={setConfirmRemove} title="Remove the business logo?" description="S'Shop branding is shown wherever your logo appeared. You can upload a logo again any time."
        confirmLabel="Remove logo" destructive busy={busy} onConfirm={() => void remove()} />
      {preview && <BrandingPreview logo={logo} name={name} branding={b} onClose={() => setPreview(false)} />}
      {updated && (
        <ResponsiveDialog open onOpenChange={() => {}} confirmDiscard={false} required title="Business Branding Updated">
          <div className="space-y-4 py-2 text-center">
            <p className="text-sm">{t("Your business logo has been updated successfully.")}</p>
            <p className="text-sm text-muted-foreground">{t("Please log out and sign in again to synchronize your new branding across S'Shop.")}</p>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button size="lg" className="w-full sm:w-auto" onClick={signOut}><LogOut /> {t("Log Out")}</Button>
              </TooltipTrigger>
              <TooltipContent>{t("Log out to refresh your business logo across the application.")}</TooltipContent>
            </Tooltip>
          </div>
        </ResponsiveDialog>
      )}
    </SettingsPage>
  );
}

/** Small faithful previews of where the logo goes, light and dark. */
function BrandingPreview({ logo, name, branding, onClose }: { logo: string | null; name: string; branding: Branding; onClose: () => void }) {
  const [dark, setDark] = useState(false);
  const on = (k: keyof Branding) => branding.enabled && !!branding[k] && !!logo;
  const Mark = ({ k, size = "h-8 w-8" }: { k: keyof Branding; size?: string }) =>
    on(k) ? <img src={logo!} alt={name} className={cn(size, "object-contain")} /> : <img src={mark} alt="S'Shop" className={cn(size, "object-contain")} />;
  const panel = cn("rounded-xl border p-3", dark ? "border-white/10 bg-[#16110d] text-white" : "bg-[#faf5ee]");
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} wide confirmDiscard={false} title="Preview branding"
      footer={<div className="flex w-full justify-between"><Button size="sm" variant="outline" onClick={() => setDark(!dark)}>{dark ? <Sun /> : <Moon />} {t(dark ? "Light" : "Dark")}</Button><Button size="sm" onClick={onClose}>{t("Done")}</Button></div>}>
      <div className="grid gap-3 py-1 sm:grid-cols-2">
        <section className={panel}>
          <p className="label-caps mb-2">{t("Login")}</p>
          <div className="flex flex-col items-center gap-1.5 py-2">
            <Mark k="login" size="h-10 w-10" />
            <span className="text-sm font-semibold">{on("login") ? name : "S'Shop"}</span>
            <span className="h-7 w-40 rounded-md border opacity-60" />
            <span className="text-[10px] opacity-60">{t("Powered by S'Shop")}</span>
          </div>
        </section>
        <section className={panel}>
          <p className="label-caps mb-2">{t("Sidebar & mobile header")}</p>
          <div className="flex items-center gap-2"><Mark k="sidebar" /><span className="text-sm font-semibold">{name || "S'Shop"}</span></div>
          <div className="mt-3 flex items-center gap-2 rounded-lg border px-2 py-1.5 text-xs"><Mark k="sidebar" size="h-5 w-5" /><span>{name}</span></div>
        </section>
        <section className={panel}>
          <p className="label-caps mb-2">{t("POS / Record Sale")}</p>
          <div className="flex items-center gap-2 border-b pb-2"><Mark k="pos" size="h-6 w-6" /><span className="text-sm font-semibold">{t("Record Sale")}</span></div>
          <div className="mt-2 grid grid-cols-2 gap-1.5">{[1, 2, 3, 4].map((i) => <span key={i} className="h-8 rounded-md border opacity-60" />)}</div>
        </section>
        <section className={cn(panel, "bg-white text-black")}>
          <p className="label-caps mb-2">{t("Receipt")}</p>
          <div className="mx-auto w-40 space-y-1 text-center font-mono text-[10px]">
            <div className="flex justify-center"><Mark k="documents" size="h-8 w-8" /></div>
            <div className="font-bold">{name}</div>
            <div className="border-t border-dashed border-black/40 pt-1">1 × Item … 100</div>
            <div className="font-bold">TOTAL 100</div>
          </div>
        </section>
      </div>
    </ResponsiveDialog>
  );
}
