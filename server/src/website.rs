//! Website Add-On (roadmap 51–57): the configuration a business's public website is rendered from.
//!
//! S'Shop controls the architecture, components, responsiveness, accessibility and security; the business controls its
//! brand, content, sections, products presentation, services, testimonials, navigation, domain and SEO — through this
//! typed document, never raw HTML or CSS. A business edits a **draft**; publishing copies it to the **published**
//! version the public website serves (`websites.draft` / `websites.published`, numbered history for rollback).
//! Choices are bounded to options known to be responsive and readable (`validate`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{bad, AppResult};

// ───────────────────────────── Document ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SiteConfig {
    pub brand: Brand,
    pub theme: Theme,
    pub navigation: Vec<NavItem>,
    pub sections: Vec<Section>,
    pub hero: Hero,
    pub promotions: Vec<Promotion>,
    pub about: About,
    pub contact: Contact,
    pub social: Social,
    pub products: ProductsCfg,
    pub categories: CategoriesCfg,
    pub services: ServicesCfg,
    pub testimonials: TestimonialsCfg,
    pub seo: Seo,
    pub cookies: Cookies,
    pub footer: Footer,
    /// Roadmap 80: the page visitors see first at the website's root — home | products (default) | categories | services.
    pub landing: String,
}

/// Written out so that a default document and one read back from the database (missing fields filled in) are equal —
/// the activation step compares a stored draft with the default to know whether it is still blank.
impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            brand: Brand::default(),
            theme: Theme::default(),
            navigation: Vec::new(),
            sections: Vec::new(),
            hero: Hero::default(),
            promotions: Vec::new(),
            about: About::default(),
            contact: Contact::default(),
            social: Social::default(),
            products: ProductsCfg::default(),
            categories: CategoriesCfg::default(),
            services: ServicesCfg::default(),
            testimonials: TestimonialsCfg::default(),
            seo: Seo::default(),
            cookies: Cookies::default(),
            footer: Footer::default(),
            landing: "products".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Brand {
    pub name: String,
    pub tagline: String,
    /// Website-specific logo (media id); None = the business logo from Settings.
    pub logo: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Palette {
    pub primary: String,
    pub secondary: String,
    pub accent: String,
    pub background: String,
    pub surface: String,
    pub text: String,
    pub muted: String,
    pub heading: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self::light()
    }
}

