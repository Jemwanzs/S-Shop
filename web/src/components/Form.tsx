import { createContext, useState, type ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "./ResponsiveDialog";
import { t, tx } from "@/lib/i18n";

/** The label of the enclosing Field, so a dropdown inside it names its sheet (see Select). */
export const FieldLabel = createContext<string | undefined>(undefined);

export function Field({ label, hint, optional, children, className }: { label: ReactNode; hint?: ReactNode; optional?: boolean; children: ReactNode; className?: string }) {
  return (
    <label className={cn("block space-y-1.5", className)}>
      <span className="flex items-baseline justify-between gap-2 text-[0.85rem] font-medium">
        {tx(label)}
        {optional && <span className="text-xs font-normal text-muted-foreground">{t("Optional")}</span>}
      </span>
      <FieldLabel.Provider value={typeof label === "string" ? label : undefined}>{children}</FieldLabel.Provider>
      {hint && <span className="block text-xs text-muted-foreground">{tx(hint)}</span>}
    </label>
  );
}

export function ToggleRow({ label, hint, checked, onChange, disabled }: { label: ReactNode; hint?: ReactNode; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <div className="flex items-start justify-between gap-4 py-2.5">
      <div className="min-w-0">
        <div className="text-sm font-medium">{tx(label)}</div>
        {hint && <div className="text-xs text-muted-foreground">{tx(hint)}</div>}
      </div>
      <Switch checked={checked} onCheckedChange={onChange} disabled={disabled} />
    </div>
  );
}

/** Every dropdown in the app (see components/Select.tsx). */
export { Select } from "@/components/Select";

/** Confirmation with an optional mandatory reason (cancellations, write-offs …). */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel = "Confirm",
  destructive,
  requireReason,
  busy,
  onConfirm,
  children,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title: string;
  description?: ReactNode;
  confirmLabel?: string;
  destructive?: boolean;
  requireReason?: boolean;
  busy?: boolean;
  onConfirm: (reason: string) => void;
  children?: ReactNode;
}) {
  const [reason, setReason] = useState("");
  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={(o) => {
        if (!o) setReason("");
        onOpenChange(o);
      }}
      title={title}
      description={description}
      footer={
        <div className="flex w-full gap-2 md:w-auto">
          <Button variant="outline" className="flex-1 md:flex-none" onClick={() => onOpenChange(false)}>
            Back
          </Button>
          <Button
            variant={destructive ? "destructive" : "default"}
            className="flex-1 md:flex-none"
            disabled={busy || (requireReason && !reason.trim())}
            onClick={() => onConfirm(reason.trim())}
          >
            {busy ? "Working…" : confirmLabel}
          </Button>
        </div>
      }
    >
      <div className="space-y-4">
        {children}
        {requireReason && (
          <Field label="Reason">
            <Textarea value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Why is this needed?" autoFocus />
          </Field>
        )}
      </div>
    </ResponsiveDialog>
  );
}
