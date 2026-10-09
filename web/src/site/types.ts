/** The published website as the server sends it (routes/site.rs → site_data; config = website.rs SiteConfig). */

export interface Cta { label: string; target: string }
export interface Palette { primary: string; secondary: string; accent: string; background: string; surface: string; text: string; muted: string; heading: string }
export interface Theme { light: Palette; dark: Palette; modes: "light" | "dark" | "both"; style: "modern" | "minimal" | "elegant" | "bold"; heading_font: string; body_font: string; scale: "compact" | "balanced" | "spacious"; motion?: "off" | "subtle" | "standard" }
export interface NavItem { key: string; label: string; visible: boolean }
export interface Section { key: string; visible: boolean; heading: string; subheading: string; layout: string; product_ids: string[]; cta_label: string; cta_target: string }
export interface Hero { headline: string; text: string; image: string | null; primary: Cta; secondary: Cta; align: "left" | "center"; overlay: boolean }
export interface Promotion { id: string; title: string; text: string; image: string | null; cta: Cta; active: boolean }
export interface About { intro: string; story: string; mission: string; vision: string; values: string[]; image: string | null; show_story: boolean; show_mission: boolean; show_vision: boolean; show_values: boolean; cta: Cta }
export interface Contact { intro: string; phone: string; whatsapp: string; email: string; location: string; map_url: string; hours: string; show_phone: boolean; show_whatsapp: boolean; show_email: boolean; show_location: boolean; show_hours: boolean }
export interface Social { instagram: string; facebook: string; tiktok: string; x: string; linkedin: string; youtube: string; whatsapp: string }
export interface Grid { mobile: number; tablet: number; desktop: number; card: "compact" | "standard" | "large"; ratio: "square" | "portrait" | "landscape"; fit: "cover" | "contain"; radius: "none" | "small" | "medium" | "large"; shadow: boolean; name_lines: number; show_badges: boolean; quick_add: boolean; show_availability: boolean }
export interface CategoriesCfg { mobile: number; tablet: number; desktop: number; card: "compact" | "standard" | "large"; style: "grid" | "carousel"; show_images: boolean; limit: number }
export interface Service { id: string; name: string; icon: string; image: string | null; short: string; details: string; cta: Cta; active: boolean }
export interface Testimonial { id: string; name: string; quote: string; rating: number; photo: string | null; position: string; published: boolean; sample: boolean }

export interface SiteConfig {
  brand: { name: string; tagline: string; logo: string | null };
  theme: Theme;
  navigation: NavItem[];
  sections: Section[];
  hero: Hero;
  promotions: Promotion[];
  about: About;
  contact: Contact;
  social: Social;
  products: { grid: Grid; max_photos: number; hidden_action: string; auto_publish_new: boolean; items?: unknown[]; pagination?: boolean; per_page?: number };
  categories: CategoriesCfg & { items?: unknown[] };
  services: { intro: string; items: Service[] };
  testimonials: { show: boolean; auto_scroll: boolean; speed: "slow" | "normal" | "fast"; items: Testimonial[] };
  seo: { title: string; description: string; share_image: string | null; business_description: string };
  cookies: { analytics: boolean; privacy_policy: string };
  footer: { description: string; policies: string; show_attribution: boolean };
  /** Roadmap 80: the page at the website's root. */
  landing?: "home" | "products" | "categories" | "services";
}

export interface Category { id: string; name: string; image: string | null; count: number }

export interface SiteData {
  available: boolean;
  preview?: boolean;
  slug: string;
  domain: string | null;
  version: number;
  business: { name: string; tagline?: string; phone?: string; currency: string; logo_url: string | null };
  config: SiteConfig;
  categories: Category[];
  show_prices: boolean;
  ordering: { enabled: boolean; otp_required: boolean };
}

export interface Product {
  id: string;
  slug: string;
  name: string;
  description: string;
  category_id: string | null;
  category_name: string | null;
  /** null when the business does not show this product's price. */
  price: string | null;
  badge: string;
  in_stock: boolean | null;
  /** add_to_cart | enquire | contact | whatsapp */
  action: string;
  cta_label: string;
  photo: string | null;
  photo_thumb: string | null;
  photos: string[];
  featured: boolean;
  /** Roadmap 80: the "was" price (only with a visible, lower current price). */
  compare_at?: string | null;
  seo_title: string;
  seo_description: string;
}

export interface CartLine { id: string; slug: string; name: string; price: string | null; photo: string | null; qty: number }
