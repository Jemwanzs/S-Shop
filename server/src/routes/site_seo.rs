//! What search engines and link previews read in a business's website before any JavaScript runs (roadmap 156).
//!
//! The website stays a React app; the least disruptive way to make it crawlable is for the server to place the page's real
//! content in the first HTML, using the same published data the app shows. React replaces it when it starts, so people and
//! crawlers read the same content. No new framework, and no separate pages for search engines.
//!
//! Everything here comes from one business's published website and catalogue. Nothing is assumed about its industry or
//! location, and nothing is invented: a price, availability or contact detail the website does not show is left out.

use serde_json::{json, Value};
use uuid::Uuid;

use super::site::PubProduct;
use crate::website::SiteConfig;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

pub fn clip(s: &str, n: usize) -> String {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= n { t } else { format!("{}…", t.chars().take(n - 1).collect::<String>()) }
}

/// An ISO 4217 code for Schema.org, from the business's currency as it is written. An unknown one gives none: then no
/// `Offer` is published, because a price with a guessed currency would be wrong.
pub fn currency_code(currency: &str) -> Option<String> {
    let c = currency.trim();
    let code = match c.to_lowercase().as_str() {
        "ksh" | "kshs" | "ksh." | "kes." => "KES",
        "ush" | "ushs" => "UGX",
        "tsh" | "tshs" => "TZS",
        "$" | "us$" => "USD",
        "€" => "EUR",
        "£" => "GBP",
        "r" => "ZAR",
        "₦" => "NGN",
        "₵" | "gh₵" => "GHS",
        "frw" | "rf" => "RWF",
        "₹" => "INR",
        // Already a code (`KES`, `usd`): one written in mixed case is a local abbreviation, not a code.
        _ if c.len() == 3 && (c.chars().all(|x| x.is_ascii_uppercase()) || c.chars().all(|x| x.is_ascii_lowercase())) => return Some(c.to_ascii_uppercase()),
        _ => return None,
    };
    Some(code.to_string())
}

/// A place customers can visit, as the website shows it (roadmap 157).
#[derive(serde::Serialize, Clone)]
pub struct Location {
    pub name: String,
    pub street_address: String,
    pub city: String,
    pub region: String,
    pub country: String,
    pub phone: String,
    pub maps_url: String,
    #[serde(skip)]
    pub hours: Option<crate::settings::Hours>,
}

/// A category as the public website shows it.
#[derive(Clone)]
pub struct Cat {
    pub id: Uuid,
    pub name: String,
}

/// What a page needs from its website.
pub struct Ctx<'a> {
    /// Where the website lives, with no trailing slash (custom domain, or `/s/{slug}` on the S'Shop host).
    pub root: &'a str,
    pub brand: &'a str,
    pub currency: &'a str,
    pub config: &'a SiteConfig,
    pub cats: &'a [Cat],
    pub logo: Option<String>,
    pub places: &'a [Location],
    /// Live articles this website has (roadmap 158).
    pub articles: i64,
}

/// Schema.org opening hours from trading days (Monday first). Hours never set (open all day, every day, the default) say nothing.
fn opening_hours(h: &crate::settings::Hours) -> Option<Value> {
    const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    if h.open == h.close {
        return None;
    }
    let days: Vec<&str> = DAYS.iter().zip(h.days.iter()).filter(|(_, on)| **on).map(|(d, _)| *d).collect();
    Some(json!({ "@type": "OpeningHoursSpecification", "dayOfWeek": days, "opens": h.open, "closes": h.close }))
}

fn postal(street: &str, city: &str, region: &str, country: &str, code: &str) -> Value {
    let mut a = json!({ "@type": "PostalAddress" });
    for (key, value) in [("streetAddress", street), ("addressLocality", city), ("addressRegion", region), ("addressCountry", country), ("postalCode", code)] {
        if !value.trim().is_empty() {
            a[key] = json!(value.trim());
        }
    }
    a
}

