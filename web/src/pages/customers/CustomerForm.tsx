import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { phone } from "@/lib/format";
import type { Customer } from "@/lib/types";
import { ActionButton, REASONS, isDirty } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Form";
import { CustomFieldInputs } from "@/components/CustomFields";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";

/** Default fields (mobile, first name, nickname …) plus the tenant's configured custom fields. */
/** `initial`: a new customer's details typed elsewhere (the till's customer search, roadmap 184). */
export function CustomerForm({ open, onOpenChange, customer, onSaved, initial }: {
  open: boolean; onOpenChange: (o: boolean) => void; customer?: Customer | null; onSaved?: (id: string, mobile?: string) => void;
  initial?: { mobile?: string; first_name?: string; other_names?: string };
}) {
  const qc = useQueryClient();
  const [mobile, setMobile] = useState("");
  const [firstName, setFirstName] = useState("");
  const [otherNames, setOtherNames] = useState("");
  const [nickname, setNickname] = useState("");
  const [email, setEmail] = useState("");
  const [custom, setCustom] = useState<Record<string, unknown>>({});

  useEffect(() => {
    if (!open) return;
    setMobile(customer ? phone(customer.mobile) : initial?.mobile ?? "");
    setFirstName(customer?.first_name ?? initial?.first_name ?? "");
    setOtherNames(customer?.other_names ?? initial?.other_names ?? "");
    setNickname(customer?.nickname ?? "");
    setEmail(customer?.email ?? "");
    setCustom(customer?.custom_fields ?? {});
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the typed details are read when the form opens
  }, [open, customer]);

  const save = useMutation({
    mutationFn: () =>
      api<{ id?: string; mobile?: string }>(customer ? `/customers/${customer.id}` : "/customers", {
        method: customer ? "PUT" : "POST",
        body: { mobile, first_name: firstName, other_names: otherNames, nickname, email, custom_fields: custom },
      }),
    onSuccess: (r) => {
      toast.success(customer ? "Customer updated" : "Customer added");
      qc.invalidateQueries({ queryKey: ["customers"] });
      qc.invalidateQueries({ queryKey: ["customer", customer?.id] });
      onOpenChange(false);
      onSaved?.(customer?.id ?? r.id!, r.mobile);
    },
    onError: (e) => toast.error(e),
  });


  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={onOpenChange}
      title={customer ? "Edit customer" : "New customer"}
      footer={<ActionButton className="w-full md:w-auto" online busy={save.isPending} busyLabel="Saving…"
        blockedBy={[
          (!mobile.trim() || !firstName.trim()) && REASONS.completeFields,
          customer && !isDirty(
            { m: phone(customer.mobile), f: customer.first_name, o: customer.other_names ?? "", n: customer.nickname ?? "", e: customer.email ?? "", c: customer.custom_fields ?? {} },
            { m: mobile, f: firstName, o: otherNames, n: nickname, e: email, c: custom },
          ) && REASONS.nothingToSave,
        ]}
        onAction={() => save.mutateAsync()}>{customer ? "Save" : "Add customer"}</ActionButton>}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Mobile number" className="sm:col-span-2"><Input inputMode="tel" className="num" value={mobile} onChange={(e) => setMobile(e.target.value)} placeholder="07XXXXXXXX" autoFocus={!customer && !initial?.mobile} /></Field>
        <Field label="First name"><Input value={firstName} onChange={(e) => setFirstName(e.target.value)} autoFocus={!customer && !!initial?.mobile && !initial.first_name} /></Field>
        <Field label="Other names" optional><Input value={otherNames} onChange={(e) => setOtherNames(e.target.value)} /></Field>
        <Field label="Nickname" optional><Input value={nickname} onChange={(e) => setNickname(e.target.value)} /></Field>
        <Field label="Email" optional><Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} /></Field>
        <CustomFieldInputs kind="customer" values={custom} onChange={setCustom} />
      </div>
    </ResponsiveDialog>
  );
}
