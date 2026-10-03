import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { money } from "@/lib/format";
import type { Role, UserRow } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Field, NativeSelect } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Card, SettingsPage, useSettings, type WorkflowRow } from "./shared";

const APPROVER: Record<string, string> = { admin: "Administrator", role: "Role", user: "Specific user", branch_manager: "Branch manager" };

/** Two-stage maker-checker: the initiator needs the action's permission; the approver is configured here. */
export function WorkflowSettings() {
  const qc = useQueryClient();
  const { currency } = useSession();
  const { data, isLoading } = useSettings();
  const roles = useQuery({ queryKey: ["roles"], queryFn: () => api<Role[]>("/roles") });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api<UserRow[]>("/users") });
  const [edit, setEdit] = useState<WorkflowRow | null>(null);
  const save = useMutation({
    mutationFn: (w: WorkflowRow) =>
      api(`/settings/workflows/${w.action}`, {
        method: "PUT",
        body: { enabled: w.enabled, approver_type: w.approver_type, approver_role_id: w.approver_role_id, approver_user_id: w.approver_user_id, min_amount: w.min_amount === "" || w.min_amount === null ? null : Number(w.min_amount) },
      }),
    onSuccess: () => { toast.success("Workflow saved"); setEdit(null); qc.invalidateQueries({ queryKey: ["settings"] }); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const approverName = (w: WorkflowRow) =>
    w.approver_type === "role" ? roles.data?.find((r) => r.id === w.approver_role_id)?.name : w.approver_type === "user" ? users.data?.find((u) => u.id === w.approver_user_id)?.name : APPROVER[w.approver_type];

  return (
    <SettingsPage title="Workflow engine" description="Step 1 — Initiator: anyone with the permission. Step 2 — Approver: configured per action. Makers can never approve their own requests." loading={isLoading}>
      <Card>
        {data?.workflows.map((w) => (
          <div key={w.action} className="flex items-center gap-3 py-3">
            <button className="min-w-0 flex-1 text-left" onClick={() => setEdit({ ...w })}>
              <div className="font-medium">{w.label}</div>
              <div className="text-xs text-muted-foreground">
                {w.enabled ? <>Approver: {approverName(w) ?? "—"}{w.min_amount !== null && ` · from ${money(w.min_amount, currency)}`}</> : "No approval needed"}
              </div>
            </button>
            {w.enabled && <Pill tone="warning">On</Pill>}
            <Switch checked={w.enabled} onCheckedChange={(v) => save.mutate({ ...w, enabled: v })} aria-label={`Toggle ${w.label}`} />
          </div>
        ))}
      </Card>
      <ResponsiveDialog open={!!edit} onOpenChange={(o) => !o && setEdit(null)} title={edit?.label ?? ""} footer={<Button className="w-full md:w-auto" disabled={save.isPending} onClick={() => edit && save.mutate(edit)}>Save</Button>}>
        {edit && (
          <div className="space-y-4">
            <div className="flex items-center justify-between"><span className="text-sm font-medium">Require approval</span><Switch checked={edit.enabled} onCheckedChange={(v) => setEdit({ ...edit, enabled: v })} /></div>
            <Field label="Approver">
              <NativeSelect value={edit.approver_type} onChange={(v) => setEdit({ ...edit, approver_type: v as WorkflowRow["approver_type"] })}>
                {Object.entries(APPROVER).map(([k, l]) => <option key={k} value={k}>{l}</option>)}
              </NativeSelect>
            </Field>
            {edit.approver_type === "role" && (
              <Field label="Role">
                <NativeSelect value={edit.approver_role_id ?? ""} onChange={(v) => setEdit({ ...edit, approver_role_id: v || null })}>
                  <option value="">Choose…</option>
                  {roles.data?.map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
                </NativeSelect>
              </Field>
            )}
            {edit.approver_type === "user" && (
              <Field label="User">
                <NativeSelect value={edit.approver_user_id ?? ""} onChange={(v) => setEdit({ ...edit, approver_user_id: v || null })}>
                  <option value="">Choose…</option>
                  {users.data?.filter((u) => u.is_active).map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
                </NativeSelect>
              </Field>
            )}
            {edit.uses_amount && (
              <Field label="Only from this amount" optional hint="Leave blank to require approval for every amount">
                <Input inputMode="decimal" className="num" value={edit.min_amount ?? ""} onChange={(e) => setEdit({ ...edit, min_amount: e.target.value.replace(/[^\d.]/g, "") })} />
              </Field>
            )}
            {edit.action === "sale.discount" && <p className="rounded-lg bg-muted p-3 text-xs text-muted-foreground">Discount approval happens at the counter: a supervisor enters their email and PIN at checkout.</p>}
          </div>
        )}
      </ResponsiveDialog>
    </SettingsPage>
  );
}
