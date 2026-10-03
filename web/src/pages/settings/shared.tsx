import { useEffect, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/api";
import type { Money, Settings } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Loading } from "@/components/Page";

export interface WorkflowLevel {
  approver_type: "admin" | "role" | "user" | "branch_manager";
  approver_role_id?: string | null;
  approver_user_id?: string | null;
}

export interface WorkflowRow {
  action: string;
  label: string;
  uses_amount: boolean;
  uses_category: boolean;
  enabled: boolean;
  levels: WorkflowLevel[];
  conditions: { branch_ids?: string[]; role_ids?: string[]; category_ids?: string[] };
  min_amount: Money | null;
}

export interface SettingsResponse {
  profile: { name: string; slug: string; tagline: string; phone: string; email: string; address: string; currency: string; timezone: string; logo_url: string | null; portal_url: string };
  settings: Settings;
  workflows: WorkflowRow[];
  integrations: { mpesa_stk: boolean; mpesa_environment: string | null; whatsapp: boolean; whatsapp_webhook_url: string };
}

export function useSettings() {
  return useQuery({ queryKey: ["settings"], queryFn: () => api<SettingsResponse>("/settings") });
}

/** Local editable copy of tenant settings, saved as one document. */
export function useSettingsDraft() {
  const qc = useQueryClient();
  const q = useSettings();
  const [draft, setDraft] = useState<Settings | null>(null);
  useEffect(() => {
    if (q.data) setDraft(structuredClone(q.data.settings));
  }, [q.data]);
  const save = useMutation({
    mutationFn: (s: Settings) => api<Settings>("/settings", { method: "PUT", body: s }),
    onSuccess: () => {
      toast.success("Settings saved");
      qc.invalidateQueries({ queryKey: ["settings"] });
      qc.invalidateQueries({ queryKey: ["me"] });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const dirty = !!draft && !!q.data && JSON.stringify(draft) !== JSON.stringify(q.data.settings);
  const update = (fn: (s: Settings) => void) =>
    setDraft((d) => {
      if (!d) return d;
      const next = structuredClone(d);
      fn(next);
      return next;
    });
  return { query: q, draft, update, dirty, save: () => draft && save.mutate(draft), saving: save.isPending, reset: () => q.data && setDraft(structuredClone(q.data.settings)) };
}

export function SettingsPage({ title, description, children, dirty, saving, onSave, onReset, loading }: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
  dirty?: boolean;
  saving?: boolean;
  onSave?: () => void;
  onReset?: () => void;
  loading?: boolean;
}) {
  if (loading) return <Loading />;
  return (
    <div className="space-y-5 pb-20">
      <div>
        <h2 className="text-xl font-semibold">{title}</h2>
        {description && <p className="mt-1 text-sm text-muted-foreground">{description}</p>}
      </div>
      {children}
      {onSave && dirty && (
        <div className="fixed inset-x-0 bottom-[60px] z-20 border-t bg-background/95 p-3 backdrop-blur animate-fade-up lg:bottom-0 lg:left-[272px]">
          <div className="mx-auto flex max-w-[1680px] items-center justify-end gap-2 px-1 md:px-3 lg:px-5">
            <span className="mr-auto text-sm text-muted-foreground">Unsaved changes</span>
            <Button variant="outline" onClick={onReset}>Discard</Button>
            <Button onClick={onSave} disabled={saving}>{saving ? <Loader2 className="animate-spin" /> : "Save changes"}</Button>
          </div>
        </div>
      )}
    </div>
  );
}

export function Card({ title, children, action }: { title?: string; children: ReactNode; action?: ReactNode }) {
  return (
    <section className="surface p-4 lg:p-5">
      {(title || action) && (
        <div className="mb-2 flex items-center justify-between">
          {title && <h3 className="font-semibold">{title}</h3>}
          {action}
        </div>
      )}
      <div className="divide-y">{children}</div>
    </section>
  );
}
