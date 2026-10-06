import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDown, Plus, Trash2 } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { money } from "@/lib/format";
import type { Role, UserRow } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, Select } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Card, SettingsPage, useSettings, type WorkflowLevel, type WorkflowRow } from "./shared";
import { t } from "@/lib/i18n";

const APPROVER: Record<string, string> = { admin: "Administrator", role: "Role", user: "Specific user", branch_manager: "Branch manager" };
const MAX_LEVELS = 5;

/** Maker-checker: the initiator needs the action's permission; each level must approve in order. */
export function WorkflowSettings() {
  const qc = useQueryClient();
  const { profile } = useSession();
  const { data, isLoading } = useSettings();
  const roles = useQuery({ queryKey: ["roles"], queryFn: () => api<Role[]>("/roles") });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api<UserRow[]>("/users") });
  const categories = useQuery({ queryKey: ["expense-categories"], queryFn: () => api<{ id: string; name: string }[]>("/expense-categories") });
  const [edit, setEdit] = useState<WorkflowRow | null>(null);

  const save = useMutation({
    mutationFn: (w: WorkflowRow) =>
      api(`/settings/workflows/${w.action}`, {
        method: "PUT",
        body: {
          enabled: w.enabled,
          levels: w.levels,
          conditions: w.conditions,
          min_amount: w.min_amount === "" || w.min_amount === null ? null : Number(w.min_amount),
        },
      }),
    onSuccess: () => { toast.success("Workflow saved"); setEdit(null); qc.invalidateQueries({ queryKey: ["settings"] }); },
    onError: (e) => toast.error(e),
  });

  const levelName = (l: WorkflowLevel) =>
    l.approver_type === "role" ? roles.data?.find((r) => r.id === l.approver_role_id)?.name ?? "Role"
    : l.approver_type === "user" ? users.data?.find((u) => u.id === l.approver_user_id)?.name ?? "User"
    : APPROVER[l.approver_type];
  const conditionText = (w: WorkflowRow) => {
    const parts = [];
    if (w.min_amount !== null && w.min_amount !== "") parts.push(`from ${money(w.min_amount)}`);
    if (w.conditions.branch_ids?.length) parts.push(`${w.conditions.branch_ids.length} branch(es)`);
    if (w.conditions.role_ids?.length) parts.push(`${w.conditions.role_ids.length} role(s)`);
    if (w.conditions.category_ids?.length) parts.push(`${w.conditions.category_ids.length} categor(ies)`);
    return parts.length ? ` · ${parts.join(", ")}` : "";
  };
  const setLevel = (i: number, patch: Partial<WorkflowLevel>) =>
    edit && setEdit({ ...edit, levels: edit.levels.map((l, n) => (n === i ? { ...l, ...patch } : l)) });
  const toggleIn = (key: "branch_ids" | "role_ids" | "category_ids", id: string, on: boolean) => {
    if (!edit) return;
    const list = edit.conditions[key] ?? [];
    setEdit({ ...edit, conditions: { ...edit.conditions, [key]: on ? [...list, id] : list.filter((x) => x !== id) } });
  };

  return (
    <SettingsPage
      title="Workflow engine"
      description="Step 1 — the initiator (anyone with the permission). Then each approval level in order. Requesters never approve their own requests, and nobody approves two levels of the same request."
      loading={isLoading}
    >
      <Card>
        {data?.workflows.map((w) => (
          <div key={w.action} className="flex items-center gap-3 py-3">
            <button className="min-w-0 flex-1 text-start" onClick={() => setEdit(structuredClone(w))}>
              <div className="font-medium">{w.label}</div>
              <div className="text-xs text-muted-foreground">
                {w.enabled ? <>{w.levels.map(levelName).join(" → ")}{conditionText(w)}</> : "No approval needed"}
              </div>
            </button>
            {w.enabled && <Pill tone="warning">{w.levels.length > 1 ? `${w.levels.length} levels` : "On"}</Pill>}
            <Switch checked={w.enabled} onCheckedChange={(v) => save.mutate({ ...w, enabled: v })} aria-label={`Toggle ${w.label}`} />
          </div>
        ))}
      </Card>

      <ResponsiveDialog
        open={!!edit}
        onOpenChange={(o) => !o && setEdit(null)}
        title={edit?.label ?? ""}
        wide
        footer={<Button className="w-full md:w-auto" disabled={save.isPending} onClick={() => edit && save.mutate(edit)}>{t("Save")}</Button>}
      >
        {edit && (
          <div className="space-y-6">
            <div className="flex items-center justify-between"><span className="text-sm font-medium">{t("Require approval")}</span><Switch checked={edit.enabled} onCheckedChange={(v) => setEdit({ ...edit, enabled: v })} /></div>

            <section className="space-y-2">
              <p className="label-caps">{t("Approval levels (in order)")}</p>
              {edit.levels.map((l, i) => (
                <div key={i}>
                  {i > 0 && <ArrowDown className="mx-auto my-1 h-4 w-4 text-muted-foreground" />}
                  <div className="flex flex-wrap items-end gap-2 rounded-xl border p-3">
                    <span className="num flex h-11 w-8 items-center justify-center font-semibold text-muted-foreground">{i + 1}</span>
                    <Field label="Approver" className="min-w-40 flex-1">
                      <Select value={l.approver_type} onChange={(v) => setLevel(i, { approver_type: v as WorkflowLevel["approver_type"] })}>
                        {Object.entries(APPROVER).map(([k, n]) => <option key={k} value={k}>{n}</option>)}
                      </Select>
                    </Field>
                    {l.approver_type === "role" && (
                      <Field label="Role" className="min-w-40 flex-1">
                        <Select value={l.approver_role_id ?? ""} onChange={(v) => setLevel(i, { approver_role_id: v || null })}>
                          <option value="">{t("Choose…")}</option>
                          {roles.data?.map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
                        </Select>
                      </Field>
                    )}
                    {l.approver_type === "user" && (
                      <Field label="User" className="min-w-40 flex-1">
                        <Select value={l.approver_user_id ?? ""} onChange={(v) => setLevel(i, { approver_user_id: v || null })}>
                          <option value="">{t("Choose…")}</option>
                          {users.data?.filter((u) => u.is_active).map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
                        </Select>
                      </Field>
                    )}
                    {edit.levels.length > 1 && (
                      <Button variant="ghost" size="icon" onClick={() => setEdit({ ...edit, levels: edit.levels.filter((_, n) => n !== i) })} aria-label={`Remove level ${i + 1}`}><Trash2 /></Button>
                    )}
                  </div>
                </div>
              ))}
              {edit.action === "sale.discount" ? (
                <p className="rounded-lg bg-muted p-3 text-xs text-muted-foreground">Discount approval happens at the counter: one supervisor enters their email and PIN at checkout.</p>
              ) : (
                edit.levels.length < MAX_LEVELS && (
                  <Button variant="outline" size="sm" onClick={() => setEdit({ ...edit, levels: [...edit.levels, { approver_type: "admin" }] })}><Plus /> {t("Add level")}</Button>
                )
              )}
            </section>

            <section className="space-y-4">
              <p className="label-caps">{t("Applies when (leave empty for always)")}</p>
              {edit.uses_amount && (
                <Field label="Amount is at least" optional>
                  <Input inputMode="decimal" className="num" value={edit.min_amount ?? ""} onChange={(e) => setEdit({ ...edit, min_amount: e.target.value.replace(/[^\d.]/g, "") })} />
                </Field>
              )}
              <Checklist title="Branches" items={profile?.branches ?? []} selected={edit.conditions.branch_ids ?? []} onToggle={(id, on) => toggleIn("branch_ids", id, on)} />
              <Checklist title="Requester's role" items={roles.data ?? []} selected={edit.conditions.role_ids ?? []} onToggle={(id, on) => toggleIn("role_ids", id, on)} />
              {edit.uses_category && (
                <Checklist title="Expense categories" items={categories.data ?? []} selected={edit.conditions.category_ids ?? []} onToggle={(id, on) => toggleIn("category_ids", id, on)} />
              )}
            </section>
          </div>
        )}
      </ResponsiveDialog>
    </SettingsPage>
  );
}

function Checklist({ title, items, selected, onToggle }: { title: string; items: { id: string; name: string }[]; selected: string[]; onToggle: (id: string, on: boolean) => void }) {
  return (
    <div>
      <p className="mb-1.5 text-sm font-medium">{title} <span className="font-normal text-muted-foreground">{selected.length ? `· ${selected.length} selected` : "· any"}</span></p>
      <div className="grid gap-2 rounded-xl bg-muted/50 p-3 sm:grid-cols-2">
        {items.map((it) => (
          <label key={it.id} className="flex items-center gap-2 text-sm">
            <Checkbox checked={selected.includes(it.id)} onCheckedChange={(c) => onToggle(it.id, !!c)} />
            {it.name}
          </label>
        ))}
      </div>
    </div>
  );
}
