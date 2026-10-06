import { Children, isValidElement, useContext, useEffect, useMemo, useRef, useState, type ReactElement, type ReactNode } from "react";
import { Command as CommandPrimitive } from "cmdk";
import { Check, ChevronDown, Search } from "lucide-react";
import { useMediaQuery } from "@/lib/hooks";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Drawer, DrawerContent, DrawerTitle } from "@/components/ui/drawer";
import { FieldLabel } from "@/components/Form";

interface Opt {
  value: string;
  label: string;
  disabled: boolean;
}

/** `<option value=…>label</option>` children (also inside arrays/fragments) → options. */
function readOptions(children: ReactNode): Opt[] {
  const out: Opt[] = [];
  const walk = (nodes: ReactNode) =>
    Children.forEach(nodes, (n) => {
      if (!isValidElement(n)) return;
      const el = n as ReactElement<{ value?: string | number; children?: ReactNode; disabled?: boolean }>;
      if (el.type === "option") {
        const label = Children.toArray(el.props.children).join("");
        out.push({ value: String(el.props.value ?? label), label, disabled: !!el.props.disabled });
      } else if (el.props.children) {
        walk(el.props.children);
      }
    });
  walk(children);
  return out;
}

/** Long lists get a search box. */
const SEARCH_FROM = 9;

/**
 * The S'Shop dropdown, used for every select in the app. Same API as a native select (pass `<option>` children);
 * opens a panel under the field on larger screens and a bottom sheet on phones, with a check on the chosen
 * option, keyboard navigation, and search for long lists.
 */
export function Select({ value, onChange, children, className, disabled, placeholder, label, searchable }: {
  value: string;
  onChange: (v: string) => void;
  children: ReactNode;
  className?: string;
  disabled?: boolean;
  /** Shown when no option matches `value`. */
  placeholder?: string;
  /** Sheet title on phones and accessible name (defaults to the placeholder). */
  label?: string;
  /** Force the search box on/off (default: on for long lists). */
  searchable?: boolean;
}) {
  const options = useMemo(() => readOptions(children), [children]);
  const fieldLabel = useContext(FieldLabel);
  label ??= fieldLabel;
  const [open, setOpen] = useState(false);
  const wide = useMediaQuery("(min-width: 768px)");
  const current = options.find((o) => o.value === value);
  const search = searchable ?? options.length >= SEARCH_FROM;

  const trigger = (
    <button
      type="button"
      role="combobox"
      aria-expanded={open}
      aria-label={label ?? placeholder}
      disabled={disabled}
      onClick={wide ? undefined : () => setOpen(true)}
      className={cn(
        "flex h-control w-full items-center justify-between gap-2 rounded-lg border border-input bg-background px-3 text-start text-control transition-colors",
        "hover:border-foreground/25 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        open && "border-primary ring-2 ring-primary/20",
        className,
      )}
    >
      <span className={cn("min-w-0 flex-1 truncate", !current && "text-muted-foreground")}>{current?.label ?? placeholder ?? "—"}</span>
      <ChevronDown className={cn("h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200", open && "rotate-180")} />
    </button>
  );
  const choose = (v: string) => {
    onChange(v);
    setOpen(false);
  };

  if (!wide) {
    return (
      <>
        {trigger}
        <Drawer open={open} onOpenChange={setOpen} shouldScaleBackground={false}>
          <DrawerContent className="max-h-[85dvh]">
            <DrawerTitle className="px-4 pb-1 pt-3 text-sm font-semibold">{label ?? placeholder ?? t("Choose")}</DrawerTitle>
            <OptionList options={options} value={value} onChoose={choose} search={search} touch />
          </DrawerContent>
        </Drawer>
      </>
    );
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent align="start" sideOffset={6} className="w-[var(--radix-popover-trigger-width)] min-w-44 overflow-hidden p-0" onOpenAutoFocus={(e) => e.preventDefault()}>
        <OptionList options={options} value={value} onChoose={choose} search={search} />
      </PopoverContent>
    </Popover>
  );
}

function OptionList({ options, value, onChoose, search, touch }: { options: Opt[]; value: string; onChoose: (v: string) => void; search: boolean; touch?: boolean }) {
  const list = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const root = useRef<HTMLDivElement>(null);
  // Open on the chosen option: highlighted and scrolled into view; keyboard goes to the list (or the search box).
  const keyOf = (i: number) => `${i}:${options[i].label}`;
  const selectedIndex = options.findIndex((o) => o.value === value);
  const [active, setActive] = useState(selectedIndex >= 0 ? keyOf(selectedIndex) : undefined);
  useEffect(() => {
    requestAnimationFrame(() => {
      list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
      if (search && !touch) input.current?.focus();
      else root.current?.focus();
    });
  }, [search, touch]);

  return (
    <CommandPrimitive ref={root} value={active} onValueChange={setActive} loop tabIndex={-1} className="flex flex-col outline-none">
      {search && (
        <div className="flex items-center gap-2 border-b px-3">
          <Search className="h-4 w-4 shrink-0 text-muted-foreground" />
          <CommandPrimitive.Input ref={input} placeholder={t("Search…")} className="h-10 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground" />
        </div>
      )}
      <CommandPrimitive.List ref={list} className={cn("scroll-thin overflow-y-auto overscroll-contain p-1", touch ? "max-h-[60dvh] pb-[max(0.5rem,env(safe-area-inset-bottom))]" : "max-h-72")}>
        <CommandPrimitive.Empty className="px-3 py-6 text-center text-sm text-muted-foreground">{t("No matches")}</CommandPrimitive.Empty>
        {options.map((o, i) => (
          <CommandPrimitive.Item
            key={keyOf(i)}
            value={keyOf(i)}
            keywords={[o.label]}
            disabled={o.disabled}
            onSelect={() => onChoose(o.value)}
            className={cn(
              "relative flex cursor-pointer select-none items-center gap-2 rounded-md px-2.5 text-sm outline-none",
              touch ? "min-h-11" : "min-h-8 py-1.5",
              "data-[selected=true]:bg-accent data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-45",
              o.value === value && "font-medium text-primary",
            )}
          >
            <span className="min-w-0 flex-1 truncate">{o.label || "—"}</span>
            {o.value === value && <Check className="h-4 w-4 shrink-0" />}
          </CommandPrimitive.Item>
        ))}
      </CommandPrimitive.List>
    </CommandPrimitive>
  );
}
