import type { ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { tx } from "@/lib/i18n";
import { count } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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

/** Roadmap 151: optional row selection (bulk actions). The checkbox never opens the row. */
export interface Selection<T> {
  isSelected: (row: T) => boolean;
  toggle: (row: T, on: boolean) => void;
  /** The header box: every row on this page selected (true), some ("indeterminate"), none (false). */
  page: boolean | "indeterminate";
  togglePage: (on: boolean) => void;
  /** Accessible name of a row's box. */
  label: (row: T) => string;
}

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
  selection,
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
  selection?: Selection<T>;
}) {
  if (error) return <div className={cn("surface", className)}><ErrorState error={error} retry={retry} /></div>;
  if (loading && !rows) return <div className={cn("surface", className)}><Loading /></div>;
  if (!rows?.length) return <div className={cn("surface", className)}>{empty ?? <EmptyState title="Nothing here yet" />}</div>;

  const align = (a?: string) => (a === "right" ? "text-end" : a === "center" ? "text-center" : "text-start");
  return (
    <div className={cn("surface overflow-hidden", className)}>
      {/* Wide tables scroll sideways inside the card, never the page. */}
      <div className="scroll-thin hidden overflow-x-auto md:block">
        <table className="w-full text-sm">
          <thead className="sticky top-0 z-10 bg-muted/60 backdrop-blur-sm">
            <tr>
              {selection && (
                <th className="w-10 px-3 py-3">
                  <Checkbox checked={selection.page} onCheckedChange={(v) => selection.togglePage(v === true)} aria-label={tx("Select this page")} />
                </th>
              )}
              {columns.map((c) => (
                <th key={c.key} className={cn("label-caps whitespace-nowrap px-4 py-3 font-semibold", align(c.align), c.hideBelow && HIDE[c.hideBelow])}>
                  {tx(c.header)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y">
            {rows.map((r) => (
              <tr
                key={rowKey(r)}
                onClick={onRowClick ? () => onRowClick(r) : undefined}
                className={cn("transition-colors", onRowClick && "cursor-pointer hover:bg-accent/40", selection?.isSelected(r) && "bg-primary/5")}
              >
                {selection && (
                  <td className="w-10 px-3 py-3 align-middle" onClick={(e) => e.stopPropagation()}>
                    <Checkbox checked={selection.isSelected(r)} onCheckedChange={(v) => selection.toggle(r, v === true)} aria-label={selection.label(r)} />
                  </td>
                )}
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
          <li key={rowKey(r)} className={cn(selection && "flex items-center", selection?.isSelected(r) && "bg-primary/5")}>
            {selection && (
              <div className="ps-3.5">
                <Checkbox checked={selection.isSelected(r)} onCheckedChange={(v) => selection.toggle(r, v === true)} aria-label={selection.label(r)} className="h-5 w-5" />
              </div>
            )}
            {onRowClick ? (
              <button type="button" onClick={() => onRowClick(r)} className="block min-w-0 flex-1 px-3.5 py-2.5 text-start active:bg-accent/50">
                {mobile(r)}
              </button>
            ) : (
              <div className="min-w-0 flex-1 px-3.5 py-2.5">{mobile(r)}</div>
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
        <div className="shrink-0 text-end">
          {value && <div className="num font-semibold">{value}</div>}
          {meta && <div className="text-xs text-muted-foreground">{meta}</div>}
        </div>
      )}
    </div>
  );
}
