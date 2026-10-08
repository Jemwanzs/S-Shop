/** Settings → Users → Roles & Access (roadmap 64): default branch, data-visibility exceptions, allow / restrict exceptions
 * and the effective result the server enforces (role → user exceptions → branches). */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { ActionButton } from "@/components/ActionButton";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Field, Select } from "@/components/Form";
import { Loading } from "@/components/Page";
import { Pill } from "@/components/Badges";
import { t } from "@/lib/i18n";
import { useScopeCatalogue } from "./People";

interface Access {
  role: string;
  administrator: boolean;
  all_branches: boolean;
  default_branch_id: string | null;
  role_permissions: string[];
  overrides: string[];
  effective: string[];
  scopes: { area: string; label: string; role: string | null; user: string | null; effective: string }[];
  grantable: string[];
}

const GRANT_LABELS: Record<string, string> = {
  "sales.assign_owner": "Assign the sale owner when recording",
  "sales.request_owner_change": "Request sale ownership changes",
  "dashboard.view": "View dashboard & analytics",
  "reports.view": "View reports",
  "reports.export": "Export reports (PDF / Excel)",
  "staff.view_others": "View other employees' sales & performance",
};
const SCOPE_LABEL: Record<string, string> = { own: "Own records", branches: "Assigned branches", all: "All branches" };

export function UserAccessDialog({ user, branches, onClose }: { user: { id: string; name: string; branch_ids: string[]; all_branches: boolean }; branches: { id: string; name: string; is_active: boolean }[]; onClose: () => void }) {
  const qc = useQueryClient();
  const cat = useScopeCatalogue();
  const { data, isLoading } = useQuery({ queryKey: ["user-access", user.id], queryFn: () => api<Access>(`/users/${user.id}/access`) });
  const [overrides, setOverrides] = useState<string[]>([]);
  const [defaultBranch, setDefaultBranch] = useState("");
  useEffect(() => {
    if (data) {
      setOverrides(data.overrides);
      setDefaultBranch(data.default_branch_id ?? "");
    }
  }, [data]);
  const save = useMutation({
    mutationFn: () => api(`/users/${user.id}/access`, { method: "PUT", body: { overrides, default_branch_id: defaultBranch || null } }),
    onSuccess: () => {
      toast.success("Access saved");
      qc.invalidateQueries({ queryKey: ["user-access", user.id] });
      qc.invalidateQueries({ queryKey: ["users"] });
      onClose();
    },
    onError: (e) => toast.error(e),
  });
  const scopeOf = (area: string) => overrides.find((o) => o.startsWith(`scope.${area}.`))?.split(".")[2] ?? "";
  const setScope = (area: string, v: string) => setOverrides((o) => [...o.filter((x) => !x.startsWith(`scope.${area}.`)), ...(v ? [`scope.${area}.${v}`] : [])]);
  const grantOf = (p: string) => (overrides.includes(p) ? "allow" : overrides.includes(`-${p}`) ? "restrict" : "");
  const setGrant = (p: string, v: string) => setOverrides((o) => [...o.filter((x) => x !== p && x !== `-${p}`), ...(v === "allow" ? [p] : v === "restrict" ? [`-${p}`] : [])]);
  const userBranches = branches.filter((b) => b.is_active && (user.all_branches || data?.administrator || user.branch_ids.includes(b.id)));
  const dirty = !!data && (JSON.stringify([...overrides].sort()) !== JSON.stringify([...data.overrides].sort()) || (defaultBranch || null) !== data.default_branch_id);

  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} wide title="Roles & Access" description={user.name}
      footer={<ActionButton online busy={save.isPending} busyLabel="Saving…" blockedBy={[!dirty && "No changes"]} onAction={() => save.mutateAsync()}>Save access</ActionButton>}>
      {isLoading || !data || !cat.data ? <Loading /> : (
        <div className="space-y-4">
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <span className="text-muted-foreground">{t("Role")}:</span> <Pill tone="primary">{data.role}</Pill>
            {data.administrator && <span className="text-xs text-muted-foreground">{t("Full access — exceptions do not apply to administrators.")}</span>}
          </div>
          <Field label="Default branch" hint="Opened automatically after sign-in when the user works at several branches">
            <Select value={defaultBranch} onChange={setDefaultBranch}>
              <option value="">{t("None — choose at sign-in")}</option>
              {userBranches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
            </Select>
          </Field>
          {!data.administrator && (
            <>
              <div className="rounded-xl border p-3">
                <p className="font-medium">{t("Data visibility")}</p>
                <p className="mb-2 text-xs text-muted-foreground">{t("An exception replaces the role's scope for that area only.")}</p>
                <div className="grid gap-2 sm:grid-cols-2">
                  {data.scopes.map((sc) => (
                    <Field key={sc.area} label={sc.label} hint={`${t("Effective")}: ${t(SCOPE_LABEL[sc.effective] ?? sc.effective)}`}>
                      <Select value={scopeOf(sc.area)} onChange={(v) => setScope(sc.area, v)}>
                        <option value="">{`${t("As role")}${sc.role ? ` (${t(SCOPE_LABEL[sc.role])})` : ""}`}</option>
                        {cat.data!.scopes.map((s) => <option key={s.key} value={s.key}>{t(s.label)}</option>)}
                      </Select>
                    </Field>
                  ))}
                </div>
              </div>
              <div className="rounded-xl border p-3">
                <p className="font-medium">{t("Permission exceptions")}</p>
                <p className="mb-2 text-xs text-muted-foreground">{t("Allow adds a permission the role lacks; Restrict removes one the role has. You can only give what you hold yourself.")}</p>
                <ul className="divide-y">
                  {data.grantable.map((p) => {
                    const fromRole = data.role_permissions.includes(p);
                    const v = grantOf(p);
                    return (
                      <li key={p} className="flex flex-wrap items-center gap-2 py-2 text-sm">
                        <span className="min-w-0 flex-1">{t(GRANT_LABELS[p] ?? p)}{fromRole && <span className="text-xs text-muted-foreground"> · {t("from role")}</span>}</span>
                        <div className="flex gap-1" role="radiogroup" aria-label={t(GRANT_LABELS[p] ?? p)}>
                          {[["", "As role"], ["allow", "Allow"], ["restrict", "Restrict"]].map(([k, l]) => (
                            <button key={k} type="button" role="radio" aria-checked={v === k} onClick={() => setGrant(p, k)}
                              className={cn("h-7 rounded-full border px-2.5 text-xs", v === k ? (k === "restrict" ? "border-destructive bg-destructive text-destructive-foreground" : "border-primary bg-primary text-primary-foreground") : "bg-card")}>
                              {t(l)}
                            </button>
                          ))}
                        </div>
                      </li>
                    );
                  })}
                </ul>
              </div>
            </>
          )}
        </div>
      )}
    </ResponsiveDialog>
  );
}
