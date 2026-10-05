import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Copy, ExternalLink, ImagePlus, XCircle } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, errorMessage } from "@/lib/api";
import { optimizeImage } from "@/lib/image";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, ToggleRow } from "@/components/Form";
import { Card, SettingsPage, useSettings, useSettingsDraft } from "./shared";

type ProfileForm = { name: string; slug: string; tagline: string; phone: string; email: string; address: string; currency: string; timezone: string };
const pickProfile = (p: ProfileForm): ProfileForm => ({
  name: p.name, slug: p.slug, tagline: p.tagline, phone: p.phone, email: p.email, address: p.address, currency: p.currency, timezone: p.timezone,
});

export function BusinessSettings() {
  const { data, isLoading } = useSettings();
  const qc = useQueryClient();
  const [f, setF] = useState<ProfileForm>({ name: "", slug: "", tagline: "", phone: "", email: "", address: "", currency: "KSh", timezone: "Africa/Nairobi" });
  const [logoVersion, setLogoVersion] = useState(0);
  useEffect(() => {
    if (data) setF(pickProfile(data.profile));
  }, [data]);
  const save = useMutation({
    mutationFn: () => api("/settings/profile", { method: "PUT", body: f }),
    onSuccess: () => {
      toast.success("Business profile saved");
      qc.invalidateQueries({ queryKey: ["settings"] });
      qc.invalidateQueries({ queryKey: ["me"] });
    },
    onError: (e) => toast.error(errorMessage(e)),
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
      toast.error(errorMessage(e));
    }
  };
  const dirty = !!data && JSON.stringify(f) !== JSON.stringify(pickProfile(data.profile));
  const set = (k: keyof typeof f) => (e: React.ChangeEvent<HTMLInputElement>) => setF({ ...f, [k]: e.target.value });

  return (
    <SettingsPage title="Business profile" description="Shown on receipts, the ordering link and WhatsApp messages." loading={isLoading} dirty={dirty} saving={save.isPending} onSave={() => save.mutate()} onReset={() => data && setF(pickProfile(data.profile))}>
      <Card>
        <div className="flex items-center gap-4 py-3">
          {data?.profile.logo_url ? <img src={`${data.profile.logo_url}?v=${logoVersion}`} alt="" className="h-20 w-20 rounded-2xl object-cover" /> : <div className="h-20 w-20 rounded-2xl bg-muted" />}
          <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-primary">
            <ImagePlus className="h-4 w-4" /> Upload logo
            <input type="file" accept="image/*" className="hidden" onChange={(e) => uploadLogo(e.target.files?.[0])} />
          </label>
        </div>
        <div className="grid gap-4 py-4 md:grid-cols-2">
          <Field label="Business name"><Input value={f.name} onChange={set("name")} /></Field>
          <Field label="Ordering link" hint={data ? `${location.origin}/order/${f.slug}` : undefined}><Input value={f.slug} onChange={set("slug")} /></Field>
          <Field label="Tagline" className="md:col-span-2"><Input value={f.tagline} onChange={set("tagline")} placeholder="Order directly from us. Your order will be attended to promptly by our team." /></Field>
          <Field label="Phone"><Input value={f.phone} onChange={set("phone")} /></Field>
          <Field label="Email"><Input value={f.email} onChange={set("email")} /></Field>
          <Field label="Address" className="md:col-span-2"><Input value={f.address} onChange={set("address")} /></Field>
          <Field label="Currency symbol"><Input value={f.currency} onChange={set("currency")} /></Field>
          <Field label="Time zone"><Input value={f.timezone} onChange={set("timezone")} /></Field>
        </div>
      </Card>
      {data && (
        <Card title="Customer ordering link">
          <div className="flex flex-wrap items-center gap-2 py-3">
            <code className="num min-w-0 flex-1 truncate rounded-lg bg-muted px-3 py-2 text-sm">{location.origin}/order/{data.profile.slug}</code>
            <Button variant="outline" size="sm" onClick={() => { navigator.clipboard.writeText(`${location.origin}/order/${data.profile.slug}`); toast.success("Link copied"); }}><Copy /> Copy</Button>
            <Button variant="outline" size="sm" asChild><a href={`/order/${data.profile.slug}`} target="_blank" rel="noreferrer"><ExternalLink /> Open</a></Button>
          </div>
        </Card>
      )}
    </SettingsPage>
  );
}

export function IntegrationsSettings() {
  const d = useSettingsDraft();
  const i = d.query.data?.integrations;
  const Status = ({ ok }: { ok?: boolean }) =>
    ok ? <span className="inline-flex items-center gap-1 text-sm text-success"><CheckCircle2 className="h-4 w-4" /> Connected</span> : <span className="inline-flex items-center gap-1 text-sm text-muted-foreground"><XCircle className="h-4 w-4" /> Not configured</span>;
  return (
    <SettingsPage
      title="M-Pesa & WhatsApp"
      description="API credentials are server secrets (Railway variables), never stored in the app. See docs/integrations."
      loading={!d.draft}
      dirty={d.dirty}
      saving={d.saving}
      onSave={d.save}
      onReset={d.reset}
    >
      <Card title="M-Pesa (Daraja STK Push)" action={<Status ok={i?.mpesa_stk} />}>
        <p className="py-3 text-sm text-muted-foreground">
          {i?.mpesa_stk ? `Environment: ${i.mpesa_environment}. Cashiers can push payment prompts to customers' phones.` : "Set MPESA_CONSUMER_KEY, MPESA_CONSUMER_SECRET, MPESA_SHORTCODE, MPESA_PASSKEY and MPESA_CALLBACK_TOKEN to enable STK Push. Manual code entry works without it."}
        </p>
      </Card>
      <Card title="WhatsApp Cloud API" action={<Status ok={i?.whatsapp} />}>
        <div className="space-y-2 py-3 text-sm text-muted-foreground">
          <p>{i?.whatsapp ? "Messages are sent automatically. Customers can reply with an order number to get its status." : "Without the API, WhatsApp buttons open a pre-filled chat (wa.me) instead."}</p>
          {i && <p>Webhook URL: <code className="num break-all rounded bg-muted px-1.5">{i.whatsapp_webhook_url}</code></p>}
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
