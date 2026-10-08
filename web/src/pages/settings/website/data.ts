/** Website Management Centre: types and API hooks (server: routes/website.rs; document: website.rs SiteConfig). */
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { SiteConfig as PublicConfig } from "@/site/types";
import type { BillingSummary } from "@/lib/billing";

export type { Cta, Palette, Section, NavItem, Service, Testimonial, Promotion } from "@/site/types";

export interface ProductCfg {
  product_id: string;
  published: boolean;
  featured: boolean;
  sort: number;
  marketing_name: string;
  marketing_description: string;
  badge: "" | "new" | "featured" | "offer";
  price: "inherit" | "show" | "hide";
  hidden_action: "" | "enquire" | "contact" | "whatsapp" | "order";
  use_product_photos: boolean;
  photos: string[];
  hidden_photos: string[];
  seo_title: string;
  seo_description: string;
  category_id: string | null;
  cta_label: string;
}
export interface CategoryCfg { category_id: string; visible: boolean; sort: number; image: string | null }

/** The full editable document (the public one leaves out per-product settings and drafts). */
export type SiteConfig = Omit<PublicConfig, "products" | "categories"> & {
  products: Omit<PublicConfig["products"], "items"> & { items: ProductCfg[] };
  categories: Omit<PublicConfig["categories"], "items"> & { items: CategoryCfg[] };
};


export interface Overview {
  status: "none" | "requested" | "declined" | "active" | "disabled";
  status_reason?: string;
  request_message?: string;
  requested_at?: string | null;
  activated_at?: string | null;
  business: string;
  slug?: string;
  live?: boolean;
  billing_suspended?: boolean;
  billing: BillingSummary;
  draft?: SiteConfig | null;
  draft_updated_at?: string | null;
  draft_updated_by?: string | null;
  has_unpublished?: boolean;
  version?: number;
  published_at?: string | null;
  published_by?: string | null;
  public_url?: string;
  preview_url?: string;
  domain?: { domain: string; status: string } | null;
  show_prices?: boolean;
  fonts?: string[];
}

export function useOverview() {
  return useQuery({ queryKey: ["website"], queryFn: () => api<Overview>("/website") });
}

export interface CatalogueProduct { id: string; name: string; code: string; category_id: string | null; category_name: string | null; price: string; is_active: boolean; available_for_orders: boolean; photos: string[] }
export interface CatalogueCategory { id: string; name: string; is_active: boolean }

export function useCatalogue() {
  return useQuery({ queryKey: ["website-catalogue"], queryFn: () => api<{ products: CatalogueProduct[]; categories: CatalogueCategory[] }>("/website/catalogue"), staleTime: 30_000 });
}

export interface Media { id: string; kind: string; name: string; mime: string; width: number; height: number; bytes: number; quality: "good" | "warning"; warnings: string[]; archived: boolean; created_at: string; in_use?: boolean }

export function useMedia(archived = false) {
  return useQuery({ queryKey: ["website-media", archived], queryFn: () => api<{ items: Media[] }>("/website/media", { query: { archived } }) });
}

export const mediaUrl = (id: string | null | undefined, thumb = false) => (id ? `/api/site/media/${id}${thumb ? "?size=thumb" : ""}` : undefined);

export function blankProduct(product_id: string): ProductCfg {
  return {
    product_id, published: true, featured: false, sort: 0, marketing_name: "", marketing_description: "", badge: "", price: "inherit",
    hidden_action: "", use_product_photos: true, photos: [], hidden_photos: [], seo_title: "", seo_description: "", category_id: null, cta_label: "",
  };
}

export const uid = () => (crypto.randomUUID ? crypto.randomUUID() : "10000000-1000-4000-8000-100000000000".replace(/[018]/g, (c) => (Number(c) ^ (Math.random() * 16) >> (Number(c) / 4)).toString(16)));

/** WCAG contrast ratio of two #RRGGBB colours (mirrors the server's validation). */
export function contrast(a: string, b: string): number {
  const lum = (h: string) => {
    const m = /^#([0-9a-f]{6})$/i.exec(h);
    if (!m) return null;
    const n = parseInt(m[1], 16);
    const ch = (x: number) => {
      const c = x / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * ch((n >> 16) & 255) + 0.7152 * ch((n >> 8) & 255) + 0.0722 * ch(n & 255);
  };
  const x = lum(a);
  const y = lum(b);
  if (x === null || y === null) return 1;
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
export const onColor = (bg: string) => (contrast("#FFFFFF", bg) >= contrast("#111111", bg) ? "#FFFFFF" : "#111111");
