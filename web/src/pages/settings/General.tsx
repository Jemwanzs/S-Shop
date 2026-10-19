import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Copy, ExternalLink, ImagePlus, Lock, XCircle } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { optimizeImage } from "@/lib/image";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ConfirmDialog, Field, ToggleRow } from "@/components/Form";
import { Card, SettingsPage, useSettings, useSettingsDraft } from "./shared";
import { t } from "@/lib/i18n";
import { MpesaAccounts, MpesaActivity, PlatformMpesaCard } from "./Mpesa";

type ProfileForm = { name: string; slug: string; tagline: string; phone: string; email: string; address: string; currency: string; timezone: string };
const pickProfile = (p: ProfileForm): ProfileForm => ({
  name: p.name, slug: p.slug, tagline: p.tagline, phone: p.phone, email: p.email, address: p.address, currency: p.currency, timezone: p.timezone,
});

export function BusinessSettings() {
  const { data, isLoading } = useSettings();
  const qc = useQueryClient();
  const [f, setF] = useState<ProfileForm>({ name: "", slug: "", tagline: "", phone: "", email: "", address: "", currency: "KSh", timezone: "Africa/Nairobi" });
  const [logoVersion, setLogoVersion] = useState(0);
  // Roadmap 145: the slug is the business's address (workspace, ordering link, sign-in page, website) — permanent
  // unless an administrator deliberately changes it; old addresses keep working.
  const [changeSlug, setChangeSlug] = useState(false);
  const [confirmSlug, setConfirmSlug] = useState(false);
  useEffect(() => {
    if (data) setF(pickProfile(data.profile));
  }, [data]);
  const slugChanged = !!data && f.slug.trim() !== data.profile.slug;
  const workspaceUrl = data ? `${location.origin}/${data.profile.slug}` : "";
  const copy = (text: string) => {
    navigator.clipboard.writeText(text).then(() => toast.success("Link copied"), () => toast.error("Couldn't copy — select the address and copy it"));
  };
  const save = useMutation({
    mutationFn: () => api("/settings/profile", { method: "PUT", body: f }),
    onSuccess: () => {
      toast.success("Business profile saved");
      setChangeSlug(false);
      setConfirmSlug(false);
      qc.invalidateQueries({ queryKey: ["settings"] });
      qc.invalidateQueries({ queryKey: ["me"] });
    },
    onError: (e) => {
      setConfirmSlug(false); // back to the form to correct the address
      toast.error(e);
    },
  });
  const uploadLogo = async (file?: File) => {
    if (!file) return;
    try {
      const fd = new FormData();
      fd.append("file", await optimizeImage(file, 512), "logo.webp");
      await api("/settings/logo", { body: fd });
      toast.success("Logo updated");
      setLogoVersion((v) => v + 1);
      qc.invalidateQueries({ queryKey: ["settings"] });
      qc.invalidateQueries({ queryKey: ["me"] });
    } catch (e) {
      toast.error(e);
    }
  };
  const dirty = !!data && JSON.stringify(f) !== JSON.stringify(pickProfile(data.profile));
  const set = (k: keyof typeof f) => (e: React.ChangeEvent<HTMLInputElement>) => setF({ ...f, [k]: e.target.value });

  return (
    <SettingsPage title="Business profile" description="Shown on receipts, the ordering link and WhatsApp messages." loading={isLoading} dirty={dirty} saving={save.isPending} onSave={() => (slugChanged ? setConfirmSlug(true) : save.mutate())} onReset={() => { if (data) setF(pickProfile(data.profile)); setChangeSlug(false); }}>
      <Card>
        <div className="flex items-center gap-4 py-3">
          {data?.profile.logo_url ? <img src={`${data.profile.logo_url}?v=${logoVersion}`} alt="" className="h-20 w-20 rounded-2xl object-contain" /> : <div className="h-20 w-20 rounded-2xl bg-muted" />}
          <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-primary">
            <ImagePlus className="h-4 w-4" /> {t("Upload logo")}
            <input type="file" accept="image/*" className="hidden" onChange={(e) => uploadLogo(e.target.files?.[0])} />
          </label>
        </div>
        <div className="grid gap-4 py-4 md:grid-cols-2">
          <Field label="Business name"><Input value={f.name} onChange={set("name")} /></Field>
          <Field label="Business address" hint={changeSlug ? t("Letters, numbers and dashes. Old links and bookmarks keep working and lead here.") : t("Used in your workspace URL, ordering link and website. Renaming the business doesn't change it.")}>
            <div className="flex gap-2">
              <Input value={f.slug} onChange={set("slug")} readOnly={!changeSlug} className={changeSlug ? "" : "bg-muted/50"} aria-label={t("Business address")} />
              {!changeSlug && <Button variant="outline" onClick={() => setChangeSlug(true)}><Lock /> {t("Change")}</Button>}
            </div>
          </Field>
          <Field label="Tagline" className="md:col-span-2"><Input value={f.tagline} onChange={set("tagline")} placeholder="Order directly from us. Your order will be attended to promptly by our team." /></Field>
          <Field label="Phone"><Input value={f.phone} onChange={set("phone")} /></Field>
          <Field label="Email"><Input value={f.email} onChange={set("email")} /></Field>
          <Field label="Address" className="md:col-span-2"><Input value={f.address} onChange={set("address")} /></Field>
          <Field label="Currency symbol"><Input value={f.currency} onChange={set("currency")} /></Field>
          <Field label="Time zone"><Input value={f.timezone} onChange={set("timezone")} /></Field>
        </div>
      </Card>
      {data && (
        <Card title="S'Shop Workspace URL">
          <p className="pt-3 text-sm text-muted-foreground">{t("Your team's own address for this business's workspace. Bookmark it or share it with staff — they still sign in, and only see what their role allows.")}</p>
          <div className="flex flex-wrap items-center gap-2 py-3">
            <code className="num min-w-0 flex-1 truncate rounded-lg bg-muted px-3 py-2 text-sm">{workspaceUrl}</code>
            <Button variant="outline" size="sm" onClick={() => copy(workspaceUrl)}><Copy /> {t("Copy link")}</Button>
          </div>
        </Card>
      )}
      {data && (
        <Card title="Customer ordering link">
          <div className="flex flex-wrap items-center gap-2 py-3">
            <code className="num min-w-0 flex-1 truncate rounded-lg bg-muted px-3 py-2 text-sm">{location.origin}/order/{data.profile.slug}</code>
            <Button variant="outline" size="sm" onClick={() => copy(`${location.origin}/order/${data.profile.slug}`)}><Copy /> {t("Copy")}</Button>
            <Button variant="outline" size="sm" asChild><a href={`/order/${data.profile.slug}`} target="_blank" rel="noreferrer"><ExternalLink /> {t("Open")}</a></Button>
          </div>
        </Card>
      )}
      <ConfirmDialog open={confirmSlug} onOpenChange={setConfirmSlug} title="Change your business address?" confirmLabel="Change address" busy={save.isPending}
        onConfirm={() => save.mutateAsync()}
        description={<>{t("New addresses:")} <b className="num break-all">{location.origin}/{f.slug.trim()}</b> {t("and")} <b className="num break-all">/order/{f.slug.trim()}</b>. {t("Old links and bookmarks keep working and lead here. Tell your team and customers the new address.")}</>} />
    </SettingsPage>
  );
}

