import { useState, type ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "./ResponsiveDialog";
import { t, tx } from "@/lib/i18n";

export function Field({ label, hint, optional, children, className }: { label: ReactNode; hint?: ReactNode; optional?: boolean; children: ReactNode; className?: string }) {
  return (
    <label className={cn("block space-y-1.5", className)}>
      <span className="flex items-baseline justify-between gap-2 text-sm font-medium">
        {tx(label)}
        {optional && <span className="text-xs font-normal text-muted-foreground">{t("Optional")}</span>}
      </span>
      {children}
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

/** Native select styled like inputs — reliable on every phone. */
export function NativeSelect({ value, onChange, children, className, disabled }: {
  value: string;
  onChange: (v: string) => void;
  children: ReactNode;
  className?: string;
  disabled?: boolean;
}) {
  return (
    <select
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      className={cn(
        "flex h-11 w-full appearance-none rounded-lg border border-input bg-background bg-[url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%2212%22 height=%2212%22 viewBox=%220 0 24 24%22 fill=%22none%22 stroke=%22%23888%22 stroke-width=%222%22><path d=%22m6 9 6 6 6-6%22/></svg>')] bg-[right_0.75rem_center] bg-no-repeat px-3 pe-9 text-base focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring md:text-sm",
        className,
      )}
    >
      {children}
    </select>
  );
}

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
