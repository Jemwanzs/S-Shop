/** Services and testimonials. Sample testimonials are flagged and can never be published as genuine (roadmap 90:
 * two-level visibility — *Show testimonials* for the whole section, *Show this testimonial* for each; samples only in
 * the staff preview until confirmed genuine with the customer's own words). */
import { useState } from "react";
import { BadgeCheck, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { ConfirmDialog, Field, Select, ToggleRow } from "@/components/Form";
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
  const live = items.filter((x) => x.published && !x.sample).length;
  const [genuine, setGenuine] = useState<number | null>(null);
  const status = (r: (typeof items)[number]) =>
    r.sample ? (r.published ? t("Preview only — samples never appear on your public website.") : t("Sample: preview only. To show it publicly, replace it with a real review and confirm it below."))
    : !r.published ? t("Hidden from your website.")
    : !c.testimonials.show ? t("On — hidden while Show testimonials is off.")
    : t("Shown on your website once you publish.");
  return (
    <div className="space-y-4">
      <Block title="Testimonials carousel">
        <ToggleRow label="Show testimonials" hint={c.testimonials.show
          ? (live ? `${live} ${t("genuine testimonials shown")}` : t("Nothing is shown until at least one genuine testimonial is switched on."))
          : t("The whole section is hidden. Each testimonial keeps its own setting.")}
          checked={c.testimonials.show} onChange={(v) => set((x) => { x.testimonials.show = v; })} />
        <ToggleRow label="Scroll automatically" hint="Right to left; pauses when a visitor hovers, and stays still for visitors who prefer less motion" checked={c.testimonials.auto_scroll} onChange={(v) => set((x) => { x.testimonials.auto_scroll = v; })} />
        {c.testimonials.auto_scroll && <Field label="Speed"><Choice value={c.testimonials.speed} onChange={(v) => set((x) => { x.testimonials.speed = v; })} options={[["slow", "Slow"], ["normal", "Normal"], ["fast", "Fast"]]} /></Field>}
      </Block>
      <Block title="Testimonials" hint={samples ? `${samples} ${t("sample testimonials show the design — replace them with real reviews. Samples are never published.")}` : undefined}
        action={
          <div className="flex gap-1.5">
            {samples > 0 && <Button size="sm" variant="ghost" onClick={() => set((x) => { x.testimonials.items = x.testimonials.items.filter((y) => !y.sample); })}>{t("Remove samples")}</Button>}
            <Button size="sm" variant="outline" disabled={items.length >= 50} onClick={() => set((x) => { x.testimonials.items.push({ id: uid(), name: "", quote: "", rating: 5, photo: null, position: "", published: false, sample: false }); })}><Plus /> {t("Add")}</Button>
          </div>
        }>
        {items.map((r, i) => (
          <div key={r.id} className="space-y-2 rounded-lg border p-3">
            <div className="flex items-center gap-2">
              <Switch checked={r.published} onCheckedChange={(v) => set((x) => { x.testimonials.items[i].published = v; })}
                aria-label={r.sample ? t("Show in preview") : t("Show this testimonial")} />
              <Input value={r.name} maxLength={60} placeholder={t("Customer name")} className="h-8" onChange={(e) => set((x) => { x.testimonials.items[i].name = e.target.value; })} />
              {r.sample && <span className="shrink-0 rounded-full bg-warning/15 px-2 py-0.5 text-xs font-medium text-warning">{t("Sample")}</span>}
              <Move index={i} count={items.length} onMove={(a, b) => set((x) => moveItem(x.testimonials.items, a, b))} />
              <Button size="icon" variant="ghost" aria-label={t("Remove")} onClick={() => set((x) => { x.testimonials.items.splice(i, 1); })}><Trash2 /></Button>
            </div>
            <p className={r.sample ? "text-xs text-warning" : "text-xs text-muted-foreground"}>{status(r)}</p>
            {r.published && (!r.name.trim() || !r.quote.trim()) && <p className="text-xs text-destructive">{t("Add a name and a quote to show this testimonial.")}</p>}
            <Textarea value={r.quote} maxLength={500} placeholder={t("What the customer said")} onChange={(e) => set((x) => { x.testimonials.items[i].quote = e.target.value; })} />
            <Grid2>
              <Field label="Location or role" optional><Input value={r.position} maxLength={60} onChange={(e) => set((x) => { x.testimonials.items[i].position = e.target.value; })} /></Field>
              <Field label="Rating"><Choice value={r.rating} onChange={(v) => set((x) => { x.testimonials.items[i].rating = v; })} options={[[0, "None"], [4, "★★★★"], [5, "★★★★★"]]} /></Field>
            </Grid2>
            <MediaField label="Photo" kind="testimonial" value={r.photo} onChange={(id) => set((x) => { x.testimonials.items[i].photo = id; })} />
            {r.sample && (
              <Button size="sm" variant="outline" onClick={() => setGenuine(i)}>
                <BadgeCheck /> {t("This is a real customer review")}
              </Button>
            )}
          </div>
        ))}
        {!items.length && <p className="text-sm text-muted-foreground">{t("No testimonials yet.")}</p>}
      </Block>
      <ConfirmDialog open={genuine !== null} onOpenChange={(o) => !o && setGenuine(null)} title="Confirm a genuine review"
        description={t("Confirm that this review was given by this customer and that the name, words and photo are theirs. Sample text cannot be published — replace it with the customer's own words first.")}
        confirmLabel="It's a genuine review"
        onConfirm={() => { const i = genuine; if (i !== null) set((x) => { x.testimonials.items[i].sample = false; }); setGenuine(null); }}>
        {genuine !== null && items[genuine] && (
          <p className="rounded-lg bg-muted/60 p-3 text-sm">“{items[genuine].quote}” — <b>{items[genuine].name || t("Customer name")}</b>
            <span className="mt-1 block text-xs text-muted-foreground">{items[genuine].published ? t("It will appear on your website after you publish.") : t("Switch on Show this testimonial to show it.")}</span></p>
        )}
      </ConfirmDialog>
    </div>
  );
}