impl Ctx<'_> {
    fn href(&self, path: &str) -> String {
        format!("{}{}", self.root, path)
    }

    fn money(&self, p: &PubProduct) -> Option<String> {
        p.price.map(|v| format!("{} {}", self.currency, v.round_dp(2).normalize()))
    }

    fn nav(&self) -> String {
        let mut links = vec![format!("<a href=\"{}\">{}</a>", esc(&self.href("/")), esc(self.brand))];
        for n in &self.config.navigation {
            let path = match n.key.as_str() {
                "about" => "/about",
                "products" => "/products",
                "categories" => "/categories",
                "services" => "/services",
                "testimonials" => "/testimonials",
                "contact" => "/contact",
                _ => continue,
            };
            if n.visible {
                links.push(format!("<a href=\"{}\">{}</a>", esc(&self.href(path)), esc(&n.label)));
            }
        }
        if self.articles > 0 {
            links.push(format!("<a href=\"{}\">Articles</a>", esc(&self.href("/articles"))));
        }
        format!("<nav aria-label=\"Main\">{}</nav>", links.join(" "))
    }

    fn footer(&self) -> String {
        let c = &self.config.contact;
        let mut lines = Vec::new();
        if c.show_location && !c.location.trim().is_empty() {
            lines.push(esc(c.location.trim()));
        }
        if c.show_phone && !c.phone.trim().is_empty() {
            lines.push(esc(c.phone.trim()));
        }
        if c.show_email && !c.email.trim().is_empty() {
            lines.push(esc(c.email.trim()));
        }
        if c.show_hours && !c.hours.trim().is_empty() {
            lines.push(esc(c.hours.trim()));
        }
        format!("<footer><p>{}</p><p>{}</p></footer>", esc(self.brand), lines.join(" · "))
    }

    fn card(&self, p: &PubProduct) -> String {
        let img = p.photo_thumb.as_deref().or(p.photo.as_deref()).map(|u| format!("<img src=\"{}\" alt=\"{}\" loading=\"lazy\" decoding=\"async\" />", esc(&absolute(self.root, u)), esc(&p.name))).unwrap_or_default();
        let price = self.money(p).map(|m| format!("<p>{}</p>", esc(&m))).or_else(|| p.price_note.as_ref().map(|n| format!("<p>{}</p>", esc(n)))).unwrap_or_default();
        format!("<li><a href=\"{}\">{img}<h3>{}</h3></a>{price}</li>", esc(&self.href(&format!("/products/{}", p.slug))), esc(&p.name))
    }

    fn list(&self, items: &[&PubProduct]) -> String {
        if items.is_empty() {
            return String::new();
        }
        format!("<ul>{}</ul>", items.iter().map(|p| self.card(p)).collect::<String>())
    }

    fn pager(&self, base: &str, page: usize, last: usize) -> String {
        if last <= 1 {
            return String::new();
        }
        let link = |n: usize, label: &str, rel: &str| {
            let q = if n > 1 { format!("?page={n}") } else { String::new() };
            format!("<a rel=\"{rel}\" href=\"{}\">{}</a>", esc(&format!("{}{q}", self.href(base))), label)
        };
        let mut out = Vec::new();
        if page > 1 {
            out.push(link(page - 1, "Previous", "prev"));
        }
        if page < last {
            out.push(link(page + 1, "Next", "next"));
        }
        format!("<nav aria-label=\"Pages\">{} <span>{page} / {last}</span></nav>", out.join(" "))
    }

    fn wrap(&self, main: String) -> String {
        format!("<div class=\"seed\">{}<main>{main}</main>{}</div>", self.nav(), self.footer())
    }

    pub fn product_page(&self, p: &PubProduct) -> String {
        let mut crumbs = vec![format!("<a href=\"{}\">{}</a>", esc(&self.href("/")), esc(self.brand)), format!("<a href=\"{}\">Products</a>", esc(&self.href("/products")))];
        if let (Some(id), Some(name)) = (p.category_id, &p.category_name) {
            crumbs.push(format!("<a href=\"{}\">{}</a>", esc(&self.href(&format!("/categories/{id}"))), esc(name)));
        }
        let photos: String = p.photos.iter().enumerate().map(|(i, u)| {
            let alt = if i == 0 { p.name.clone() } else { format!("{} — photo {}", p.name, i + 1) };
            format!("<img src=\"{}\" alt=\"{}\"{} />", esc(&absolute(self.root, u)), esc(&alt), if i == 0 { "" } else { " loading=\"lazy\"" })
        }).collect();
        let price = self.money(p).map(|m| format!("<p>{}</p>", esc(&m))).or_else(|| p.price_note.as_ref().map(|n| format!("<p>{}</p>", esc(n)))).unwrap_or_default();
        let stock = p.in_stock.map(|s| format!("<p>{}</p>", if s { "In stock" } else { "Out of stock" })).unwrap_or_default();
        let description = if p.description.trim().is_empty() { String::new() } else { format!("<p>{}</p>", esc(p.description.trim())) };
        self.wrap(format!("<nav aria-label=\"Breadcrumb\">{}</nav><h1>{}</h1>{photos}{price}{stock}{description}", crumbs.join(" / "), esc(&p.name)))
    }

    pub fn products_page(&self, title: &str, base: &str, items: &[&PubProduct], page: usize, last: usize) -> String {
        self.wrap(format!("<h1>{}</h1>{}{}", esc(title), self.list(items), self.pager(base, page, last)))
    }

    pub fn categories_page(&self) -> String {
        let links: String = self.cats.iter().map(|c| format!("<li><a href=\"{}\">{}</a></li>", esc(&self.href(&format!("/categories/{}", c.id))), esc(&c.name))).collect();
        self.wrap(format!("<h1>Categories</h1><ul>{links}</ul>"))
    }

    pub fn home_page(&self, featured: &[&PubProduct]) -> String {
        let c = self.config;
        let headline = if c.hero.headline.trim().is_empty() { self.brand } else { c.hero.headline.trim() };
        let intro = [c.hero.text.as_str(), c.brand.tagline.as_str()].into_iter().find(|x| !x.trim().is_empty()).map(|x| format!("<p>{}</p>", esc(x.trim()))).unwrap_or_default();
        let cats = if self.cats.is_empty() {
            String::new()
        } else {
            let links: String = self.cats.iter().map(|x| format!("<li><a href=\"{}\">{}</a></li>", esc(&self.href(&format!("/categories/{}", x.id))), esc(&x.name))).collect();
            format!("<section><h2>Categories</h2><ul>{links}</ul></section>")
        };
        let list = if featured.is_empty() { String::new() } else { format!("<section><h2>Products</h2>{}</section>", self.list(featured)) };
        self.wrap(format!("<h1>{}</h1>{intro}{cats}{list}", esc(headline)))
    }

    pub fn about_page(&self) -> String {
        let a = &self.config.about;
        let mut body = String::new();
        for (show, heading, text) in [(true, "", a.intro.as_str()), (a.show_story, "Our story", a.story.as_str()), (a.show_mission, "Mission", a.mission.as_str()), (a.show_vision, "Vision", a.vision.as_str())] {
            if show && !text.trim().is_empty() {
                if !heading.is_empty() {
                    body.push_str(&format!("<h2>{heading}</h2>"));
                }
                body.push_str(&format!("<p>{}</p>", esc(text.trim())));
            }
        }
        self.wrap(format!("<h1>About {}</h1>{body}", esc(self.brand)))
    }

    pub fn services_page(&self) -> String {
        let items: String = self.config.services.items.iter().filter(|s| s.active).map(|s| format!("<li><h2>{}</h2><p>{}</p><p>{}</p></li>", esc(&s.name), esc(&s.short), esc(&s.details))).collect();
        self.wrap(format!("<h1>Services</h1><p>{}</p><ul>{items}</ul>", esc(&self.config.services.intro)))
    }

    pub fn contact_page(&self) -> String {
        let places: String = self.places.iter().map(|l| {
            let line = [l.street_address.as_str(), l.city.as_str(), l.region.as_str(), l.country.as_str()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", ");
            let phone = if l.phone.is_empty() { String::new() } else { format!("<p>{}</p>", esc(&l.phone)) };
            format!("<li><h2>{}</h2><p>{}</p>{phone}</li>", esc(&l.name), esc(&line))
        }).collect();
        let places = if places.is_empty() { places } else { format!("<ul>{places}</ul>") };
        self.wrap(format!("<h1>Contact {}</h1><p>{}</p>{places}", esc(self.brand), esc(&self.config.contact.intro)))
    }

    pub fn testimonials_page(&self) -> String {
        let items: String = shown_testimonials(self.config).map(|t| format!("<li><blockquote>{}</blockquote><p>{}</p></li>", esc(&t.quote), esc(&t.name))).collect();
        self.wrap(format!("<h1>Testimonials</h1><ul>{items}</ul>"))
    }

    pub fn articles_page(&self, rows: &[super::articles::ArticleRow], page: usize, last: usize) -> String {
        let items: String = rows.iter().map(|a| {
            let img = a.cover_media.map(|m| format!("<img src=\"{}\" alt=\"{}\" loading=\"lazy\" />", esc(&absolute(self.root, &format!("{}?size=thumb", super::site::media_url(m)))), esc(&a.cover_alt))).unwrap_or_default();
            format!("<li><a href=\"{}\">{img}<h2>{}</h2></a><p>{}</p></li>", esc(&self.href(&format!("/articles/{}", a.slug))), esc(&a.title), esc(&a.description))
        }).collect();
        self.wrap(format!("<h1>Articles</h1><ul>{items}</ul>{}", self.pager("/articles", page, last)))
    }

    pub fn article_page(&self, a: &super::articles::ArticleRow) -> String {
        let crumbs = format!("<a href=\"{}\">{}</a> / <a href=\"{}\">Articles</a>", esc(&self.href("/")), esc(self.brand), esc(&self.href("/articles")));
        let cover = a.cover_media.map(|m| format!("<img src=\"{}\" alt=\"{}\" />", esc(&absolute(self.root, &super::site::media_url(m))), esc(&a.cover_alt))).unwrap_or_default();
        let by = if a.author_name.trim().is_empty() { String::new() } else { format!("<p>{}</p>", esc(a.author_name.trim())) };
        self.wrap(format!("<nav aria-label=\"Breadcrumb\">{crumbs}</nav><article><h1>{}</h1>{by}{cover}{}</article>", esc(&a.title), super::articles::render_body(&a.body, self.root)))
    }

    /// `Article`, with only what the page shows.
    pub fn article_ld(&self, a: &super::articles::ArticleRow, canonical: &str) -> Value {
        let mut v = json!({ "@context": "https://schema.org", "@type": "Article", "headline": clip(&a.title, 110), "description": a.description, "url": canonical, "mainEntityOfPage": canonical,
            "dateModified": a.updated_at.to_rfc3339(), "publisher": { "@type": "Organization", "name": self.brand } });
        if let Some(at) = a.published_at {
            v["datePublished"] = json!(at.to_rfc3339());
        }
        if let Some(m) = a.cover_media {
            v["image"] = json!(absolute(self.root, &super::site::media_url(m)));
        }
        v["author"] = if a.author_name.trim().is_empty() { json!({ "@type": "Organization", "name": self.brand }) } else { json!({ "@type": "Person", "name": a.author_name.trim() }) };
        v
    }

    pub fn not_found(&self) -> String {
        self.wrap(format!("<h1>Page not found</h1><p><a href=\"{}\">Back to {}</a></p>", esc(&self.href("/")), esc(self.brand)))
    }

    /// The business, as far as its website shows it.
    pub fn organization(&self) -> Value {
        let c = &self.config.contact;
        let mut o = json!({ "@context": "https://schema.org", "@type": "Organization", "name": self.brand, "url": self.href("/") });
        if let Some(logo) = &self.logo {
            o["logo"] = json!(absolute(self.root, logo));
        }
        let about = [self.config.seo.business_description.as_str(), self.config.seo.description.as_str()].into_iter().find(|x| !x.trim().is_empty());
        if let Some(d) = about {
            o["description"] = json!(d.trim());
        }
        if c.show_phone && !c.phone.trim().is_empty() {
            o["telephone"] = json!(c.phone.trim());
        }
        if c.show_email && !c.email.trim().is_empty() {
            o["email"] = json!(c.email.trim());
        }
        let s = &self.config.social;
        let local = &self.config.seo.local;
        let same: Vec<&str> = [&s.instagram, &s.facebook, &s.tiktok, &s.x, &s.linkedin, &s.youtube, &local.profile_url].into_iter().map(|x| x.trim()).filter(|x| x.starts_with("http")).collect();
        if !same.is_empty() {
            o["sameAs"] = json!(same);
        }
        // Roadmap 157: a place customers can visit, only as the business states it.
        if local.enabled {
            o["@type"] = json!("LocalBusiness");
            o["address"] = postal(&local.street_address, &local.city, &local.region, &local.country, &local.postal_code);
            if !local.phone.trim().is_empty() {
                o["telephone"] = json!(local.phone.trim());
            }
            if !local.maps_url.trim().is_empty() {
                o["hasMap"] = json!(local.maps_url.trim());
            }
            if !local.service_areas.is_empty() {
                o["areaServed"] = json!(local.service_areas);
            }
        }
        o
    }

    /// One `LocalBusiness` per location the business chose to show, on the page that shows them. No separate location pages.
    pub fn branches_ld(&self) -> Vec<Value> {
        self.places.iter().filter(|l| !l.street_address.is_empty()).map(|l| {
            let mut v = json!({ "@context": "https://schema.org", "@type": "LocalBusiness", "name": l.name, "url": self.href("/contact"), "parentOrganization": { "@type": "Organization", "name": self.brand },
                "address": postal(&l.street_address, &l.city, &l.region, &l.country, "") });
            if !l.phone.is_empty() {
                v["telephone"] = json!(l.phone);
            }
            if !l.maps_url.is_empty() {
                v["hasMap"] = json!(l.maps_url);
            }
            if let Some(h) = l.hours.as_ref().and_then(opening_hours) {
                v["openingHoursSpecification"] = json!([h]);
            }
            v
        }).collect()
    }

    pub fn website(&self) -> Value {
        json!({ "@context": "https://schema.org", "@type": "WebSite", "name": self.brand, "url": self.href("/") })
    }

    /// `Product`, with an `Offer` only for a price the website shows, in a currency we can name, and availability only when
    /// the website shows it. No identifier, rating or review is made up.
    pub fn product(&self, p: &PubProduct, canonical: &str) -> Value {
        let mut v = json!({ "@context": "https://schema.org", "@type": "Product", "name": p.name, "url": canonical });
        if !p.description.trim().is_empty() {
            v["description"] = json!(clip(&p.description, 5000));
        }
        if !p.photos.is_empty() {
            v["image"] = json!(p.photos.iter().map(|u| absolute(self.root, u)).collect::<Vec<_>>());
        }
        if let Some(c) = &p.category_name {
            v["category"] = json!(c);
        }
        if let (Some(price), Some(code)) = (p.price, currency_code(self.currency)) {
            let mut offer = json!({ "@type": "Offer", "price": price.round_dp(2).normalize().to_string(), "priceCurrency": code, "url": canonical });
            if let Some(stock) = p.in_stock {
                offer["availability"] = json!(if stock { "https://schema.org/InStock" } else { "https://schema.org/OutOfStock" });
            }
            v["offers"] = offer;
        }
        v
    }

    /// `BreadcrumbList` from (name, path) steps.
    pub fn breadcrumbs(&self, steps: &[(String, String)]) -> Value {
        let items: Vec<Value> = steps.iter().enumerate().map(|(i, (name, path))| json!({ "@type": "ListItem", "position": i + 1, "name": name, "item": self.href(path) })).collect();
        json!({ "@context": "https://schema.org", "@type": "BreadcrumbList", "itemListElement": items })
    }
}

