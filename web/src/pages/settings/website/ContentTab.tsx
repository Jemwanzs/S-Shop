/** Content: navigation, home page sections, hero, offers, about, contact, social links, footer and cookies. */
import { Plus, Trash2 } from "lucide-react";
import { useSession } from "@/lib/session";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { Field, ToggleRow } from "@/components/Form";
import { t } from "@/lib/i18n";
import { uid } from "./data";
import { Block, Choice, CtaEditor, Grid2, MediaField, Move, moveItem } from "./kit";
import type { TabProps } from "./Website";

const SECTION_NAMES: Record<string, string> = {
  hero: "Hero banner", categories: "Categories", featured: "Featured products", new_arrivals: "New arrivals", popular: "Most popular",
  promotions: "Offers & promotions", about: "About", services: "Services", testimonials: "Testimonials", cta: "Call to action", contact: "Contact",
};

export function ContentTab({ c, set }: TabProps) {
  const { can } = useSession();
  const content = can("website.content");
  return (
    <div className="space-y-4">
      {can("website.navigation") && (
        <Block title="Navigation" hint="Show, hide, rename and reorder the menu.">
          <ul className="divide-y">
            {c.navigation.map((n, i) => (
              <li key={n.key} className="flex items-center gap-2 py-2">
                <Switch checked={n.visible} disabled={n.key === "home"} onCheckedChange={(v) => set((x) => { x.navigation[i].visible = v; })} aria-label={`${t("Show")} ${n.label}`} />
                <Input value={n.label} maxLength={24} className="h-8" onChange={(e) => set((x) => { x.navigation[i].label = e.target.value; })} />
                <Move index={i} count={c.navigation.length} onMove={(a, b) => set((x) => moveItem(x.navigation, a, b))} />
              </li>
            ))}
          </ul>
        </Block>
      )}
      {content && (
        <>
          <Block title="Home page sections" hint="Show, hide, edit and reorder what the home page shows.">
            <ul className="divide-y">
              {c.sections.map((s, i) => (
                <li key={s.key} className="space-y-2 py-2.5">
                  <div className="flex items-center gap-2">
                    <Switch checked={s.visible} onCheckedChange={(v) => set((x) => { x.sections[i].visible = v; })} aria-label={`${t("Show")} ${t(SECTION_NAMES[s.key] ?? s.key)}`} />
                    <span className="flex-1 text-sm font-medium">{t(SECTION_NAMES[s.key] ?? s.key)}</span>
                    <Move index={i} count={c.sections.length} onMove={(a, b) => set((x) => moveItem(x.sections, a, b))} />
                  </div>
                  {s.visible && s.key !== "hero" && (
                    <div className="grid gap-2 ps-11 sm:grid-cols-2">
                      <Input value={s.heading} maxLength={80} placeholder={t("Heading")} className="h-8" onChange={(e) => set((x) => { x.sections[i].heading = e.target.value; })} />
                      <Input value={s.subheading} maxLength={160} placeholder={t("Subheading")} className="h-8" onChange={(e) => set((x) => { x.sections[i].subheading = e.target.value; })} />
                      {["categories", "featured", "new_arrivals", "popular"].includes(s.key) && (
                        <Choice value={s.layout || "grid"} onChange={(v) => set((x) => { x.sections[i].layout = v; })} options={[["grid", "Grid"], ["carousel", "Carousel"]]} />
                      )}
                      {s.key === "cta" && (
                        <div className="sm:col-span-2">
                          <CtaEditor label="Button" value={{ label: s.cta_label, target: s.cta_target }} onChange={(v) => set((x) => { x.sections[i].cta_label = v.label; x.sections[i].cta_target = v.target; })} />
                        </div>
                      )}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          </Block>

          <Block title="Hero banner">
            <Grid2>
              <Field label="Headline"><Input value={c.hero.headline} maxLength={90} onChange={(e) => set((x) => { x.hero.headline = e.target.value; })} /></Field>
              <Field label="Text alignment"><Choice value={c.hero.align || "left"} onChange={(v) => set((x) => { x.hero.align = v as "left"; })} options={[["left", "Left"], ["center", "Centre"]]} /></Field>
            </Grid2>
            <Field label="Supporting text"><Textarea value={c.hero.text} maxLength={300} onChange={(e) => set((x) => { x.hero.text = e.target.value; })} /></Field>
            <MediaField label="Banner image" kind="banner" value={c.hero.image} onChange={(id) => set((x) => { x.hero.image = id; })} hint="Landscape, at least 1600 px wide, works best." />
            {c.hero.image && <ToggleRow label="Darken the image behind the text" hint="Keeps the headline readable on busy photos" checked={c.hero.overlay} onChange={(v) => set((x) => { x.hero.overlay = v; })} />}
            <Grid2>
              <CtaEditor label="Main button" value={c.hero.primary} onChange={(v) => set((x) => { x.hero.primary = v; })} />
              <CtaEditor label="Second button" value={c.hero.secondary} onChange={(v) => set((x) => { x.hero.secondary = v; })} />
            </Grid2>
          </Block>

          <Block title="Offers & promotions" action={<Button size="sm" variant="outline" disabled={c.promotions.length >= 6} onClick={() => set((x) => { x.promotions.push({ id: uid(), title: "", text: "", image: null, cta: { label: "", target: "" }, active: true }); })}><Plus /> {t("Add")}</Button>}>
            {!c.promotions.length && <p className="text-sm text-muted-foreground">{t("No offers yet.")}</p>}
            {c.promotions.map((p, i) => (
              <div key={p.id} className="space-y-2 rounded-lg border p-3">
                <div className="flex items-center gap-2">
                  <Switch checked={p.active} onCheckedChange={(v) => set((x) => { x.promotions[i].active = v; })} aria-label={t("Active")} />
                  <Input value={p.title} maxLength={80} placeholder={t("Title")} className="h-8" onChange={(e) => set((x) => { x.promotions[i].title = e.target.value; })} />
                  <Button size="icon" variant="ghost" aria-label={t("Remove")} onClick={() => set((x) => { x.promotions.splice(i, 1); })}><Trash2 /></Button>
                </div>
                <Textarea value={p.text} maxLength={240} placeholder={t("Details")} onChange={(e) => set((x) => { x.promotions[i].text = e.target.value; })} />
                <MediaField label="Image" kind="promotion" value={p.image} onChange={(id) => set((x) => { x.promotions[i].image = id; })} />
                <CtaEditor label="Button" value={p.cta} onChange={(v) => set((x) => { x.promotions[i].cta = v; })} />
              </div>
            ))}
          </Block>

          <Block title="About Us page">
            <Field label="Introduction"><Textarea value={c.about.intro} maxLength={600} onChange={(e) => set((x) => { x.about.intro = e.target.value; })} /></Field>
            <MediaField label="Image" kind="about" value={c.about.image} onChange={(id) => set((x) => { x.about.image = id; })} />
            {([["story", "Our story", "show_story"], ["mission", "Mission", "show_mission"], ["vision", "Vision", "show_vision"]] as const).map(([k, label, show]) => (
              <div key={k} className="space-y-1.5">
                <ToggleRow label={label} checked={c.about[show]} onChange={(v) => set((x) => { x.about[show] = v; })} />
                {c.about[show] && <Textarea value={c.about[k]} maxLength={2000} onChange={(e) => set((x) => { x.about[k] = e.target.value; })} />}
              </div>
            ))}
            <ToggleRow label="Values" hint="One per line" checked={c.about.show_values} onChange={(v) => set((x) => { x.about.show_values = v; })} />
            {c.about.show_values && <Textarea value={c.about.values.join("\n")} onChange={(e) => set((x) => { x.about.values = e.target.value.split("\n").map((s) => s.slice(0, 80)).slice(0, 12); })} />}
            <CtaEditor label="Button on the home page" value={c.about.cta} onChange={(v) => set((x) => { x.about.cta = v; })} />
          </Block>

          <Block title="Contact" hint="Shown on the Contact page, the home page and the footer.">
            <Field label="Introduction"><Input value={c.contact.intro} maxLength={200} onChange={(e) => set((x) => { x.contact.intro = e.target.value; })} /></Field>
            {([["phone", "Phone", "show_phone"], ["whatsapp", "WhatsApp number", "show_whatsapp"], ["email", "Email", "show_email"], ["location", "Location", "show_location"]] as const).map(([k, label, show]) => (
              <div key={k} className="flex items-center gap-2">
                <Switch checked={c.contact[show]} onCheckedChange={(v) => set((x) => { x.contact[show] = v; })} aria-label={`${t("Show")} ${t(label)}`} />
                <Input value={c.contact[k]} maxLength={160} placeholder={t(label)} onChange={(e) => set((x) => { x.contact[k] = e.target.value; })} />
              </div>
            ))}
            <Field label="Map link" optional hint="Leave empty to use the location"><Input value={c.contact.map_url} placeholder="https://maps.google.com/…" onChange={(e) => set((x) => { x.contact.map_url = e.target.value.trim(); })} /></Field>
            <ToggleRow label="Opening hours" checked={c.contact.show_hours} onChange={(v) => set((x) => { x.contact.show_hours = v; })} />
            {c.contact.show_hours && <Textarea value={c.contact.hours} maxLength={400} placeholder={"Mon – Sat: 8am – 8pm\nSun: 10am – 4pm"} onChange={(e) => set((x) => { x.contact.hours = e.target.value; })} />}
          </Block>

          <Block title="Social media" hint="Full links starting with https://">
            <Grid2>
              {(["instagram", "facebook", "tiktok", "x", "linkedin", "youtube"] as const).map((k) => (
                <Field key={k} label={k === "x" ? "X (Twitter)" : k[0].toUpperCase() + k.slice(1)}>
                  <Input value={c.social[k]} placeholder="https://" onChange={(e) => set((x) => { x.social[k] = e.target.value.trim(); })} />
                </Field>
              ))}
            </Grid2>
          </Block>

          <Block title="Footer & cookies">
            <Field label="Footer description"><Textarea value={c.footer.description} maxLength={300} onChange={(e) => set((x) => { x.footer.description = e.target.value; })} /></Field>
            <Field label="Policies" optional hint="Returns, delivery and payment notes"><Textarea value={c.footer.policies} maxLength={1500} onChange={(e) => set((x) => { x.footer.policies = e.target.value; })} /></Field>
            <ToggleRow label="Show “Powered by S'Shop”" checked={c.footer.show_attribution} onChange={(v) => set((x) => { x.footer.show_attribution = v; })} />
            <ToggleRow label="Ask visitors for analytics consent" hint="A cookie notice; visitors who decline are not counted. The cart works either way." checked={c.cookies.analytics} onChange={(v) => set((x) => { x.cookies.analytics = v; })} />
            <Field label="Privacy policy link" optional><Input value={c.cookies.privacy_policy} placeholder="https://" onChange={(e) => set((x) => { x.cookies.privacy_policy = e.target.value.trim(); })} /></Field>
          </Block>
        </>
      )}
    </div>
  );
}
