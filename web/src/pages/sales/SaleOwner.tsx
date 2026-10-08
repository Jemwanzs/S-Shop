/** Sale Owner (roadmap 62): who is credited with a sale. Defaults to the signed-in user; others only with permission. */
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Check, Pencil, UserRound } from "lucide-react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Loading } from "@/components/Page";
import { t } from "@/lib/i18n";

export interface Owner { id: string; name: string; role?: string }

export function useOwners(branchId: string | undefined, enabled: boolean) {
  return useQuery({
    queryKey: ["sale-owners", branchId],
    queryFn: () => api<{ items: (Owner & { me: boolean })[] }>("/sales/owners", { query: { branch_id: branchId } }),
    enabled: enabled && !!branchId,
    staleTime: 60_000,
  });
}

/** Searchable list of eligible owners (active salespeople of the branch). */
export function OwnerPicker({ open, onOpenChange, branchId, value, onPick, exclude, title = "Sale Owner" }: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  branchId: string | undefined;
  value: string | null;
  onPick: (o: Owner) => void;
  exclude?: string | null;
  title?: string;
}) {
  const { data, isLoading } = useOwners(branchId, open);
  const [q, setQ] = useState("");
  const list = useMemo(() => {
    const k = q.trim().toLowerCase();
    return (data?.items ?? []).filter((o) => o.id !== exclude && (!k || `${o.name} ${o.role ?? ""}`.toLowerCase().includes(k)));
  }, [data, q, exclude]);
  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} title={title} description="Active salespeople at this branch">
      <div className="space-y-2">
        <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("Search people")} autoFocus />
        {isLoading ? <Loading /> : (
          <ul className="max-h-[50vh] divide-y overflow-y-auto">
            {list.map((o) => (
              <li key={o.id}>
                <button type="button" onClick={() => onPick(o)} className={cn("flex w-full items-center gap-3 px-1 py-2.5 text-start hover:bg-accent/50", value === o.id && "text-primary")}>
                  <UserRound className="h-4 w-4 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{o.name}{"me" in o && o.me && <span className="text-muted-foreground"> · {t("you")}</span>}</span>
                    {o.role && <span className="block truncate text-xs text-muted-foreground">{o.role}</span>}
                  </span>
                  {value === o.id && <Check className="h-4 w-4" />}
                </button>
              </li>
            ))}
            {!list.length && <li className="py-6 text-center text-sm text-muted-foreground">{t("No one matches.")}</li>}
          </ul>
        )}
      </div>
    </ResponsiveDialog>
  );
}

/** The compact "Sale Owner: Name ✎" line at the top of New Sale. */
export function SaleOwnerBar({ owner, onChange }: { owner: Owner | null; onChange: (o: Owner | null) => void }) {
  const { profile, branch, can } = useSession();
  const [open, setOpen] = useState(false);
  const me = profile!.user;
  const current = owner ?? { id: me.id, name: me.name };
  const canAssign = can("sales.assign_owner");
  const content = (
    <>
      <UserRound className="h-4 w-4 shrink-0 text-muted-foreground" />
      <span className="text-muted-foreground">{t("Sale Owner")}:</span>
      <span className="min-w-0 truncate font-medium">{current.name}</span>
      {owner && owner.id !== me.id && <span className="shrink-0 rounded-full bg-primary/10 px-1.5 text-[11px] font-medium text-primary">{t("assigned")}</span>}
      {canAssign && <Pencil className="ms-auto h-3.5 w-3.5 shrink-0 text-muted-foreground" />}
    </>
  );
  return (
    <>
      {canAssign ? (
        <button type="button" onClick={() => setOpen(true)} className="mb-2 flex w-full items-center gap-2 rounded-xl border bg-card px-3 py-2 text-sm hover:bg-accent/40" aria-label={t("Change Sale Owner")}>
          {content}
        </button>
      ) : (
        <div className="mb-2 flex w-full items-center gap-2 rounded-xl border bg-card px-3 py-2 text-sm">{content}</div>
      )}
      <OwnerPicker open={open} onOpenChange={setOpen} branchId={branch?.id} value={current.id}
        onPick={(o) => { onChange(o.id === me.id ? null : { id: o.id, name: o.name }); setOpen(false); }} />
    </>
  );
}
