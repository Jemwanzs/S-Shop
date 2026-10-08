/** Design: brand, colours (with readability checks), fonts, style, spacing, themes, product cards and category display. */
import { Input } from "@/components/ui/input";
import { Field, Select, ToggleRow } from "@/components/Form";
import { Button } from "@/components/ui/button";
import { t } from "@/lib/i18n";
import { contrast, onColor, type Palette } from "./data";
import { Block, Choice, ColorField, Grid2, MediaField } from "./kit";
import type { TabProps } from "./Website";

const FONTS = ["Outfit", "Poppins", "Inter", "Roboto", "Nunito"];
const LIGHT: Palette = { primary: "#C2410C", secondary: "#7C2D12", accent: "#F59E0B", background: "#FFFBF5", surface: "#FFFFFF", text: "#1C1917", muted: "#57534E", heading: "#1C1917" };
const DARK: Palette = { primary: "#FB923C", secondary: "#FDBA74", accent: "#FBBF24", background: "#14110F", surface: "#1F1A17", text: "#F5F0EB", muted: "#B9AFA6", heading: "#FFFFFF" };

/** Same rules the server enforces before saving. */
export function paletteProblems(p: Palette): string[] {
  const rules: [string, string, string, number][] = [
    ["Body text on the background", p.text, p.background, 4.5],
    ["Body text on cards", p.text, p.surface, 4.5],
    ["Headings on the background", p.heading, p.background, 4.5],
    ["Muted text on the background", p.muted, p.background, 3],
    ["Buttons", onColor(p.primary), p.primary, 3],
    ["Primary colour on the background", p.primary, p.background, 2],
  ];
  return rules.filter(([, a, b, min]) => contrast(a, b) < min).map(([what]) => what);
}

