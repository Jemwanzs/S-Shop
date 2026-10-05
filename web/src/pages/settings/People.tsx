import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, LocateFixed, Pencil, Plus } from "lucide-react";
import { currentPosition } from "@/lib/location";
import { toast } from "@/lib/toast";
import { api, errorMessage } from "@/lib/api";
import { useSession } from "@/lib/session";
import { ago, initials } from "@/lib/format";
import type { Hours, Role, UserRow } from "@/lib/types";
import { HoursEditor, hoursLabel } from "@/components/Hours";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, NativeSelect, ToggleRow } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { PasswordInput } from "@/components/PasswordInput";
import { Card, SettingsPage } from "./shared";
import { t } from "@/lib/i18n";

// ───────────────────────────── Branches ─────────────────────────────

interface BranchRow { id: string; name: string; code: string; location: string; phone: string; manager_id: string | null; manager_name: string | null; is_active: boolean; user_count: number; hours: Hours | null;
  latitude: number | string | null; longitude: number | string | null; geofence_radius_m: number; geofence_enabled: boolean }

/** Coordinates are edited as text (so "-1." can become "-1.28"), converted when saved. */
const coord = (v: number | string | null | undefined) => (v == null || String(v).trim() === "" || Number.isNaN(Number(v)) ? null : Number(v));