export function IntegrationsSettings() {
  const d = useSettingsDraft();
  const i = d.query.data?.integrations;
  const Status = ({ ok }: { ok?: boolean }) =>
    ok ? <span className="inline-flex items-center gap-1 text-sm text-success"><CheckCircle2 className="h-4 w-4" /> {t("Connected")}</span> : <span className="inline-flex items-center gap-1 text-sm text-muted-foreground"><XCircle className="h-4 w-4" /> {t("Not configured")}</span>;
  return (
    <SettingsPage
      title="M-Pesa & WhatsApp"
      description="M-Pesa: your own Till or Paybill, with credentials encrypted on the server. WhatsApp: S'Shop's server configuration."
      loading={!d.draft}
      dirty={d.dirty}
      saving={d.saving}
      onSave={d.save}
      onReset={d.reset}
    >
      <PlatformMpesaCard />
      <MpesaAccounts />
      <MpesaActivity />
      <Card title="WhatsApp Cloud API" action={<Status ok={i?.whatsapp} />}>
        <div className="space-y-2 py-3 text-sm text-muted-foreground">
          <p>{i?.whatsapp ? "Messages are sent automatically. Customers can reply with an order number to get its status." : "Without the API, WhatsApp buttons open a pre-filled chat (wa.me) instead."}</p>
          {i && <p>{t("Webhook URL:")} <code className="num break-all rounded bg-muted px-1.5">{i.whatsapp_webhook_url}</code></p>}
        </div>
        {d.draft && (
          <>
            <ToggleRow label="Send receipts on WhatsApp" hint="After every sale with a customer" checked={d.draft.notifications.whatsapp_receipts} onChange={(v) => d.update((s) => { s.notifications.whatsapp_receipts = v; })} disabled={!i?.whatsapp} />
            <ToggleRow label="Overdue credit reminders" checked={d.draft.notifications.whatsapp_credit_reminders} onChange={(v) => d.update((s) => { s.notifications.whatsapp_credit_reminders = v; })} disabled={!i?.whatsapp} />
            <ToggleRow label="Award winner messages" checked={d.draft.notifications.whatsapp_loyalty} onChange={(v) => d.update((s) => { s.notifications.whatsapp_loyalty = v; })} disabled={!i?.whatsapp} />
            <ToggleRow label="Order status updates to customers" checked={d.draft.orders.notify_customer_whatsapp} onChange={(v) => d.update((s) => { s.orders.notify_customer_whatsapp = v; })} disabled={!i?.whatsapp} />
          </>
        )}
      </Card>
    </SettingsPage>
  );
}
