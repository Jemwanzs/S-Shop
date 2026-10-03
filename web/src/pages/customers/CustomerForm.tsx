import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/api";
import { phone } from "@/lib/format";
import type { Customer, CustomerField } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Field, NativeSelect } from "@/components/Form";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

/** Default fields (mobile, first name, nickname …) plus the tenant's configured custom fields. */
export function CustomerForm({ open, onOpenChange, customer, onSaved }: { open: boolean; onOpenChange: (o: boolean) => void; customer?: Customer | null; onSaved?: (id: string) => void }) {
  const qc = useQueryClient();
  const fields = useQuery({ queryKey: ["customer-fields"], queryFn: () => api<CustomerField[]>("/customer-fields") });
  const [mobile, setMobile] = useState("");
  const [firstName, setFirstName] = useState("");
  const [otherNames, setOtherNames] = useState("");
  const [nickname, setNickname] = useState("");
  const [email, setEmail] = useState("");
  const [custom, setCustom] = useState<Record<string, unknown>>({});

  useEffect(() => {
    if (!open) return;
    setMobile(customer ? phone(customer.mobile) : "");
    setFirstName(customer?.first_name ?? "");
    setOtherNames(customer?.other_names ?? "");
    setNickname(customer?.nickname ?? "");
    setEmail(customer?.email ?? "");
    setCustom(customer?.custom_fields ?? {});
  }, [open, customer]);

  const save = useMutation({
    mutationFn: () =>
      api<{ id?: string }>(customer ? `/customers/${customer.id}` : "/customers", {
        method: customer ? "PUT" : "POST",
        body: { mobile, first_name: firstName, other_names: otherNames, nickname, email, custom_fields: custom },
      }),
    onSuccess: (r) => {
      toast.success(customer ? "Customer updated" : "Customer added");
      qc.invalidateQueries({ queryKey: ["customers"] });
      qc.invalidateQueries({ queryKey: ["customer", customer?.id] });
      onOpenChange(false);
      onSaved?.(customer?.id ?? r.id!);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const active = (fields.data ?? []).filter((f) => f.is_active);
  const setField = (k: string, v: unknown) => setCustom((c) => ({ ...c, [k]: v }));

  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={onOpenChange}
      title={customer ? "Edit customer" : "New customer"}
      footer={<Button className="w-full md:w-auto" disabled={!mobile.trim() || !firstName.trim() || save.isPending} onClick={() => save.mutate()}>{customer ? "Save" : "Add customer"}</Button>}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Mobile number" className="sm:col-span-2"><Input inputMode="tel" className="num" value={mobile} onChange={(e) => setMobile(e.target.value)} placeholder="07XXXXXXXX" autoFocus={!customer} /></Field>
        <Field label="First name"><Input value={firstName} onChange={(e) => setFirstName(e.target.value)} /></Field>
        <Field label="Other names" optional><Input value={otherNames} onChange={(e) => setOtherNames(e.target.value)} /></Field>
        <Field label="Nickname" optional><Input value={nickname} onChange={(e) => setNickname(e.target.value)} /></Field>
        <Field label="Email" optional><Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} /></Field>
        {active.map((f) => {
          const v = custom[f.key];
          return (
            <Field key={f.id} label={f.label} optional={!f.required}>
              {f.field_type === "dropdown" ? (
                <NativeSelect value={String(v ?? "")} onChange={(x) => setField(f.key, x)}>
                  <option value="">—</option>
                  {f.options.map((o) => <option key={o} value={o}>{o}</option>)}
                </NativeSelect>
              ) : f.field_type === "boolean" ? (
                <div className="flex h-11 items-center"><Switch checked={!!v} onCheckedChange={(c) => setField(f.key, c)} /></div>
              ) : (
                <Input
                  type={f.field_type === "date" ? "date" : f.field_type === "email" ? "email" : "text"}
                  inputMode={f.field_type === "number" ? "decimal" : undefined}
                  value={String(v ?? "")}
                  onChange={(e) => setField(f.key, e.target.value)}
                />
              )}
            </Field>
          );
        })}
      </div>
    </ResponsiveDialog>
  );
}