pub fn shown_testimonials(c: &SiteConfig) -> impl Iterator<Item = &crate::website::Testimonial> {
    c.testimonials.items.iter().filter(|t| c.testimonials.show && t.published && !t.sample)
}

/// Images and pages may live on the S'Shop host while `root` carries `/s/{slug}`.
pub fn absolute(root: &str, url: &str) -> String {
    if url.starts_with("http") {
        return url.to_string();
    }
    let host_root = root.split("/s/").next().unwrap_or(root);
    format!("{host_root}{url}")
}

/// JSON for a `<script type="application/ld+json">`: `<` is escaped so no value can end the block.
pub fn ld_block(v: &Value) -> String {
    format!("<script type=\"application/ld+json\">{}</script>\n", v.to_string().replace('<', "\\u003c"))
}

/// Roadmap 157: a search title from a product or category's own name and the business, shortened rather than cut.
pub fn suggest_title(name: &str, brand: &str, place: &str) -> String {
    let full = if place.trim().is_empty() { format!("{name} | {brand}") } else { format!("{name} | {brand} {}", place.trim()) };
    if full.chars().count() <= 60 {
        return full;
    }
    let short = format!("{name} | {brand}");
    if short.chars().count() <= 60 { short } else { clip(name, 60) }
}

/// Roadmap 157: a search description from what is verified: the product's own description, else only its name, category and
/// the business. No claims are added.
pub fn suggest_description(name: &str, description: &str, category: Option<&str>, brand: &str, place: &str) -> String {
    let d = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if d.chars().count() >= 40 {
        return clip(&d, 155);
    }
    let what = category.map_or_else(|| name.to_string(), |c| format!("{name} in {c}"));
    let from = if place.trim().is_empty() { brand.to_string() } else { format!("{brand}, {}", place.trim()) };
    clip(&if d.is_empty() { format!("{what} from {from}.") } else { format!("{what} from {from}. {d}") }, 155)
}