impl Palette {
    pub fn light() -> Self {
        Self {
            primary: "#C2410C".into(),
            secondary: "#7C2D12".into(),
            accent: "#F59E0B".into(),
            background: "#FFFBF5".into(),
            surface: "#FFFFFF".into(),
            text: "#1C1917".into(),
            muted: "#57534E".into(),
            heading: "#1C1917".into(),
        }
    }
    pub fn dark() -> Self {
        Self {
            primary: "#FB923C".into(),
            secondary: "#FDBA74".into(),
            accent: "#FBBF24".into(),
            background: "#14110F".into(),
            surface: "#1F1A17".into(),
            text: "#F5F0EB".into(),
            muted: "#B9AFA6".into(),
            heading: "#FFFFFF".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Theme {
    pub light: Palette,
    pub dark: Palette,
    /// light | dark | both (visitors get a theme switch)
    pub modes: String,
    /// modern | minimal | elegant | bold
    pub style: String,
    pub heading_font: String,
    pub body_font: String,
    /// compact | balanced | spacious
    pub scale: String,
    /// Roadmap 77: off | subtle (default) | standard — always off for visitors who ask for reduced motion.
    pub motion: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            light: Palette::light(),
            dark: Palette::dark(),
            modes: "light".into(),
            style: "modern".into(),
            heading_font: "Outfit".into(),
            body_font: "Outfit".into(),
            scale: "balanced".into(),
            motion: "subtle".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct NavItem {
    /// home | about | products | categories | services | testimonials | contact | order
    pub key: String,
    pub label: String,
    pub visible: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Section {
    /// hero | categories | featured | new_arrivals | popular | promotions | about | services | testimonials | cta | contact
    pub key: String,
    pub visible: bool,
    pub heading: String,
    pub subheading: String,
    /// grid | carousel (product and category sections)
    pub layout: String,
    /// Products chosen for this section (featured / new arrivals / popular); empty = automatic.
    pub product_ids: Vec<Uuid>,
    pub cta_label: String,
    pub cta_target: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Cta {
    pub label: String,
    /// A page key (products, about, contact, order …) or an https:// / tel: / mailto: / WhatsApp link.
    pub target: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Hero {
    pub headline: String,
    pub text: String,
    pub image: Option<Uuid>,
    pub primary: Cta,
    pub secondary: Cta,
    /// left | center
    pub align: String,
    /// Darken the image behind the text so it stays readable.
    pub overlay: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Promotion {
    pub id: Uuid,
    pub title: String,
    pub text: String,
    pub image: Option<Uuid>,
    pub cta: Cta,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct About {
    pub intro: String,
    pub story: String,
    pub mission: String,
    pub vision: String,
    pub values: Vec<String>,
    pub image: Option<Uuid>,
    pub show_story: bool,
    pub show_mission: bool,
    pub show_vision: bool,
    pub show_values: bool,
    pub cta: Cta,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Contact {
    pub intro: String,
    pub phone: String,
    pub whatsapp: String,
    pub email: String,
    pub location: String,
    /// "Get directions" link (a maps URL); empty = built from the location.
    pub map_url: String,
    pub hours: String,
    pub show_phone: bool,
    pub show_whatsapp: bool,
    pub show_email: bool,
    pub show_location: bool,
    pub show_hours: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Social {
    pub instagram: String,
    pub facebook: String,
    pub tiktok: String,
    pub x: String,
    pub linkedin: String,
    pub youtube: String,
    pub whatsapp: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Grid {
    pub mobile: u8,
    pub tablet: u8,
    pub desktop: u8,
    /// compact | standard | large
    pub card: String,
    /// square | portrait | landscape
    pub ratio: String,
    /// cover | contain
    pub fit: String,
    /// none | small | medium | large
    pub radius: String,
    pub shadow: bool,
    /// 1 | 2
    pub name_lines: u8,
    pub show_badges: bool,
    pub quick_add: bool,
    pub show_availability: bool,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            mobile: 2,
            tablet: 3,
            desktop: 4,
            card: "standard".into(),
            // Roadmap 80: portrait 3:4 frames, whole product visible (perfumes, watches, jewellery).
            ratio: "portrait".into(),
            fit: "contain".into(),
            radius: "medium".into(),
            shadow: true,
            name_lines: 2,
            show_badges: true,
            quick_add: true,
            show_availability: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ProductCfg {
    pub product_id: Uuid,
    pub published: bool,
    pub featured: bool,
    pub sort: i32,
    pub marketing_name: String,
    pub marketing_description: String,
    /// "" | new | featured | offer
    pub badge: String,
    /// inherit | show | hide
    pub price: String,
    /// enquire | contact | whatsapp | order (when the price is hidden)
    pub hidden_action: String,
    /// Website photos follow the product's own photos.
    pub use_product_photos: bool,
    /// Website gallery (media ids) when `use_product_photos` is off.
    pub photos: Vec<Uuid>,
    /// Product photos left out of the website (when following the product's photos).
    pub hidden_photos: Vec<Uuid>,
    pub seo_title: String,
    pub seo_description: String,
    /// Website category placement (None = the product's category).
    pub category_id: Option<Uuid>,
    pub cta_label: String,
    /// Roadmap 80: the website's "was" price — shown struck through with the saving when above the current price.
    pub compare_at: Option<rust_decimal::Decimal>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ProductsCfg {
    pub items: Vec<ProductCfg>,
    pub grid: Grid,
    /// Website photos per product (1–5; the product photo limit is 5).
    pub max_photos: u8,
    /// Action for products whose price is hidden (unless the product sets its own).
    pub hidden_action: String,
    /// New S'Shop products appear on the website automatically (still subject to the publish step).
    pub auto_publish_new: bool,
    /// Roadmap 80: product lists in pages (on) or a "Load more" button (off) — never the whole catalogue at once.
    pub pagination: bool,
    /// Products per page (1–100), default 10.
    pub per_page: u16,
}

impl Default for ProductsCfg {
    fn default() -> Self {
        Self { items: Vec::new(), grid: Grid::default(), max_photos: 5, hidden_action: "enquire".into(), auto_publish_new: true, pagination: true, per_page: 10 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct CategoryCfg {
    pub category_id: Uuid,
    pub visible: bool,
    pub sort: i32,
    pub image: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct CategoriesCfg {
    pub items: Vec<CategoryCfg>,
    pub mobile: u8,
    pub tablet: u8,
    pub desktop: u8,
    /// compact | standard | large
    pub card: String,
    /// grid | carousel
    pub style: String,
    pub show_images: bool,
    /// How many categories the home page shows (0 = all).
    pub limit: u8,
}

impl Default for CategoriesCfg {
    fn default() -> Self {
        Self { items: Vec::new(), mobile: 3, tablet: 4, desktop: 6, card: "standard".into(), style: "carousel".into(), show_images: true, limit: 0 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Service {
    pub id: Uuid,
    pub name: String,
    /// A lucide icon name from the supported set, or empty.
    pub icon: String,
    pub image: Option<Uuid>,
    pub short: String,
    pub details: String,
    pub cta: Cta,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ServicesCfg {
    pub intro: String,
    pub items: Vec<Service>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Testimonial {
    pub id: Uuid,
    pub name: String,
    pub quote: String,
    /// 4 or 5 (0 = no rating shown)
    pub rating: u8,
    pub photo: Option<Uuid>,
    pub position: String,
    pub published: bool,
    /// Sample content shipped to show the design: never published until replaced or confirmed genuine.
    pub sample: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct TestimonialsCfg {
    pub show: bool,
    pub auto_scroll: bool,
    /// slow | normal | fast
    pub speed: String,
    pub items: Vec<Testimonial>,
}

impl Default for TestimonialsCfg {
    fn default() -> Self {
        Self { show: true, auto_scroll: true, speed: "normal".into(), items: Vec::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Seo {
    pub title: String,
    pub description: String,
    pub share_image: Option<Uuid>,
    pub business_description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Cookies {
    /// Optional analytics cookies ask for consent first; the shopping cart (necessary) always works.
    pub analytics: bool,
    pub privacy_policy: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Footer {
    pub description: String,
    pub policies: String,
    /// Optional "Powered by S'Shop" (off by default — the website is the business's own brand).
    pub show_attribution: bool,
}

// ───────────────────────────── Defaults ─────────────────────────────

pub const NAV_KEYS: [&str; 8] = ["home", "about", "products", "categories", "services", "testimonials", "contact", "order"];
pub const SECTION_KEYS: [&str; 11] =
    ["hero", "categories", "featured", "new_arrivals", "popular", "promotions", "about", "services", "testimonials", "cta", "contact"];
pub const FONTS: [&str; 5] = ["Outfit", "Poppins", "Inter", "Roboto", "Nunito"];

fn label(key: &str) -> &'static str {
    match key {
        "home" => "Home",
        "about" => "About Us",
        "products" => "Products",
        "categories" => "Categories",
        "services" => "Services",
        "testimonials" => "Testimonials",
        "contact" => "Contact",
        "order" => "Order",
        _ => "",
    }
}

fn section(key: &str, heading: &str, sub: &str, layout: &str, visible: bool) -> Section {
    Section { key: key.into(), visible, heading: heading.into(), subheading: sub.into(), layout: layout.into(), ..Default::default() }
}

/// The starting website for a business, from what S'Shop already knows about it.
pub struct Business<'a> {
    pub name: &'a str,
    pub tagline: &'a str,
    pub phone: &'a str,
    pub email: &'a str,
    pub address: &'a str,
}

pub fn starter(b: &Business) -> SiteConfig {
    let mut c = SiteConfig::default();
    c.brand = Brand { name: b.name.into(), tagline: b.tagline.into(), logo: None };
    c.navigation = NAV_KEYS
        .iter()
        .map(|k| NavItem { key: (*k).into(), label: label(k).into(), visible: !matches!(*k, "categories" | "testimonials") })
        .collect();
    c.sections = vec![
        section("hero", "", "", "", true),
        section("categories", "Top Categories", "", "carousel", true),
        section("featured", "Featured Products", "Hand-picked favourites", "grid", true),
        section("new_arrivals", "New Arrivals", "", "carousel", true),
        section("popular", "Most Popular", "", "carousel", false),
        section("promotions", "Offers", "", "", false),
        section("about", "About Us", "", "", true),
        section("services", "Our Services", "", "", false),
        section("testimonials", "What Our Customers Say", "", "", true),
        Section { cta_label: "Order Now".into(), cta_target: "order".into(), ..section("cta", "Find Something You Love", "", "", true) },
        section("contact", "Get in Touch", "", "", true),
    ];
    c.hero = Hero {
        headline: if b.tagline.is_empty() { format!("Welcome to {}", b.name) } else { b.tagline.into() },
        text: "Discover pieces selected to make every moment feel exceptional.".into(),
        image: None,
        primary: Cta { label: "Shop Now".into(), target: "products".into() },
        secondary: Cta { label: "Explore Products".into(), target: "categories".into() },
        align: "left".into(),
        overlay: true,
    };
    c.about = About {
        intro: format!("{} brings you quality you can count on, with friendly service every time.", b.name),
        show_story: true,
        show_mission: true,
        show_vision: false,
        show_values: false,
        cta: Cta { label: "Learn More".into(), target: "about".into() },
        ..Default::default()
    };
    c.contact = Contact {
        intro: "We would love to hear from you.".into(),
        phone: b.phone.into(),
        whatsapp: b.phone.into(),
        email: b.email.into(),
        location: b.address.into(),
        show_phone: !b.phone.is_empty(),
        show_whatsapp: !b.phone.is_empty(),
        show_email: !b.email.is_empty(),
        show_location: !b.address.is_empty(),
        show_hours: false,
        ..Default::default()
    };
    c.testimonials.items = SAMPLE_TESTIMONIALS
        .iter()
        .map(|(name, quote, rating, position)| Testimonial {
            id: Uuid::new_v4(),
            name: (*name).into(),
            quote: (*quote).into(),
            rating: *rating,
            photo: None,
            position: (*position).into(),
            published: false,
            sample: true,
        })
        .collect();
    c.seo = Seo { title: b.name.into(), description: b.tagline.into(), share_image: None, business_description: String::new() };
    c.footer = Footer { description: if b.tagline.is_empty() { b.name.into() } else { b.tagline.into() }, ..Default::default() };
    c
}

/// Five demonstration testimonials to show the carousel. They are marked as samples and are never shown publicly until
/// the business replaces them or confirms them genuine.
const SAMPLE_TESTIMONIALS: [(&str, &str, u8, &str); 5] = [
    ("Amina W.", "Beautiful products and such a smooth order — delivered the same day. I'm already planning my next purchase!", 5, "Nairobi"),
    ("Brian K.", "Great quality for the price and the team kept me updated the whole way. Highly recommended.", 5, "Westlands"),
    ("Cynthia M.", "I love how easy it is to find exactly what I need. The gift wrapping was a lovely touch.", 5, "Kilimani"),
    ("David O.", "Friendly service and genuine products. Ordering on my phone took less than two minutes.", 4, "Mombasa"),
    ("Esther N.", "My go-to shop now — consistent quality and they always respond quickly on WhatsApp.", 5, "Kisumu"),
];

// ───────────────────────────── Validation ─────────────────────────────

fn hex_rgb(s: &str) -> Option<(f64, f64, f64)> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|x| x as f64 / 255.0);
    Some((v(0)?, v(2)?, v(4)?))
}

fn luminance(c: (f64, f64, f64)) -> f64 {
    let ch = |x: f64| if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
    0.2126 * ch(c.0) + 0.7152 * ch(c.1) + 0.0722 * ch(c.2)
}

/// WCAG contrast ratio between two #RRGGBB colours (1–21).
pub fn contrast(a: &str, b: &str) -> f64 {
    match (hex_rgb(a), hex_rgb(b)) {
        (Some(x), Some(y)) => {
            let (l1, l2) = (luminance(x), luminance(y));
            let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            (hi + 0.05) / (lo + 0.05)
        }
        _ => 1.0,
    }
}

/// Text colour for a filled button in `bg` (white or near-black, whichever reads better).
pub fn on_color(bg: &str) -> &'static str {
    if contrast("#FFFFFF", bg) >= contrast("#111111", bg) { "#FFFFFF" } else { "#111111" }
}

fn check_palette(name: &str, p: &Palette) -> AppResult<()> {
    for (field, v) in [
        ("primary", &p.primary), ("secondary", &p.secondary), ("accent", &p.accent), ("background", &p.background),
        ("surface", &p.surface), ("text", &p.text), ("muted text", &p.muted), ("heading", &p.heading),
    ] {
        if hex_rgb(v).is_none() {
            return Err(bad(format!("{name} theme: {field} must be a colour like #1C1917")));
        }
    }
    // Readability is not optional: text must stay legible on the background and on cards.
    for (what, fg, bg, min) in [
        ("Body text on the background", p.text.as_str(), p.background.as_str(), 4.5),
        ("Body text on cards", p.text.as_str(), p.surface.as_str(), 4.5),
        ("Headings on the background", p.heading.as_str(), p.background.as_str(), 4.5),
        ("Muted text on the background", p.muted.as_str(), p.background.as_str(), 3.0),
        ("Buttons", on_color(&p.primary), p.primary.as_str(), 3.0),
        ("Primary colour on the background", p.primary.as_str(), p.background.as_str(), 2.0),
    ] {
        let r = contrast(fg, bg);
        if r < min {
            return Err(bad(format!("{name} theme: {what} is hard to read (contrast {r:.1}:1, needs {min}:1) — choose colours further apart")));
        }
    }
    Ok(())
}

fn one_of(v: &str, allowed: &[&str], what: &str) -> AppResult<()> {
    if allowed.contains(&v) { Ok(()) } else { Err(bad(format!("{what}: choose one of {}", allowed.join(", ")))) }
}

fn max_len(v: &str, n: usize, what: &str) -> AppResult<()> {
    if v.chars().count() > n { Err(bad(format!("{what} is too long (max {n} characters)"))) } else { Ok(()) }
}

pub fn link_ok(target: &str) -> bool {
    let t = target.trim();
    t.is_empty()
        || NAV_KEYS.contains(&t)
        || t.starts_with("https://")
        || t.starts_with("tel:")
        || t.starts_with("mailto:")
        || t.starts_with("/products/")
        || t.starts_with("/categories/")
}

fn check_cta(c: &Cta, what: &str) -> AppResult<()> {
    max_len(&c.label, 40, what)?;
    if !link_ok(&c.target) {
        return Err(bad(format!("{what}: the link must be a page (products, about, contact, order …) or start with https://, tel: or mailto:")));
    }
    Ok(())
}

/// Bounded choices only, so no configuration can break the layout, readability or security of the website.
pub fn validate(c: &SiteConfig) -> AppResult<()> {
    max_len(&c.brand.name, 80, "Business name")?;
    max_len(&c.brand.tagline, 160, "Tagline")?;
    check_palette("Light", &c.theme.light)?;
    check_palette("Dark", &c.theme.dark)?;
    one_of(&c.theme.modes, &["light", "dark", "both"], "Themes")?;
    one_of(&c.theme.style, &["modern", "minimal", "elegant", "bold"], "Style")?;
    one_of(&c.theme.scale, &["compact", "balanced", "spacious"], "Typography scale")?;
    one_of(&c.theme.motion, &["off", "subtle", "standard"], "Animations")?;
    one_of(&c.theme.heading_font, &FONTS, "Heading font")?;
    one_of(&c.theme.body_font, &FONTS, "Body font")?;

    let mut seen = Vec::new();
    for n in &c.navigation {
        one_of(&n.key, &NAV_KEYS, "Navigation item")?;
        if seen.contains(&n.key) {
            return Err(bad("Each navigation item can appear once"));
        }
        seen.push(n.key.clone());
        max_len(&n.label, 24, "Navigation label")?;
    }
    let mut seen = Vec::new();
    for s in &c.sections {
        one_of(&s.key, &SECTION_KEYS, "Section")?;
        if seen.contains(&s.key) {
            return Err(bad("Each section can appear once"));
        }
        seen.push(s.key.clone());
        max_len(&s.heading, 80, "Section heading")?;
        max_len(&s.subheading, 200, "Section text")?;
        if !s.layout.is_empty() {
            one_of(&s.layout, &["grid", "carousel"], "Section layout")?;
        }
        if s.product_ids.len() > 24 {
            return Err(bad("A section can feature up to 24 products"));
        }
        check_cta(&Cta { label: s.cta_label.clone(), target: s.cta_target.clone() }, "Section button")?;
    }
    max_len(&c.hero.headline, 90, "Hero headline")?;
    max_len(&c.hero.text, 240, "Hero text")?;
    check_cta(&c.hero.primary, "Hero primary button")?;
    check_cta(&c.hero.secondary, "Hero secondary button")?;
    if !c.hero.align.is_empty() {
        one_of(&c.hero.align, &["left", "center"], "Hero alignment")?;
    }
    if c.promotions.len() > 10 {
        return Err(bad("Up to 10 promotions"));
    }
    for p in &c.promotions {
        max_len(&p.title, 80, "Promotion title")?;
        max_len(&p.text, 240, "Promotion text")?;
        check_cta(&p.cta, "Promotion button")?;
    }
    for (v, n, what) in [
        (&c.about.intro, 600, "About introduction"), (&c.about.story, 4000, "Story"), (&c.about.mission, 1000, "Mission"),
        (&c.about.vision, 1000, "Vision"), (&c.contact.intro, 400, "Contact introduction"), (&c.contact.hours, 300, "Business hours"),
        (&c.footer.description, 400, "Footer description"), (&c.footer.policies, 6000, "Policies"),
        (&c.cookies.privacy_policy, 12000, "Privacy policy"), (&c.seo.title, 70, "Website title"),
        (&c.seo.description, 170, "Meta description"), (&c.seo.business_description, 600, "Business description"),
        (&c.services.intro, 400, "Services introduction"),
    ] {
        max_len(v, n, what)?;
    }
    if c.about.values.len() > 8 {
        return Err(bad("Up to 8 values"));
    }
    check_cta(&c.about.cta, "About button")?;
    for (v, what) in [
        (&c.contact.map_url, "Directions link"), (&c.social.instagram, "Instagram"), (&c.social.facebook, "Facebook"),
        (&c.social.tiktok, "TikTok"), (&c.social.x, "X"), (&c.social.linkedin, "LinkedIn"), (&c.social.youtube, "YouTube"),
    ] {
        if !v.is_empty() && !v.starts_with("https://") {
            return Err(bad(format!("{what}: use a full link starting with https://")));
        }
        max_len(v, 300, what)?;
    }

    let g = &c.products.grid;
    if !(1..=3).contains(&g.mobile) || !(2..=4).contains(&g.tablet) || !(3..=6).contains(&g.desktop) {
        return Err(bad("Products per row: mobile 1–3, tablet 2–4, desktop 3–6"));
    }
    one_of(&g.card, &["compact", "standard", "large"], "Product card size")?;
    one_of(&g.ratio, &["square", "portrait", "landscape"], "Image ratio")?;
    one_of(&g.fit, &["cover", "contain"], "Image fit")?;
    one_of(&g.radius, &["none", "small", "medium", "large"], "Card corners")?;
    if !(1..=2).contains(&g.name_lines) {
        return Err(bad("Product name lines: 1 or 2"));
    }
    if !(1..=5).contains(&c.products.max_photos) {
        return Err(bad("Website photos per product: 1–5"));
    }
    one_of(&c.products.hidden_action, &["enquire", "contact", "whatsapp", "order"], "Action when the price is hidden")?;
    if !(1..=100).contains(&c.products.per_page) {
        return Err(bad("Products per page must be between 1 and 100"));
    }
    one_of(&c.landing, &["home", "products", "categories", "services"], "Default landing page")?;
    if c.products.items.len() > 5000 {
        return Err(bad("Too many products"));
    }
    for p in &c.products.items {
        max_len(&p.marketing_name, 120, "Marketing name")?;
        max_len(&p.marketing_description, 4000, "Marketing description")?;
        if !p.badge.is_empty() {
            one_of(&p.badge, &["new", "featured", "offer"], "Badge")?;
        }
        if p.compare_at.is_some_and(|v| v < rust_decimal::Decimal::ZERO || v > rust_decimal::Decimal::from(1_000_000_000)) {
            return Err(bad("The \"was\" price must be a positive amount"));
        }
        one_of(if p.price.is_empty() { "inherit" } else { &p.price }, &["inherit", "show", "hide"], "Price visibility")?;
        if !p.hidden_action.is_empty() {
            one_of(&p.hidden_action, &["enquire", "contact", "whatsapp", "order"], "Action when the price is hidden")?;
        }
        if p.photos.len() > c.products.max_photos as usize {
            return Err(bad(format!("Up to {} website photos per product", c.products.max_photos)));
        }
        max_len(&p.seo_title, 70, "Product SEO title")?;
        max_len(&p.seo_description, 170, "Product SEO description")?;
        max_len(&p.cta_label, 30, "Product button")?;
    }
    let cg = &c.categories;
    if !(1..=4).contains(&cg.mobile) || !(2..=6).contains(&cg.tablet) || !(3..=8).contains(&cg.desktop) {
        return Err(bad("Categories per row: mobile 1–4, tablet 2–6, desktop 3–8"));
    }
    one_of(&cg.card, &["compact", "standard", "large"], "Category card size")?;
    one_of(&cg.style, &["grid", "carousel"], "Category display")?;

    if c.services.items.len() > 30 {
        return Err(bad("Up to 30 services"));
    }
    for s in &c.services.items {
        if s.name.trim().is_empty() {
            return Err(bad("Each service needs a name"));
        }
        max_len(&s.name, 80, "Service name")?;
        max_len(&s.short, 240, "Service summary")?;
        max_len(&s.details, 3000, "Service details")?;
        max_len(&s.icon, 40, "Service icon")?;
        check_cta(&s.cta, "Service button")?;
    }
    let t = &c.testimonials;
    one_of(&t.speed, &["slow", "normal", "fast"], "Testimonial speed")?;
    if t.items.len() > 50 {
        return Err(bad("Up to 50 testimonials"));
    }
    for x in &t.items {
        if x.name.trim().is_empty() || x.quote.trim().is_empty() {
            return Err(bad("Each testimonial needs a name and a quote"));
        }
        max_len(&x.name, 80, "Testimonial name")?;
        max_len(&x.quote, 600, "Testimonial")?;
        max_len(&x.position, 80, "Testimonial title / location")?;
        if !matches!(x.rating, 0 | 4 | 5) {
            return Err(bad("Testimonial rating: 4 or 5 stars (or none)"));
        }
    }
    Ok(())
}

/// Media ids a configuration refers to (to check they belong to the business).
pub fn media_ids(c: &SiteConfig) -> Vec<Uuid> {
    let mut v: Vec<Uuid> = Vec::new();
    v.extend(c.brand.logo);
    v.extend(c.hero.image);
    v.extend(c.about.image);
    v.extend(c.seo.share_image);
    v.extend(c.promotions.iter().filter_map(|p| p.image));
    v.extend(c.services.items.iter().filter_map(|s| s.image));
    v.extend(c.testimonials.items.iter().filter_map(|t| t.photo));
    v.extend(c.categories.items.iter().filter_map(|x| x.image));
    for p in &c.products.items {
        v.extend(p.photos.iter().copied());
    }
    v.sort();
    v.dedup();
    v
}

/// Which permission each top-level part of the configuration needs to change (users may edit some parts only).
pub fn permission_for(part: &str) -> &'static str {
    match part {
        "brand" | "theme" => "website.design",
        "navigation" => "website.navigation",
        "products" => "website.products",
        "categories" => "website.categories",
        "services" => "website.services",
        "testimonials" => "website.testimonials",
        "seo" => "website.seo",
        _ => "website.content", // sections, hero, promotions, about, contact, social, cookies, footer
    }
}

/// Top-level parts that differ between two configurations.
pub fn changed_parts(a: &SiteConfig, b: &SiteConfig) -> Vec<String> {
    let (x, y) = (serde_json::to_value(a).unwrap_or_default(), serde_json::to_value(b).unwrap_or_default());
    let mut out = Vec::new();
    if let (Some(x), Some(y)) = (x.as_object(), y.as_object()) {
        for (k, v) in y {
            if x.get(k) != Some(v) {
                out.push(k.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn biz() -> Business<'static> {
        Business { name: "Pablo Niche", tagline: "Shop the Difference.", phone: "0798993404", email: "hi@example.com", address: "Nairobi" }
    }

    #[test]
    fn starter_is_valid_and_samples_are_never_public() {
        let c = starter(&biz());
        validate(&c).unwrap();
        assert_eq!(c.testimonials.items.len(), 5);
        assert!(c.testimonials.items.iter().all(|t| t.sample && !t.published));
        assert_eq!(c.products.max_photos, 5);
        // Roadmap 80: products first, portrait whole-product photos, 10 per page.
        assert_eq!(c.landing, "products");
        assert_eq!((c.products.grid.ratio.as_str(), c.products.grid.fit.as_str()), ("portrait", "contain"));
        assert!(c.products.pagination && c.products.per_page == 10);
        let old: SiteConfig = serde_json::from_value(serde_json::json!({ "brand": { "name": "Old" } })).unwrap();
        assert_eq!(old.landing, "products", "saved websites without the field land on Products");
        let blank: SiteConfig = serde_json::from_value(serde_json::to_value(SiteConfig::default()).unwrap()).unwrap();
        assert_eq!(blank, SiteConfig::default(), "a stored blank draft is still recognised as blank");
        assert_eq!(c.products.grid.mobile, 2);
    }

    #[test]
    fn unreadable_colours_refused() {
        let mut c = starter(&biz());
        c.theme.light.text = "#FAFAFA".into();
        assert!(validate(&c).is_err());
        let mut c = starter(&biz());
        c.theme.light.background = "#zzzzzz".into();
        assert!(validate(&c).is_err());
    }

    #[test]
    fn contrast_and_on_color() {
        assert!((contrast("#000000", "#FFFFFF") - 21.0).abs() < 0.01);
        assert_eq!(on_color("#C2410C"), "#FFFFFF");
        assert_eq!(on_color("#FDE68A"), "#111111");
    }

    #[test]
    fn bounded_choices_and_links() {
        let mut c = starter(&biz());
        c.products.grid.mobile = 4;
        assert!(validate(&c).is_err());
        let mut c = starter(&biz());
        c.hero.primary.target = "javascript:alert(1)".into();
        assert!(validate(&c).is_err());
        let mut c = starter(&biz());
        c.social.instagram = "http://insecure".into();
        assert!(validate(&c).is_err());
    }

    #[test]
    fn parts_and_permissions() {
        let a = starter(&biz());
        let mut b = a.clone();
        b.theme.style = "bold".into();
        b.hero.headline = "New".into();
        let parts = changed_parts(&a, &b);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().any(|p| permission_for(p) == "website.design"));
        assert!(parts.iter().any(|p| permission_for(p) == "website.content"));
    }
}

// ───────────────────────────── Images (roadmap 55) ─────────────────────────────

/// The real format of an uploaded image (never the browser's word).
pub fn sniff(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Pixel size read from the image header (JPEG SOF, PNG IHDR, WebP VP8 / VP8L / VP8X).
pub fn dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let be16 = |i: usize| data.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as u32);
    let be32 = |i: usize| data.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le16 = |i: usize| data.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32);
    let le24 = |i: usize| data.get(i..i + 3).map(|b| b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16);
    match sniff(data)? {
        "image/png" => Some((be32(16)?, be32(20)?)),
        "image/webp" => match data.get(12..16)? {
            b"VP8 " => Some((le16(26)? & 0x3FFF, le16(28)? & 0x3FFF)),
            b"VP8L" => {
                let b = data.get(21..25)?;
                let w = 1 + (((b[1] as u32 & 0x3F) << 8) | b[0] as u32);
                let h = 1 + (((b[3] as u32 & 0x0F) << 10) | ((b[2] as u32) << 2) | ((b[1] as u32 & 0xC0) >> 6));
                Some((w, h))
            }
            b"VP8X" => Some((1 + le24(24)?, 1 + le24(27)?)),
            _ => None,
        },
        _ => {
            // JPEG: walk the segments to the frame header (SOF0–SOF15 except DHT/JPG/DAC).
            let mut i = 2;
            while i + 9 < data.len() {
                if data[i] != 0xFF {
                    return None;
                }
                let marker = data[i + 1];
                if marker == 0xFF {
                    i += 1;
                    continue;
                }
                let len = be16(i + 2)? as usize;
                if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    return Some((be16(i + 7)?, be16(i + 5)?));
                }
                i += 2 + len;
            }
            None
        }
    }
}

/// Website image checks: ✕ refused (returned as Err), ⚠ warnings (stored with the image, shown before publishing).
/// Usable photographs are never refused just for not being ideal.
pub fn assess(kind: &str, data: &[u8], blurry: bool) -> Result<(&'static str, u32, u32, Vec<String>), String> {
    let mime = sniff(data).ok_or("Only JPEG, PNG or WebP images can be used")?;
    let (w, h) = dimensions(data).ok_or("The image could not be read")?;
    if w.min(h) < 200 {
        return Err(format!("The image is too small ({w}×{h}) — use one at least 200 pixels on each side"));
    }
    if w.max(h) > 6000 {
        return Err("The image is too large to display — use one under 6000 pixels".into());
    }
    let mut warn = Vec::new();
    let (min_side, wide_ok) = match kind {
        "banner" | "promotion" | "about" => (800, true),
        "logo" => (200, true),
        _ => (600, false),
    };
    if w.min(h) < min_side {
        warn.push(format!("Low resolution ({w}×{h}) — may look soft on large screens"));
    }
    let ratio = w as f64 / h as f64;
    if !wide_ok && !(1.0 / 2.5..=2.5).contains(&ratio) {
        warn.push("Unusual proportions — the picture will be cropped in product cards".into());
    }
    if kind == "banner" && !(1.2..=3.5).contains(&ratio) {
        warn.push("Banners look best in landscape (about 16:9) — the important part may be cropped".into());
    }
    let bytes_per_pixel = data.len() as f64 / (w as f64 * h as f64);
    if mime != "image/png" && bytes_per_pixel < 0.02 {
        warn.push("Heavily compressed — details may look blocky".into());
    }
    if blurry {
        warn.push("Looks blurry or out of focus".into());
    }
    Ok((mime, w, h, warn))
}

#[cfg(test)]
mod image_tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v
    }

    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08];
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[0u8; 12]);
        v
    }

    #[test]
    fn reads_sizes() {
        assert_eq!(dimensions(&png(1200, 800)), Some((1200, 800)));
        assert_eq!(dimensions(&jpeg(1600, 900)), Some((1600, 900)));
        assert_eq!(dimensions(b"not an image"), None);
    }

    #[test]
    fn quality_levels() {
        assert!(assess("product", &png(150, 150), false).is_err());
        let (_, _, _, w) = assess("product", &png(1200, 1200), false).unwrap();
        assert!(w.is_empty());
        let (_, _, _, w) = assess("product", &png(500, 1600), true).unwrap();
        assert_eq!(w.len(), 3); // low resolution, proportions, blurry
        let (_, _, _, w) = assess("banner", &png(1600, 1600), false).unwrap();
        assert!(w.iter().any(|x| x.contains("landscape")));
    }
}
