/** Roadmap 204: one way to show a product's name everywhere in the back office. By default the full name wraps onto as
 * many lines as the space needs (Settings → Products → Display preferences → Product Name Appearance = Automatic);
 * a business may limit it to one, two or three lines — then hovering shows the full name and tapping a name that is
 * not itself a button reveals it. The public website keeps its own card settings. */
import { useState, type ReactNode } from "react";
import { useSession } from "@/lib/session";
import { cn } from "@/lib/utils";

export type NameLines = "auto" | "1" | "2" | "3";
const CLAMP: Record<Exclude<NameLines, "auto">, string> = { "1": "line-clamp-1", "2": "line-clamp-2", "3": "line-clamp-3" };

export function useNameLines(): NameLines {
  const { profile } = useSession();
  const v = profile?.settings.product.name_lines;
  return v === "1" || v === "2" || v === "3" ? v : "auto";
}

export function ProductName({ name, className, as = "span", expandable = true, children }: {
  name: string;
  className?: string;
  as?: "span" | "div" | "p";
  /** Tapping reveals a limited name; off where the name sits inside a button or link (the tap belongs to it). */
  expandable?: boolean;
  children?: ReactNode;
}) {
  const lines = useNameLines();
  const [open, setOpen] = useState(false);
  const limited = lines !== "auto" && !open;
  const tap = expandable && lines !== "auto";
  const Tag = as;
  return (
    <Tag
      data-product-name=""
      title={lines !== "auto" ? name : undefined}
      className={cn("min-w-0 [overflow-wrap:anywhere]", limited && CLAMP[lines as Exclude<NameLines, "auto">], tap && "cursor-pointer", className)}
      onClick={tap ? (e) => { e.stopPropagation(); setOpen((o) => !o); } : undefined}
    >
      {name}
      {children}
    </Tag>
  );
}