function PaletteEditor({ title, p, onChange, reset }: { title: string; p: Palette; onChange: (p: Palette) => void; reset: Palette }) {
  const f = (k: keyof Palette) => (v: string) => onChange({ ...p, [k]: v });
  const problems = paletteProblems(p);
  return (
    <Block title={title} action={<Button size="sm" variant="ghost" onClick={() => onChange(reset)}>{t("Reset")}</Button>}>
      <div className="flex flex-wrap items-center gap-3 rounded-lg p-3" style={{ background: p.background, color: p.text }}>
        <span style={{ color: p.heading, fontWeight: 700 }}>{t("Heading")}</span>
        <span>{t("Body text")}</span>
        <span style={{ color: p.muted }}>{t("Muted")}</span>
        <span className="rounded-full px-3 py-1 text-sm font-semibold" style={{ background: p.primary, color: onColor(p.primary) }}>{t("Button")}</span>
        <span className="rounded-md px-2 py-1 text-sm" style={{ background: p.surface }}>{t("Card")}</span>
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        <ColorField label="Primary (buttons, links)" value={p.primary} onChange={f("primary")} against={p.background} min={2} />
        <ColorField label="Secondary" value={p.secondary} onChange={f("secondary")} />
        <ColorField label="Accent (badges, stars)" value={p.accent} onChange={f("accent")} />
        <ColorField label="Background" value={p.background} onChange={f("background")} />
        <ColorField label="Cards" value={p.surface} onChange={f("surface")} />
        <ColorField label="Text" value={p.text} onChange={f("text")} against={p.background} />
        <ColorField label="Muted text" value={p.muted} onChange={f("muted")} against={p.background} min={3} />
        <ColorField label="Headings" value={p.heading} onChange={f("heading")} against={p.background} />
      </div>
      {problems.length > 0 && <p className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive">{t("Hard to read — choose colours further apart:")} {problems.map((x) => t(x)).join(" · ")}</p>}
    </Block>
  );
}

const range = (a: number, b: number) => Array.from({ length: b - a + 1 }, (_, i) => a + i);

export function DesignTab({ c, set }: TabProps) {
  const g = c.products.grid;
  const k = c.categories;
  const modes = c.theme.modes;
  return (
    <div className="space-y-4">
      <Block title="Brand">
        <Grid2>
          <Field label="Website name"><Input value={c.brand.name} maxLength={60} onChange={(e) => set((x) => { x.brand.name = e.target.value; })} /></Field>
          <Field label="Tagline"><Input value={c.brand.tagline} maxLength={120} onChange={(e) => set((x) => { x.brand.tagline = e.target.value; })} /></Field>
        </Grid2>
        <MediaField label="Website logo" kind="logo" value={c.brand.logo} onChange={(id) => set((x) => { x.brand.logo = id; })} hint="Leave empty to use your business logo from Settings." />
      </Block>

      <Block title="Look & feel">
        <Field label="Theme"><Choice value={modes} onChange={(v) => set((x) => { x.theme.modes = v; })} options={[["light", "Light"], ["dark", "Dark"], ["both", "Both (visitors choose)"]]} /></Field>
        <Field label="Style"><Choice value={c.theme.style} onChange={(v) => set((x) => { x.theme.style = v; })} options={[["modern", "Modern"], ["minimal", "Minimal"], ["elegant", "Elegant"], ["bold", "Bold"]]} /></Field>
        <Field label="Spacing"><Choice value={c.theme.scale} onChange={(v) => set((x) => { x.theme.scale = v; })} options={[["compact", "Compact"], ["balanced", "Balanced"], ["spacious", "Spacious"]]} /></Field>
        <Grid2>
          <Field label="Heading font"><Select value={c.theme.heading_font} onChange={(v) => set((x) => { x.theme.heading_font = v; })}>{FONTS.map((f) => <option key={f} value={f}>{f}</option>)}</Select></Field>
          <Field label="Body font"><Select value={c.theme.body_font} onChange={(v) => set((x) => { x.theme.body_font = v; })}>{FONTS.map((f) => <option key={f} value={f}>{f}</option>)}</Select></Field>
        </Grid2>
      </Block>

      {modes !== "dark" && <PaletteEditor title="Light theme colours" p={c.theme.light} reset={LIGHT} onChange={(p) => set((x) => { x.theme.light = p; })} />}
      {modes !== "light" && <PaletteEditor title="Dark theme colours" p={c.theme.dark} reset={DARK} onChange={(p) => set((x) => { x.theme.dark = p; })} />}

      <Block title="Product cards" hint="How many per row on each screen, and how each card looks.">
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label="Phones"><Choice value={g.mobile} onChange={(v) => set((x) => { x.products.grid.mobile = v; })} options={range(1, 3).map((n) => [n, String(n)])} /></Field>
          <Field label="Tablets"><Choice value={g.tablet} onChange={(v) => set((x) => { x.products.grid.tablet = v; })} options={range(2, 4).map((n) => [n, String(n)])} /></Field>
          <Field label="Desktops"><Choice value={g.desktop} onChange={(v) => set((x) => { x.products.grid.desktop = v; })} options={range(3, 6).map((n) => [n, String(n)])} /></Field>
        </div>
        <Field label="Card size"><Choice value={g.card} onChange={(v) => set((x) => { x.products.grid.card = v; })} options={[["compact", "Compact"], ["standard", "Standard"], ["large", "Large"]]} /></Field>
        <Field label="Photo shape"><Choice value={g.ratio} onChange={(v) => set((x) => { x.products.grid.ratio = v; })} options={[["square", "Square"], ["portrait", "Portrait"], ["landscape", "Landscape"]]} /></Field>
        <Field label="Photo fit"><Choice value={g.fit} onChange={(v) => set((x) => { x.products.grid.fit = v; })} options={[["cover", "Fill the frame"], ["contain", "Show whole photo"]]} /></Field>
        <Field label="Corners"><Choice value={g.radius} onChange={(v) => set((x) => { x.products.grid.radius = v; })} options={[["none", "Square"], ["small", "Small"], ["medium", "Medium"], ["large", "Large"]]} /></Field>
        <Field label="Product name"><Choice value={g.name_lines} onChange={(v) => set((x) => { x.products.grid.name_lines = v; })} options={[[1, "1 line"], [2, "2 lines"]]} /></Field>
        <ToggleRow label="Card shadow" checked={g.shadow} onChange={(v) => set((x) => { x.products.grid.shadow = v; })} />
        <ToggleRow label="Badges (New, Offer…)" checked={g.show_badges} onChange={(v) => set((x) => { x.products.grid.show_badges = v; })} />
        <ToggleRow label="Quick add to cart" checked={g.quick_add} onChange={(v) => set((x) => { x.products.grid.quick_add = v; })} />
        <ToggleRow label="Show availability" hint="In stock / out of stock" checked={g.show_availability} onChange={(v) => set((x) => { x.products.grid.show_availability = v; })} />
      </Block>

      <Block title="Category display">
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label="Phones"><Choice value={k.mobile} onChange={(v) => set((x) => { x.categories.mobile = v; })} options={range(2, 4).map((n) => [n, String(n)])} /></Field>
          <Field label="Tablets"><Choice value={k.tablet} onChange={(v) => set((x) => { x.categories.tablet = v; })} options={range(3, 6).map((n) => [n, String(n)])} /></Field>
          <Field label="Desktops"><Choice value={k.desktop} onChange={(v) => set((x) => { x.categories.desktop = v; })} options={range(4, 8).map((n) => [n, String(n)])} /></Field>
        </div>
        <Field label="Card size"><Choice value={k.card} onChange={(v) => set((x) => { x.categories.card = v; })} options={[["compact", "Compact"], ["standard", "Standard"], ["large", "Large"]]} /></Field>
        <Field label="Layout"><Choice value={k.style} onChange={(v) => set((x) => { x.categories.style = v; })} options={[["carousel", "Carousel"], ["grid", "Grid"]]} /></Field>
        <ToggleRow label="Category images" checked={k.show_images} onChange={(v) => set((x) => { x.categories.show_images = v; })} />
      </Block>
    </div>
  );
}
