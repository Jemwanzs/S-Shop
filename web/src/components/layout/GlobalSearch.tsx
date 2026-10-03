import { useNavigate } from "react-router-dom";
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ClipboardList, Package, Receipt, User } from "lucide-react";
import { api } from "@/lib/api";
import { useDebounced } from "@/lib/hooks";
import { money } from "@/lib/format";
import { useSession } from "@/lib/session";
import { CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";

interface Result {
  type: "product" | "customer" | "order" | "sale";
  id: string;
  title: string;
  subtitle: string;
  amount?: string;
  link: string;
}

const GROUPS: { type: Result["type"]; label: string; icon: typeof Package }[] = [
  { type: "product", label: "Products", icon: Package },
  { type: "customer", label: "Customers", icon: User },
  { type: "order", label: "Orders", icon: ClipboardList },
  { type: "sale", label: "Receipts", icon: Receipt },
];

/** Universal search: product name/nickname/code/barcode, customer name/mobile, order & receipt numbers. */
export function GlobalSearch({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const [q, setQ] = useState("");
  const term = useDebounced(q.trim(), 200);
  const navigate = useNavigate();
  const { currency } = useSession();
  const { data, isFetching } = useQuery({
    queryKey: ["search", term],
    queryFn: () => api<{ results: Result[] }>("/search", { query: { q: term } }),
    enabled: open && term.length >= 2,
  });
  const results = data?.results ?? [];
  return (
    <CommandDialog open={open} onOpenChange={(o) => { if (!o) setQ(""); onOpenChange(o); }}>
      <CommandInput value={q} onValueChange={setQ} placeholder="Name, mobile, barcode, ORD-…, RCP-…" />
      <CommandList className="max-h-[60vh]">
        {term.length >= 2 && !isFetching && <CommandEmpty>No matches</CommandEmpty>}
        {GROUPS.map((g) => {
          const rows = results.filter((r) => r.type === g.type);
          if (!rows.length) return null;
          return (
            <CommandGroup key={g.type} heading={g.label}>
              {rows.map((r) => (
                <CommandItem
                  key={r.type + r.id}
                  value={`${r.type}-${r.id}-${r.title}`}
                  onSelect={() => {
                    onOpenChange(false);
                    setQ("");
                    navigate(r.link);
                  }}
                  className="gap-3"
                >
                  <g.icon className="h-4 w-4 text-muted-foreground" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{r.title}</span>
                    <span className="block truncate text-xs text-muted-foreground">{r.subtitle}</span>
                  </span>
                  {r.amount !== undefined && <span className="num text-sm">{money(r.amount, currency)}</span>}
                </CommandItem>
              ))}
            </CommandGroup>
          );
        })}
      </CommandList>
    </CommandDialog>
  );
}
