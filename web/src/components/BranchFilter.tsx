/** Roadmap 176: which branch's figures a screen shows — the Current Branch (default), another branch the person may see,
 * or all of them. Viewing only: it never changes the branch stock is received, adjusted, transferred or sold at.
 * People who work at one branch see no filter (they have nothing else to choose). */
import { useState } from "react";
import { Store } from "lucide-react";
import { useSession } from "@/lib/session";
import { t } from "@/lib/i18n";
import { Select } from "@/components/Form";

export const ALL_BRANCHES = "all";

/** The chosen branch filter: a branch id or `all`; starts on the Current Branch. */
export function useBranchFilter(initial?: string | null) {
  const { branch } = useSession();
  const [value, setValue] = useState<string>(initial || branch?.id || "");
  return [value || branch?.id || "", setValue] as const;
}

/** Query parameters for a list that takes `branch_id` (absent = every branch the person may see). */
export function branchQuery(value: string): { branch_id?: string } {
  return value && value !== ALL_BRANCHES ? { branch_id: value } : {};
}

export function BranchFilter({ value, onChange, className }: { value: string; onChange: (v: string) => void; className?: string }) {
  const { profile, branch } = useSession();
  const branches = profile?.branches ?? [];
  if (branches.length < 2) return null;
  return (
    <Select value={value} onChange={onChange} label="Branch" className={className}>
      {branches.map((b) => <option key={b.id} value={b.id}>{b.id === branch?.id ? `${b.name} (${t("current")})` : b.name}</option>)}
      <option value={ALL_BRANCHES}>{t("All accessible branches")}</option>
    </Select>
  );
}

/** Shown while another branch (or all) is being viewed: what is shown, and that working actions stay at the working branch. */
export function BranchViewNote({ value }: { value: string }) {
  const { profile, branch } = useSession();
  if (!branch || !value || value === branch.id) return null;
  const name = value === ALL_BRANCHES ? t("all your branches") : profile?.branches.find((b) => b.id === value)?.name ?? "";
  return (
    <p className="mb-3 flex items-start gap-2 rounded-lg bg-muted/70 p-2.5 text-xs text-muted-foreground">
      <Store className="mt-0.5 h-3.5 w-3.5 shrink-0" />
      <span>{t("Showing")} <b className="text-foreground">{name}</b>. {t("Receiving, adjusting and transfers still use your working branch,")} <b className="text-foreground">{branch.name}</b>.</span>
    </p>
  );
}