export function BranchesSettings() {
  const qc = useQueryClient();
  const { can, profile } = useSession();
  // Hours are only sent by people who may change them; otherwise the server keeps them as they are.
  const canHours = can("settings.workspace");
  const businessHours = profile?.settings.workspace.hours;
  const [locating, setLocating] = useState(false);
  const { data, isLoading } = useQuery({ queryKey: ["branches"], queryFn: () => api<BranchRow[]>("/branches") });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api<UserRow[]>("/users") });
  const [edit, setEdit] = useState<Partial<BranchRow> | null>(null);
  const save = useMutation({
    mutationFn: (b: Partial<BranchRow>) =>
      api(b.id ? `/branches/${b.id}` : "/branches", { method: b.id ? "PUT" : "POST", body: { name: b.name, code: b.code, location: b.location, phone: b.phone, manager_id: b.manager_id || null, is_active: b.is_active, ...(canHours
            ? {
                hours: b.hours ?? null,
                geofence: { latitude: coord(b.latitude), longitude: coord(b.longitude), radius_m: b.geofence_radius_m ?? 150, enabled: !!b.geofence_enabled },
              }
            : {}) } }),
    onSuccess: () => {
      toast.success("Branch saved");
      setEdit(null);
      qc.invalidateQueries({ queryKey: ["branches"] });
      qc.invalidateQueries({ queryKey: ["me"] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  return (
    <SettingsPage title="Branches" description="Each branch keeps its own stock, sales, orders and expenses." loading={isLoading}>
      <Card action={<Button size="sm" onClick={() => setEdit({ is_active: true })}><Plus /> Add branch</Button>}>
        {data?.map((b) => (
          <div key={b.id} className="flex items-center gap-3 py-3">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 font-medium">{b.name} <Pill>{b.code}</Pill>{!b.is_active && <Pill tone="danger">Inactive</Pill>}{b.geofence_enabled && <Pill tone="success">{t("Geofenced")} · {b.geofence_radius_m} m</Pill>}</div>
              <div className="truncate text-xs text-muted-foreground">{[b.location, b.manager_name && `Manager: ${b.manager_name}`, `${b.user_count} users`].filter(Boolean).join(" · ")}</div>
              {b.hours && <div className="truncate text-xs text-muted-foreground">{t("Own hours")}: {hoursLabel(b.hours)}</div>}
            </div>
            <Button variant="ghost" size="icon-sm" onClick={() => setEdit(b)} aria-label="Edit"><Pencil /></Button>
          </div>
        ))}
      </Card>
      <ResponsiveDialog open={!!edit} onOpenChange={(o) => !o && setEdit(null)} title={edit?.id ? "Edit branch" : "New branch"} footer={<Button className="w-full md:w-auto" disabled={!edit?.name || !edit?.code || save.isPending} onClick={() => edit && save.mutate(edit)}>Save</Button>}>
        {edit && (
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Name"><Input value={edit.name ?? ""} onChange={(e) => setEdit({ ...edit, name: e.target.value })} /></Field>
            <Field label="Code"><Input className="uppercase" value={edit.code ?? ""} onChange={(e) => setEdit({ ...edit, code: e.target.value.toUpperCase() })} /></Field>
            <Field label="Location" optional><Input value={edit.location ?? ""} onChange={(e) => setEdit({ ...edit, location: e.target.value })} /></Field>
            <Field label="Phone" optional><Input value={edit.phone ?? ""} onChange={(e) => setEdit({ ...edit, phone: e.target.value })} /></Field>
            <Field label="Branch manager" optional className="sm:col-span-2" hint="Approves requests when a workflow uses “Branch manager”">
              <NativeSelect value={edit.manager_id ?? ""} onChange={(v) => setEdit({ ...edit, manager_id: v || null })}>
                <option value="">—</option>
                {users.data?.filter((u) => u.is_active).map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
              </NativeSelect>
            </Field>
            {canHours && businessHours && (
              <div className="space-y-3 sm:col-span-2">
                <ToggleRow
                  label="Own trading hours"
                  hint={edit.hours ? "This branch keeps its own days and hours" : `Follows the business: ${hoursLabel(businessHours)}`}
                  checked={!!edit.hours}
                  onChange={(v) => setEdit({ ...edit, hours: v ? { ...businessHours, days: [...businessHours.days] } : null })}
                />
                {edit.hours && <HoursEditor value={edit.hours} onChange={(h) => setEdit({ ...edit, hours: h })} />}
              </div>
            )}
            {canHours && (
              <div className="space-y-3 border-t pt-3 sm:col-span-2">
                <ToggleRow
                  label="Geofencing"
                  hint="When the business requires it (Workspace), chosen actions are accepted only within this distance of the branch"
                  checked={!!edit.geofence_enabled}
                  disabled={coord(edit.latitude) == null || coord(edit.longitude) == null}
                  onChange={(v) => setEdit({ ...edit, geofence_enabled: v })}
                />
                <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
                  <Field label="Latitude"><Input inputMode="decimal" value={edit.latitude ?? ""} onChange={(e) => setEdit({ ...edit, latitude: e.target.value, geofence_enabled: coord(e.target.value) == null ? false : edit.geofence_enabled })} /></Field>
                  <Field label="Longitude"><Input inputMode="decimal" value={edit.longitude ?? ""} onChange={(e) => setEdit({ ...edit, longitude: e.target.value, geofence_enabled: coord(e.target.value) == null ? false : edit.geofence_enabled })} /></Field>
                  <Field label="Radius (m)" className="col-span-2 sm:col-span-1"><Input inputMode="numeric" value={edit.geofence_radius_m ?? 150} onChange={(e) => setEdit({ ...edit, geofence_radius_m: Number(e.target.value.replace(/\D/g, "")) || 0 })} /></Field>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={locating}
                  onClick={async () => {
                    setLocating(true);
                    try {
                      const p = await currentPosition();
                      setEdit({ ...edit, latitude: +p.coords.latitude.toFixed(6), longitude: +p.coords.longitude.toFixed(6) });
                      toast.success(`${t("Location set")} (±${Math.round(p.coords.accuracy)} m)`);
                    } catch (e) {
                      toast.error(errorMessage(e));
                    } finally {
                      setLocating(false);
                    }
                  }}
                >
                  <LocateFixed /> {t("Use my current location")}
                </Button>
              </div>
            )}
            {edit.id && <div className="sm:col-span-2"><ToggleRow label="Active" checked={!!edit.is_active} onChange={(v) => setEdit({ ...edit, is_active: v })} /></div>}
          </div>
        )}
      </ResponsiveDialog>
    </SettingsPage>
  );
}

// ───────────────────────────── Users ─────────────────────────────

interface UserForm { id?: string; name: string; email: string; phone: string; pin: string; role_id: string; all_branches: boolean; branch_ids: string[]; is_active: boolean }

export function UsersSettings() {
  const qc = useQueryClient();
  const { profile } = useSession();
  const { data, isLoading } = useQuery({ queryKey: ["users"], queryFn: () => api<UserRow[]>("/users") });
  const roles = useQuery({ queryKey: ["roles"], queryFn: () => api<Role[]>("/roles") });
  const branches = useQuery({ queryKey: ["branches"], queryFn: () => api<BranchRow[]>("/branches") });
  const [edit, setEdit] = useState<UserForm | null>(null);
  const [resetFor, setResetFor] = useState<UserRow | null>(null);
  const [newPin, setNewPin] = useState("");
  const [pinConfirm, setPinConfirm] = useState("");
  const save = useMutation({
    mutationFn: (u: UserForm) => api(u.id ? `/users/${u.id}` : "/users", { method: u.id ? "PUT" : "POST", body: { ...u, pin: u.id ? undefined : u.pin } }),
    onSuccess: () => { toast.success("User saved"); setEdit(null); qc.invalidateQueries({ queryKey: ["users"] }); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const reset = useMutation({
    mutationFn: () => api(`/users/${resetFor!.id}/reset-pin`, { body: { pin: newPin } }),
    onSuccess: () => { toast.success("PIN reset"); setResetFor(null); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const branchName = (id: string) => branches.data?.find((b) => b.id === id)?.name ?? "";

  return (
    <SettingsPage title="Users" description="Staff sign in with email and PIN. Access comes from their role and branches." loading={isLoading}>
      <Card action={<Button size="sm" onClick={() => { setPinConfirm(""); setEdit({ name: "", email: "", phone: "", pin: "", role_id: roles.data?.find((r) => r.name === "Salesperson")?.id ?? "", all_branches: false, branch_ids: profile?.branches[0] ? [profile.branches[0].id] : [], is_active: true }); }}><Plus /> Add user</Button>}>
        {data?.map((u) => (
          <div key={u.id} className="flex items-center gap-3 py-3">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-primary/15 text-sm font-semibold text-primary">{initials(u.name)}</span>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2 font-medium">{u.name} <Pill tone="primary">{u.role_name}</Pill>{!u.is_active && <Pill tone="danger">Inactive</Pill>}</div>
              <div className="truncate text-xs text-muted-foreground">
                {u.email} · {u.all_branches ? "All branches" : u.branch_ids.map(branchName).join(", ")} · {u.last_login_at ? `active ${ago(u.last_login_at)}` : "never signed in"}
              </div>
            </div>
            <Button variant="ghost" size="icon-sm" onClick={() => { setNewPin(""); setPinConfirm(""); setResetFor(u); }} aria-label="Reset PIN"><KeyRound /></Button>
            <Button variant="ghost" size="icon-sm" onClick={() => setEdit({ id: u.id, name: u.name, email: u.email, phone: u.phone, pin: "", role_id: u.role_id, all_branches: u.all_branches, branch_ids: u.branch_ids, is_active: u.is_active })} aria-label="Edit"><Pencil /></Button>
          </div>
        ))}
      </Card>
      <ResponsiveDialog
        open={!!edit}
        onOpenChange={(o) => !o && setEdit(null)}
        title={edit?.id ? "Edit user" : "New user"}
        footer={<Button className="w-full md:w-auto" disabled={!edit?.name || !edit?.email || !edit?.role_id || (!edit.id && (edit.pin.length < 4 || edit.pin !== pinConfirm)) || save.isPending} onClick={() => edit && save.mutate(edit)}>Save user</Button>}
      >
        {edit && (
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Name"><Input value={edit.name} onChange={(e) => setEdit({ ...edit, name: e.target.value })} /></Field>
            <Field label="Email"><Input type="email" value={edit.email} onChange={(e) => setEdit({ ...edit, email: e.target.value })} /></Field>
            <Field label="Phone" optional><Input value={edit.phone} onChange={(e) => setEdit({ ...edit, phone: e.target.value })} /></Field>
            {!edit.id && <Field label="Login PIN" hint="4–12 characters"><PasswordInput autoComplete="new-password" maxLength={12} value={edit.pin} onChange={(e) => setEdit({ ...edit, pin: e.target.value })} /></Field>}
            {!edit.id && <Field label="Confirm PIN" hint={pinConfirm && pinConfirm !== edit.pin ? "PINs do not match" : undefined}><PasswordInput autoComplete="new-password" maxLength={12} value={pinConfirm} onChange={(e) => setPinConfirm(e.target.value)} /></Field>}
            <Field label="Role" className="sm:col-span-2">
              <NativeSelect value={edit.role_id} onChange={(v) => setEdit({ ...edit, role_id: v })}>
                <option value="">Choose…</option>
                {roles.data?.filter((r) => r.is_active || r.id === edit.role_id).map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
              </NativeSelect>
            </Field>
            <div className="sm:col-span-2">
              <ToggleRow label="All branches" hint="Otherwise restricted to the branches below" checked={edit.all_branches} onChange={(v) => setEdit({ ...edit, all_branches: v })} />
              {!edit.all_branches && (
                <div className="grid gap-2 rounded-xl bg-muted/50 p-3 sm:grid-cols-2">
                  {branches.data?.filter((b) => b.is_active).map((b) => (
                    <label key={b.id} className="flex items-center gap-2 text-sm">
                      <Checkbox checked={edit.branch_ids.includes(b.id)} onCheckedChange={(c) => setEdit({ ...edit, branch_ids: c ? [...edit.branch_ids, b.id] : edit.branch_ids.filter((x) => x !== b.id) })} />
                      {b.name}
                    </label>
                  ))}
                </div>
              )}
              {edit.id && <ToggleRow label="Active" checked={edit.is_active} onChange={(v) => setEdit({ ...edit, is_active: v })} />}
            </div>
          </div>
        )}
      </ResponsiveDialog>
      <ResponsiveDialog open={!!resetFor} onOpenChange={(o) => !o && setResetFor(null)} title={`Reset PIN for ${resetFor?.name}`} footer={<Button className="w-full md:w-auto" disabled={newPin.length < 4 || newPin !== pinConfirm || reset.isPending} onClick={() => reset.mutate()}>Reset PIN</Button>}>
        <div className="space-y-4">
          <Field label="New PIN" hint="4–12 characters. Share it with the user privately"><PasswordInput autoComplete="new-password" maxLength={12} value={newPin} onChange={(e) => setNewPin(e.target.value)} autoFocus /></Field>
          <Field label="Confirm new PIN" hint={pinConfirm && pinConfirm !== newPin ? "PINs do not match" : undefined}><PasswordInput autoComplete="new-password" maxLength={12} value={pinConfirm} onChange={(e) => setPinConfirm(e.target.value)} /></Field>
        </div>
      </ResponsiveDialog>
    </SettingsPage>
  );
}

// ───────────────────────────── Roles ─────────────────────────────

interface PermGroup { module: string; label: string; permissions: [string, string][] }

export function RolesSettings() {
  const qc = useQueryClient();
  const { data, isLoading } = useQuery({ queryKey: ["roles"], queryFn: () => api<Role[]>("/roles") });
  const catalogue = useQuery({ queryKey: ["permissions"], queryFn: () => api<PermGroup[]>("/permissions") });
  const [edit, setEdit] = useState<{ id?: string; name: string; description: string; permissions: string[]; is_active: boolean; user_count: number } | null>(null);
  const save = useMutation({
    mutationFn: () => api(edit!.id ? `/roles/${edit!.id}` : "/roles", { method: edit!.id ? "PUT" : "POST", body: edit }),
    onSuccess: () => { toast.success("Role saved"); setEdit(null); qc.invalidateQueries({ queryKey: ["roles"] }); qc.invalidateQueries({ queryKey: ["me"] }); },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const toggle = (p: string, on: boolean) => edit && setEdit({ ...edit, permissions: on ? [...edit.permissions, p] : edit.permissions.filter((x) => x !== p) });
  return (
    <SettingsPage title="Roles & permissions" description="Permissions are granted per module and action. Users can additionally be restricted by branch." loading={isLoading}>
      <Card action={<Button size="sm" onClick={() => setEdit({ name: "", description: "", permissions: [], is_active: true, user_count: 0 })}><Plus /> New role</Button>}>
        {data?.map((r) => (
          <div key={r.id} className="flex items-center gap-3 py-3">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 font-medium">{r.name}{r.is_system && <Pill tone="primary">Full access</Pill>}{!r.is_active && <Pill>Retired</Pill>}</div>
              <div className="truncate text-xs text-muted-foreground">{r.description} · {r.user_count} user{r.user_count === 1 ? "" : "s"} · {r.is_system ? "all" : r.permissions.length} permissions</div>
            </div>
            {!r.is_system && <Button variant="ghost" size="icon-sm" onClick={() => setEdit({ id: r.id, name: r.name, description: r.description, permissions: r.permissions, is_active: r.is_active, user_count: r.user_count })} aria-label="Edit"><Pencil /></Button>}
          </div>
        ))}
      </Card>
      <ResponsiveDialog open={!!edit} onOpenChange={(o) => !o && setEdit(null)} title={edit?.id ? `Edit ${edit.name}` : "New role"} wide footer={<Button className="w-full md:w-auto" disabled={!edit?.name.trim() || save.isPending} onClick={() => save.mutate()}>Save role</Button>}>
        {edit && (
          <div className="space-y-4">
            <div className="grid gap-3 sm:grid-cols-2">
              <Field label="Role name"><Input value={edit.name} onChange={(e) => setEdit({ ...edit, name: e.target.value })} /></Field>
              <Field label="Description" optional><Input value={edit.description} onChange={(e) => setEdit({ ...edit, description: e.target.value })} /></Field>
            </div>
            {edit.id && (
              <ToggleRow
                label="Active"
                hint={edit.is_active && edit.user_count > 0 ? `${edit.user_count} user(s) have this role — move them before retiring it` : "Retired roles stay on record but cannot be assigned"}
                checked={edit.is_active}
                disabled={edit.is_active && edit.user_count > 0}
                onChange={(v) => setEdit({ ...edit, is_active: v })}
              />
            )}
            <div className="grid gap-3 md:grid-cols-2">
              {catalogue.data?.map((g) => (
                <div key={g.module} className="rounded-xl border p-3">
                  <div className="mb-2 flex items-center justify-between">
                    <span className="font-medium">{g.label}</span>
                    <button className="text-xs text-primary" onClick={() => {
                      const all = g.permissions.every(([k]) => edit.permissions.includes(k));
                      setEdit({ ...edit, permissions: all ? edit.permissions.filter((p) => !g.permissions.some(([k]) => k === p)) : [...new Set([...edit.permissions, ...g.permissions.map(([k]) => k)])] });
                    }}>Toggle all</button>
                  </div>
                  <div className="space-y-2">
                    {g.permissions.map(([key, label]) => (
                      <label key={key} className="flex items-center gap-2 text-sm">
                        <Checkbox checked={edit.permissions.includes(key)} onCheckedChange={(c) => toggle(key, !!c)} />
                        {label}
                      </label>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </ResponsiveDialog>
    </SettingsPage>
  );
}