/// `robots.txt` of a host serving websites. The JSON the pages read and the images must stay readable for crawlers to
/// render them; everything else under `/api/` stays out.
pub fn robots(sitemap: &str, extra_disallow: &[&str]) -> String {
    let mut out = String::from("User-agent: *\nAllow: /\nAllow: /api/site/\nAllow: /api/photos/\nAllow: /api/public/\nDisallow: /api/\n");
    for d in extra_disallow {
        out.push_str(&format!("Disallow: {d}\n"));
    }
    out.push_str(&format!("Sitemap: {sitemap}\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;

    fn product(price: Option<Decimal>, in_stock: Option<bool>) -> PubProduct {
        PubProduct {
            id: Uuid::nil(), slug: "a-b".into(), name: "A <b>".into(), description: "D".into(), category_id: None, category_name: None, price, badge: String::new(), in_stock,
            action: "enquire".into(), secondary_action: None, price_note: None, cta_label: String::new(), photo: None, photo_thumb: None, photos: vec!["/api/photos/1".into()], featured: false,
            sort: 0, created_at: Utc::now(), updated_at: Utc::now(), sold: 0, search: String::new(), stock_ok: true, compare_at: None, seo_title: String::new(), seo_description: String::new(),
        }
    }

    fn with<T>(currency: &str, f: impl FnOnce(&Ctx) -> T) -> T {
        let config = SiteConfig::default();
        f(&Ctx { root: "https://shop.example", brand: "Any Business", currency, config: &config, cats: &[], logo: None, places: &[], articles: 0 })
    }

    #[test]
    fn currency_is_named_only_when_known() {
        assert_eq!(currency_code("KSh").as_deref(), Some("KES"));
        assert_eq!(currency_code("usd").as_deref(), Some("USD"));
        assert_eq!(currency_code("beads"), None);
        assert_eq!(currency_code("Bds"), None, "a mixed-case abbreviation we do not know is not guessed");
    }

    #[test]
    fn a_hidden_price_never_reaches_structured_data() {
        let hidden = with("KSh", |c| c.product(&product(None, Some(true)), "https://x"));
        assert!(hidden.get("offers").is_none());
        assert!(!hidden.to_string().contains("rating"));
        let shown = with("KSh", |c| c.product(&product(Some(Decimal::new(1250, 1)), None), "https://x"));
        assert_eq!(shown["offers"]["price"], "125");
        assert_eq!(shown["offers"]["priceCurrency"], "KES");
        assert!(shown["offers"].get("availability").is_none(), "availability only when the website shows it");
        let unknown = with("beads", |c| c.product(&product(Some(Decimal::ONE), None), "https://x"));
        assert!(unknown.get("offers").is_none(), "no price with a guessed currency");
    }

    #[test]
    fn page_content_is_escaped_and_blocks_cannot_be_closed() {
        let html = with("KSh", |c| c.product_page(&product(None, None)));
        assert!(html.contains("A &lt;b&gt;") && !html.contains("A <b>"));
        assert!(!ld_block(&json!({ "x": "</script>" })).contains("</script><"));
        assert!(!ld_block(&json!({ "x": "</script>" }))[30..].contains("</script>\"}"));
    }

    #[test]
    fn suggestions_use_only_what_is_known_and_fit() {
        assert_eq!(suggest_title("Claw Hammer", "Any Business", "Ghana"), "Claw Hammer | Any Business Ghana");
        let long = "A very long product name that nearly fills the whole search title";
        assert!(suggest_title(long, "Any Business", "Ghana").chars().count() <= 60);
        assert_eq!(suggest_description("Claw Hammer", "", Some("Tools"), "Any Business", "Accra"), "Claw Hammer in Tools from Any Business, Accra.");
        assert_eq!(suggest_description("X", "", None, "B", ""), "X from B.");
        let own = "A steel claw hammer with a rubber grip, balanced for everyday work around the home.";
        assert_eq!(suggest_description("Claw Hammer", own, None, "B", ""), own);
    }

    #[test]
    fn opening_hours_only_when_really_set() {
        use crate::settings::Hours;
        assert!(opening_hours(&Hours::default()).is_none(), "the default (open all day) was never set by anyone");
        let h = Hours { days: [true, true, true, true, true, false, false], open: "08:00".into(), close: "17:00".into() };
        let v = opening_hours(&h).unwrap();
        assert_eq!(v["dayOfWeek"].as_array().unwrap().len(), 5);
        assert_eq!(v["dayOfWeek"][0], "Monday");
    }

    #[test]
    fn robots_keeps_what_pages_need_and_names_the_sitemap() {
        let r = robots("https://shop.example/sitemap.xml", &[]);
        assert!(r.contains("Allow: /api/site/") && r.contains("Allow: /api/photos/") && r.contains("Disallow: /api/\n") && r.contains("Sitemap: https://shop.example/sitemap.xml"));
    }
}
