import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { CustomField } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Field, Select } from "./Form";
import { KV } from "./Page";

export type FieldKind = "customer" | "product";

/** Field definitions configured in Settings for customers or products. */
export function useCustomFields(kind: FieldKind) {
  return useQuery({ queryKey: [`${kind}-fields`], queryFn: () => api<CustomField[]>(`/${kind}-fields`) });
}

/** Inputs for every active custom field; values keyed by field key. */
export function CustomFieldInputs({ kind, values, onChange }: { kind: FieldKind; values: Record<string, unknown>; onChange: (v: Record<string, unknown>) => void }) {
  const { data } = useCustomFields(kind);
  const set = (k: string, v: unknown) => onChange({ ...values, [k]: v });
  return (
    <>
      {(data ?? []).filter((f) => f.is_active).map((f) => {
        const v = values[f.key];
        return (
          <Field key={f.id} label={f.label} optional={!f.required}>
            {f.field_type === "dropdown" ? (
              <Select value={String(v ?? "")} onChange={(x) => set(f.key, x)}>
                <option value="">—</option>
                {f.options.map((o) => <option key={o} value={o}>{o}</option>)}
              </Select>
            ) : f.field_type === "boolean" ? (
              <div className="flex h-11 items-center"><Switch checked={!!v} onCheckedChange={(c) => set(f.key, c)} /></div>
            ) : (
              <Input
                type={f.field_type === "date" ? "date" : f.field_type === "email" ? "email" : "text"}
                inputMode={f.field_type === "number" ? "decimal" : undefined}
                value={String(v ?? "")}
                onChange={(e) => set(f.key, e.target.value)}
              />
            )}
          </Field>
        );
      })}
    </>
  );
}

/** Read-only rows for the custom fields that have a value. */
export function CustomFieldValues({ kind, values }: { kind: FieldKind; values: Record<string, unknown> }) {
  const { data } = useCustomFields(kind);
  return (
    <>
      {(data ?? [])
        .filter((f) => f.is_active && values[f.key] !== undefined && values[f.key] !== "")
        .map((f) => (
          <KV key={f.id} label={f.label}>{f.field_type === "boolean" ? (values[f.key] ? "Yes" : "No") : String(values[f.key])}</KV>
        ))}
    </>
  );
}
