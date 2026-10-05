import type { ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { count } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { EmptyState, ErrorState, Loading } from "./Page";

export interface Column<T> {
  key: string;
  header: ReactNode;
  cell: (row: T) => ReactNode;
  align?: "left" | "right" | "center";
  /** Hide on narrower desktops to keep tables readable. */
  hideBelow?: "lg" | "xl" | "2xl";
  className?: string;
}

const HIDE = { lg: "hidden lg:table-cell", xl: "hidden xl:table-cell", "2xl": "hidden 2xl:table-cell" };

/** One definition, two layouts: a full table from md up, tappable cards on phones. */
export function DataList<T>({
  rows,
  columns,
  rowKey,
  onRowClick,
  mobile,
  loading,
  error,
  retry,
  empty,
  footer,
  className,
}: {
  rows: T[] | undefined;
  columns: Column<T>[];
  rowKey: (row: T) => string;
  onRowClick?: (row: T) => void;
  mobile: (row: T) => ReactNode;
  loading?: boolean;
  error?: unknown;
  retry?: () => void;
  empty?: ReactNode;
  footer?: ReactNode;
  className?: string;
}) {
  if (error) return <div className={cn("surface", className)}><ErrorState error={error} retry={retry} /></div>;
  if (loading && !rows) return <div className={cn("surface", className)}><Loading /></div>;
  if (!rows?.length) return <div className={cn("surface", className)}>{empty ?? <EmptyState title="Nothing here yet" />}</div>;

  const align = (a?: string) => (a === "right" ? "text-right" : a === "center" ? "text-center" : "text-left");
  return (
    <div className={cn("surface overflow-hidden", className)}>
      <div className="hidden md:block">
        <table className="w-full text-sm">
          <thead className="sticky top-0 z-10 bg-muted/60 backdrop-blur">
            <tr>
              {columns.map((c) => (
                <th key={c.key} className={cn("label-caps whitespace-nowrap px-4 py-3 font-semibold", align(c.align), c.hideBelow && HIDE[c.hideBelow])}>
                  {c.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y">
            {rows.map((r) => (
              <tr
                key={rowKey(r)}
                onClick={onRowClick ? () => onRowClick(r) : undefined}
                className={cn("transition-colors", onRowClick && "cursor-pointer hover:bg-accent/40")}
              >
                {columns.map((c) => (
                  <td key={c.key} className={cn("px-4 py-3 align-middle", align(c.align), c.hideBelow && HIDE[c.hideBelow], c.className)}>
                    {c.cell(r)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="divide-y md:hidden">
        {rows.map((r) => (
          <li key={rowKey(r)}>
            {onRowClick ? (
              <button type="button" onClick={() => onRowClick(r)} className="block w-full px-3.5 py-2.5 text-left active:bg-accent/50">
                {mobile(r)}
              </button>
            ) : (
              <div className="px-3.5 py-2.5">{mobile(r)}</div>
            )}
          </li>
        ))}
      </ul>
      {footer}
    </div>
  );
}

export function Pager({ total, limit, offset, onChange }: { total: number; limit: number; offset: number; onChange: (offset: number) => void }) {
  if (total <= limit) return null;
  const from = offset + 1;
  const to = Math.min(offset + limit, total);
  return (
    <div className="flex items-center justify-between border-t px-4 py-2 text-sm text-muted-foreground">
      <span className="num">
        {count(from)}–{count(to)} of {count(total)}
      </span>
      <div className="flex gap-1">
        <Button variant="ghost" size="icon-sm" disabled={offset === 0} onClick={() => onChange(Math.max(0, offset - limit))} aria-label="Previous page">
          <ChevronLeft />
        </Button>
        <Button variant="ghost" size="icon-sm" disabled={to >= total} onClick={() => onChange(offset + limit)} aria-label="Next page">
          <ChevronRight />
        </Button>
      </div>
    </div>
  );
}

/** Two-line mobile card row: title/subtitle left, value/meta right. */
export function CardRow({ title, subtitle, value, meta, leading }: { title: ReactNode; subtitle?: ReactNode; value?: ReactNode; meta?: ReactNode; leading?: ReactNode }) {
  return (
    <div className="flex items-center gap-3">
      {leading}
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium">{title}</div>
        {subtitle && <div className="truncate text-xs text-muted-foreground">{subtitle}</div>}
      </div>
      {(value || meta) && (
        <div className="shrink-0 text-right">
          {value && <div className="num font-semibold">{value}</div>}
          {meta && <div className="text-xs text-muted-foreground">{meta}</div>}
        </div>
      )}
    </div>
  );
}
