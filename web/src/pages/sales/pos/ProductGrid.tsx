/**
 * Roadmap 109 — the Record Sale product list in the business's chosen style: compact cards (the original), image
 * cards or list rows; optional pictures (primary photo, small copy); price labels on or off; tablet / desktop column
 * counts that never exceed what the space beside the cart allows (phones stay automatic).
 */
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Package } from "lucide-react";
import { photoUrl } from "@/lib/api";
import { count, money } from "@/lib/format";
import type { PosProduct, Settings } from "@/lib/types";
import { cn } from "@/lib/utils";

type Display = Settings["sales"]["pos"];

/** The smallest comfortable width of one product, per style (px). */
const MIN_WIDTH: Record<string, number> = { compact: 210, image: 150, list: 250 };

/** A product's primary photo in a rounded square — never stretched; a quiet placeholder when there is none. */
export function ProductThumb({ id, className }: { id: string | null; className?: string }) {
  const [failed, setFailed] = useState(false);
  return (
    <span className={cn("flex shrink-0 items-center justify-center overflow-hidden rounded-lg bg-muted", className)}>
      {id && !failed
        ? <img src={`${photoUrl(id)}?size=thumb`} alt="" loading="lazy" decoding="async" onError={() => setFailed(true)} className="h-full w-full object-contain" />
        : <Package className="h-1/2 w-1/2 text-muted-foreground/50" aria-hidden />}
    </span>
  );
}

function useColumns(display: Display) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [viewport, setViewport] = useState(() => window.innerWidth);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setWidth(e.contentRect.width));
    ro.observe(el);
    const onResize = () => setViewport(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", onResize);
    };
  }, []);
  const style = display.style in MIN_WIDTH ? display.style : "compact";
  const wanted = viewport < 768 ? (style === "image" ? 2 : 1) : viewport < 1024 ? display.tablet_columns : display.desktop_columns;
  const fits = width > 0 ? Math.max(1, Math.floor((width + 8) / (MIN_WIDTH[style] + 8))) : wanted;
  return { ref, columns: Math.max(1, Math.min(wanted, fits)), style };
}

export function ProductGrid({ products, display, inCart, onOpen, lowThreshold, badge }: {
  products: PosProduct[];
  display: Display;
  inCart: (id: string) => number;
  onOpen: (p: PosProduct) => void;
  lowThreshold: number;
  /** Optional decoration per product (e.g. sales ranking). */
  badge?: (p: PosProduct) => ReactNode;
}) {
  const { ref, columns, style } = useColumns(display);
  const images = style === "image" || display.show_images;
  return (
    <div ref={ref} className="grid gap-2" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
      {products.map((p) => {
        const left = p.available - inCart(p.id);
        const out = left <= 0;
        const stock = (
          <span className={cn("num text-xs", out ? "text-destructive" : left <= lowThreshold ? "text-warning" : "text-success")}>
            {out ? "Out of stock" : `${count(left)} left`}
          </span>
        );
        const price = display.show_prices && <span className="num font-semibold">{money(p.marked_price)}</span>;
        const common = {
          onClick: () => onOpen(p),
          disabled: out && !p.track_items,
          className: cn(
            "surface text-start transition hover:border-primary/40 hover:shadow-lift active:scale-[0.99] disabled:opacity-50",
            inCart(p.id) > 0 && "border-primary/50 bg-primary/5",
          ),
        };
        const sub = <>{p.code}{p.nickname && ` · ${p.nickname}`}{p.track_items && " · per item"}</>;
        if (style === "image") {
          return (
            <button key={p.id} {...common} className={cn(common.className, "flex flex-col gap-2 p-2.5")}>
              <ProductThumb id={p.primary_photo_id} className="aspect-square w-full rounded-xl" />
              <div className="min-w-0 space-y-0.5">
                <div className="flex items-start gap-1">
                  <span className="line-clamp-2 min-w-0 flex-1 text-sm font-medium leading-snug">{p.name}</span>
                  {badge?.(p)}
                </div>
                <div className="truncate text-[11px] text-muted-foreground">{sub}</div>
              </div>
              <div className="mt-auto flex items-baseline justify-between gap-2">
                {price || <span />}
                {stock}
              </div>
            </button>
          );
        }
        if (style === "list") {
          return (
            <button key={p.id} {...common} className={cn(common.className, "flex items-center gap-3 px-2.5 py-2")}>
              {images && <ProductThumb id={p.primary_photo_id} className="h-12 w-12" />}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5"><span className="truncate font-medium">{p.name}</span>{badge?.(p)}</div>
                <div className="truncate text-xs text-muted-foreground">{sub}{p.category_name && ` · ${p.category_name}`}</div>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-0.5">{price}{stock}</div>
            </button>
          );
        }
        return (
          <button key={p.id} {...common} className={cn(common.className, "flex items-center gap-3 p-3")}>
            {images && <ProductThumb id={p.primary_photo_id} className="h-11 w-11" />}
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5"><span className="truncate font-medium">{p.name}</span>{badge?.(p)}</div>
              <div className="truncate text-xs text-muted-foreground">{sub}</div>
            </div>
            <div className="shrink-0 text-end">
              {price && <div>{price}</div>}
              <div>{stock}</div>
            </div>
          </button>
        );
      })}
    </div>
  );
}
