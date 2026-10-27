import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDown, Plus, ShieldAlert, Trash2, X } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { money } from "@/lib/format";
import type { Role, UserRow } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, Select } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Card, SettingsPage, useSettings, type WorkflowApprover, type WorkflowLevel, type WorkflowRow } from "./shared";
import { t } from "@/lib/i18n";

const APPROVER: Record<string, string> = { admin: "Administrator", role: "Role", user: "Specific user", branch_manager: "Branch manager" };

/** Everyone a level names (older levels hold one approver in their own fields). */
const approversOf = (l: WorkflowLevel): WorkflowApprover[] =>
  l.approvers?.length ? l.approvers : [{ approver_type: l.approver_type, approver_role_id: l.approver_role_id ?? null, approver_user_id: l.approver_user_id ?? null }];
const keyOf = (a: WorkflowApprover) => (a.approver_type === "role" ? `role:${a.approver_role_id}` : a.approver_type === "user" ? `user:${a.approver_user_id}` : a.approver_type);
const fromKey = (k: string): WorkflowApprover =>
  k.startsWith("role:") ? { approver_type: "role", approver_role_id: k.slice(5) } : k.startsWith("user:") ? { approver_type: "user", approver_user_id: k.slice(5) } : { approver_type: k as WorkflowApprover["approver_type"] };
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
    onSuccess: (r) => {
      // Requests already pending follow the changed workflow (roadmap 49).
      const n = (r as { affected_pending?: unknown[] })?.affected_pending?.length ?? 0;
      toast.success(n ? `${t("Workflow saved")} · ${n} ${t("pending requests updated")}` : t("Workflow saved"));
      setEdit(null);
      qc.invalidateQueries({ queryKey: ["settings"] });
      qc.invalidateQueries({ queryKey: ["approvals"] });
    },
    onError: (e) => toast.error(e),
  });

  const approverName = (a: WorkflowApprover) =>
    a.approver_type === "role" ? roles.data?.find((r) => r.id === a.approver_role_id)?.name ?? "Role"
    : a.approver_type === "user" ? users.data?.find((u) => u.id === a.approver_user_id)?.name ?? "User"
    : t(APPROVER[a.approver_type]);
  const levelName = (l: WorkflowLevel) => approversOf(l).map(approverName).join(` ${t("or")} `);
  // Roadmap 150: Tenant Administrator self-approval (on unless the business switched it off; never for money actions).
  const selfApproval = data?.settings.security?.admin_self_approval ?? true;
  const policy = useMutation({
    mutationFn: (on: boolean) => api("/settings/approval-policy", { method: "PUT", body: { admin_self_approval: on } }),
    onSuccess: (_r, on) => {
      toast.success(on ? t("Tenant Administrators may approve their own requests") : t("Tenant Administrators follow maker-checker"));
      qc.invalidateQueries({ queryKey: ["settings"] });
      qc.invalidateQueries({ queryKey: ["approvals"] });
    },
    onError: (e) => toast.error(e),
  });
  const setApprovers = (i: number, list: WorkflowApprover[]) =>
    list.length && setLevel(i, { approvers: list, approver_type: list[0].approver_type, approver_role_id: list[0].approver_role_id ?? null, approver_user_id: list[0].approver_user_id ?? null });
  // Roadmap 148: an Administrator step with a single full-access person can never be approved when that person asks.
  const loneAdmin = (data?.full_access_people ?? 2) <= 1;
  const selfApprovalOn = data?.settings.security?.admin_self_approval ?? true;
  /** A level only full access decides, in a business with one full-access person who cannot approve their own request. */
  const deadlock = (w: WorkflowRow, l: WorkflowLevel) => loneAdmin && (!selfApprovalOn || !!w.self_approval_protected) && approversOf(l).every((a) => a.approver_type === "admin");
  const loneAdminWarning = t("Only one person has full access, and nobody approves their own request — requests they make at this step can never be approved. Choose a role or a specific person here, or give a second person full access.");
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
      description="Step 1 — the initiator (anyone with the permission). Then each approval level in order. Requesters never approve their own requests (a Tenant Administrator may, when allowed below), and nobody approves two levels of the same request."
      loading={isLoading}
    >
      <Card>
        <div className="flex items-start gap-3 py-3">
          <ShieldAlert className="mt-0.5 h-5 w-5 shrink-0 text-warning" />
          <div className="min-w-0 flex-1">
            <div className="font-medium">{t("Allow Tenant Administrator self-approval")}</div>
            <p className="text-xs text-muted-foreground">
              {t("A Tenant Administrator may approve their own request when they are among its approvers — always by pressing Approve, and recorded as self-approved. Never for refunds, cancellations, credit write-offs or recalls, expenses or counter discounts. Switch off for strict separation of duties.")}
            </p>
            {selfApproval && <p className="mt-1 text-xs text-warning">{t("On: a Tenant Administrator can approve their own sensitive requests (product, stock, transfer and import approvals).")}</p>}
          </div>
          <Switch checked={selfApproval} disabled={policy.isPending} onCheckedChange={(v) => policy.mutate(v)} aria-label={t("Allow Tenant Administrator self-approval")} />
        </div>
      </Card>
      <Card>
        {data?.workflows.map((w) => (
          <div key={w.action} className="flex items-center gap-3 py-3">
            <button className="min-w-0 flex-1 text-start" onClick={() => setEdit(structuredClone(w))}>
              <div className="font-medium">{w.label}</div>
              <div className="text-xs text-muted-foreground">
                {w.enabled ? <>{w.levels.map(levelName).join(" → ")}{conditionText(w)}</> : "No approval needed"}
              </div>
              {w.enabled && w.levels.some((l) => deadlock(w, l)) && (
                <div className="mt-1 text-xs text-warning">{t("Only one person can approve the Administrator step")}</div>
              )}
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
        footer={<ActionButton className="w-full md:w-auto" online busy={save.isPending} busyLabel="Saving…" onAction={() => edit && save.mutateAsync(edit)}>{t("Save")}</ActionButton>}
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
                    <div className="min-w-0 flex-1 space-y-2">
                      <p className="text-sm font-medium">{t("Approvers")}</p>
                      <div className="flex flex-wrap gap-1.5">
                        {approversOf(l).map((a) => (
                          <span key={keyOf(a)} className="inline-flex items-center gap-1 rounded-full bg-primary/10 py-1 ps-3 pe-1 text-sm text-primary">
                            {approverName(a)}
                            {approversOf(l).length > 1 && (
                              <button type="button" className="rounded-full p-0.5 hover:bg-primary/15" aria-label={`${t("Remove")} ${approverName(a)}`}
                                onClick={() => setApprovers(i, approversOf(l).filter((x) => keyOf(x) !== keyOf(a)))}><X className="h-3.5 w-3.5" /></button>
                            )}
                          </span>
                        ))}
                      </div>
                      {edit.action !== "sale.discount" && approversOf(l).length < 10 && (
                        <Select value="" onChange={(v) => v && !approversOf(l).some((a) => keyOf(a) === v) && setApprovers(i, [...approversOf(l), fromKey(v)])}>
                          <option value="">{t("Add approver…")}</option>
                          {["admin", "branch_manager"].filter((k) => !approversOf(l).some((a) => keyOf(a) === k)).map((k) => <option key={k} value={k}>{t(APPROVER[k])}</option>)}
                          {roles.data?.filter((r) => !approversOf(l).some((a) => keyOf(a) === `role:${r.id}`)).map((r) => <option key={r.id} value={`role:${r.id}`}>{t("Role")}: {r.name}</option>)}
                          {users.data?.filter((u) => u.is_active && !approversOf(l).some((a) => keyOf(a) === `user:${u.id}`)).map((u) => <option key={u.id} value={`user:${u.id}`}>{t("Person")}: {u.name}</option>)}
                        </Select>
                      )}
                      {edit.action === "sale.discount" && (
                        <Select value={l.approver_type} onChange={(v) => setApprovers(i, [fromKey(v)])}>
                          {Object.entries(APPROVER).filter(([k]) => k !== "role" && k !== "user").map(([k, n]) => <option key={k} value={k}>{t(n)}</option>)}
                        </Select>
                      )}
                      <p className="text-xs text-muted-foreground">{t("Any one eligible approver can act. The first approval or rejection finalises this level.")}</p>
                      {deadlock(edit, l) && <p className="rounded-lg bg-warning/10 p-2 text-xs text-warning">{loneAdminWarning}</p>}
                    </div>
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
