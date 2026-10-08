/** Services and testimonials. Sample testimonials are flagged and can never be published as genuine. */
import { BadgeCheck, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { Field, Select, ToggleRow } from "@/components/Form";
import { t } from "@/lib/i18n";
import { uid } from "./data";
import { Block, Choice, CtaEditor, Grid2, MediaField, Move, moveItem } from "./kit";
import type { TabProps } from "./Website";

const ICONS = ["", "truck", "gift", "sparkles", "scissors", "wrench", "shield", "heart", "star", "clock", "phone", "package", "home", "ruler", "palette", "shirt", "coffee"];

export function ServicesTab({ c, set }: TabProps) {
  const items = c.services.items;
  return (
    <div className="space-y-4">
      <Block title="Services" hint="Delivery, installation, gift wrapping, repairs, consultations…"
        action={<Button size="sm" variant="outline" disabled={items.length >= 24} onClick={() => set((x) => { x.services.items.push({ id: uid(), name: "", icon: "sparkles", image: null, short: "", details: "", cta: { label: "", target: "" }, active: true }); })}><Plus /> {t("Add service")}</Button>}>
        <Field label="Introduction" optional><Textarea value={c.services.intro} maxLength={400} onChange={(e) => set((x) => { x.services.intro = e.target.value; })} /></Field>
        {!items.length && <p className="text-sm text-muted-foreground">{t("No services yet.")}</p>}
        {items.map((s, i) => (
          <div key={s.id} className="space-y-2 rounded-lg border p-3">
            <div className="flex items-center gap-2">
              <Switch checked={s.active} onCheckedChange={(v) => set((x) => { x.services.items[i].active = v; })} aria-label={t("Active")} />
              <Input value={s.name} maxLength={80} placeholder={t("Service name")} className="h-8" onChange={(e) => set((x) => { x.services.items[i].name = e.target.value; })} />
              <Move index={i} count={items.length} onMove={(a, b) => set((x) => moveItem(x.services.items, a, b))} />
              <Button size="icon" variant="ghost" aria-label={t("Remove")} onClick={() => set((x) => { x.services.items.splice(i, 1); })}><Trash2 /></Button>
            </div>
            <Grid2>
              <Field label="Short description"><Input value={s.short} maxLength={160} onChange={(e) => set((x) => { x.services.items[i].short = e.target.value; })} /></Field>
              <Field label="Icon"><Select value={s.icon} onChange={(v) => set((x) => { x.services.items[i].icon = v; })}>{ICONS.map((k) => <option key={k} value={k}>{k ? k[0].toUpperCase() + k.slice(1) : t("None")}</option>)}</Select></Field>
            </Grid2>
            <Field label="Details" optional><Textarea value={s.details} maxLength={2000} onChange={(e) => set((x) => { x.services.items[i].details = e.target.value; })} /></Field>
            <MediaField label="Image" kind="service" value={s.image} onChange={(id) => set((x) => { x.services.items[i].image = id; })} hint="Shown instead of the icon" />
            <CtaEditor label="Button" value={s.cta} onChange={(v) => set((x) => { x.services.items[i].cta = v; })} />
          </div>
        ))}
      </Block>
    </div>
  );
}

export function TestimonialsTab({ c, set }: TabProps) {
  const items = c.testimonials.items;
  const samples = items.filter((x) => x.sample).length;
  return (
    <div className="space-y-4">
      <Block title="Testimonials carousel">
        <ToggleRow label="Show testimonials" checked={c.testimonials.show} onChange={(v) => set((x) => { x.testimonials.show = v; })} />
        <ToggleRow label="Scroll automatically" hint="Right to left; pauses when a visitor hovers, and stays still for visitors who prefer less motion" checked={c.testimonials.auto_scroll} onChange={(v) => set((x) => { x.testimonials.auto_scroll = v; })} />
        {c.testimonials.auto_scroll && <Field label="Speed"><Choice value={c.testimonials.speed} onChange={(v) => set((x) => { x.testimonials.speed = v; })} options={[["slow", "Slow"], ["normal", "Normal"], ["fast", "Fast"]]} /></Field>}
      </Block>
      <Block title="Testimonials" hint={samples ? `${samples} ${t("sample testimonials show the design — replace them with real reviews. Samples are never published.")}` : undefined}
        action={
          <div className="flex gap-1.5">
            {samples > 0 && <Button size="sm" variant="ghost" onClick={() => set((x) => { x.testimonials.items = x.testimonials.items.filter((y) => !y.sample); })}>{t("Remove samples")}</Button>}
            <Button size="sm" variant="outline" disabled={items.length >= 30} onClick={() => set((x) => { x.testimonials.items.push({ id: uid(), name: "", quote: "", rating: 5, photo: null, position: "", published: false, sample: false }); })}><Plus /> {t("Add")}</Button>
          </div>
        }>
        {items.map((r, i) => (
          <div key={r.id} className="space-y-2 rounded-lg border p-3">
            <div className="flex items-center gap-2">
              <Switch checked={r.published && !r.sample} disabled={r.sample} onCheckedChange={(v) => set((x) => { x.testimonials.items[i].published = v; })} aria-label={t("Published")} />
              <Input value={r.name} maxLength={60} placeholder={t("Customer name")} className="h-8" onChange={(e) => set((x) => { x.testimonials.items[i].name = e.target.value; })} />
              {r.sample && <span className="shrink-0 rounded-full bg-warning/15 px-2 py-0.5 text-xs font-medium text-warning">{t("Sample")}</span>}
              <Move index={i} count={items.length} onMove={(a, b) => set((x) => moveItem(x.testimonials.items, a, b))} />
              <Button size="icon" variant="ghost" aria-label={t("Remove")} onClick={() => set((x) => { x.testimonials.items.splice(i, 1); })}><Trash2 /></Button>
            </div>
            <Textarea value={r.quote} maxLength={500} placeholder={t("What the customer said")} onChange={(e) => set((x) => { x.testimonials.items[i].quote = e.target.value; })} />
            <Grid2>
              <Field label="Location or role" optional><Input value={r.position} maxLength={60} onChange={(e) => set((x) => { x.testimonials.items[i].position = e.target.value; })} /></Field>
              <Field label="Rating"><Choice value={r.rating} onChange={(v) => set((x) => { x.testimonials.items[i].rating = v; })} options={[[0, "None"], [4, "★★★★"], [5, "★★★★★"]]} /></Field>
            </Grid2>
            <MediaField label="Photo" kind="testimonial" value={r.photo} onChange={(id) => set((x) => { x.testimonials.items[i].photo = id; })} />
            {r.sample && (
              <Button size="sm" variant="outline" onClick={() => set((x) => { x.testimonials.items[i].sample = false; })}>
                <BadgeCheck /> {t("This is a real customer review")}
              </Button>
            )}
          </div>
        ))}
        {!items.length && <p className="text-sm text-muted-foreground">{t("No testimonials yet.")}</p>}
      </Block>
    </div>
  );
}
