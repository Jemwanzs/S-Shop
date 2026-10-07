/**
 * Action button state system (roadmap 47). One component for every button that commits or triggers something.
 * Each action states what must be true before it can run; the button derives the rest:
 *
 *   permission (perm) ─┐
 *   connectivity       ├─▶ hidden | unavailable (short reason as its label) | ready | processing | done | ready again
 *   blockers (record / workflow state, unsaved changes, validation, configuration)
 *   backend processing (busy / the action's promise)
 *
 * - The first blocker that is a string disables the button and becomes its label ("Nothing to save", "Select a file").
 * - `onAction` returning a promise drives Processing → Success: repeated clicks are ignored while it runs, success
 *   ("Saved") is shown only after the promise resolves (the server confirmed), and a failure returns the button to
 *   ready so the user can retry — their input is untouched. The server refuses duplicate submissions as well.
 */
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Loader2 } from "lucide-react";
import { Button, type ButtonProps } from "@/components/ui/button";
import { useSession } from "@/lib/session";
import { useOnline } from "@/lib/offline";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/** A reason the action is unavailable, or a falsy value when that condition is met. */
export type Blocker = string | false | null | undefined | 0;

/** Short, shared disabled labels. Use the one that says why nothing can happen right now. */
export const REASONS = {
  nothingToSave: "Nothing to save",
  noChanges: "No changes",
  selectFile: "Select a file",
  completeFields: "Complete required fields",
  enterReason: "Enter a reason",
  awaitingApproval: "Awaiting approval",
  nothingToReceive: "Nothing to receive",
  nothingToDispatch: "Nothing to dispatch",
  noBalance: "No outstanding balance",
  stkNotConfigured: "STK not configured",
  noRecords: "No records to export",
  emptyCart: "Cart is empty",
  offline: "Offline",
  notPermitted: "Not permitted",
} as const;

export interface ActionButtonProps extends Omit<ButtonProps, "children"> {
  children: ReactNode;
  /** Shown only to users holding any of these permissions (module-aware through the session). */
  perm?: string | string[];
  /** Needs the server: unavailable ("Offline") without a connection. */
  online?: boolean;
  /** Conditions that must be met first; the first reason given disables the button and becomes its label. */
  blockedBy?: Blocker[];
  /** Hide (instead of disabling) while blocked — for actions that do not apply in the current state. */
  hideWhenBlocked?: boolean;
  /** Keep the button's own label while blocked (reason in the tooltip) — for secondary buttons next to a primary one. */
  quietReason?: boolean;
  /** Processing driven from outside (e.g. a mutation's isPending). */
  busy?: boolean;
  /** Label while processing: "Saving…", "Processing…", "Uploading…", "Paying…". */
  busyLabel?: string;
  /** Short confirmation after the server confirmed: "Saved". Omit when the screen moves on by itself. */
  doneLabel?: string;
  /** Runs the action; return the promise (e.g. `mutateAsync`) so the button can follow it. */
  onAction?: () => unknown;
}

export function ActionButton({
  children,
  perm,
  online,
  blockedBy = [],
  hideWhenBlocked,
  quietReason,
  busy,
  busyLabel,
  doneLabel,
  onAction,
  onClick,
  disabled,
  className,
  title,
  ...rest
}: ActionButtonProps) {
  const { can } = useSession();
  const isOnline = useOnline();
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);
  const inFlight = useRef(false);
  const mounted = useRef(true);
  useEffect(() => () => void (mounted.current = false), []);
  useEffect(() => {
    if (!done) return;
    const id = setTimeout(() => mounted.current && setDone(false), 1600);
    return () => clearTimeout(id);
  }, [done]);

  const perms = perm === undefined ? [] : Array.isArray(perm) ? perm : [perm];
  if (perms.length && !perms.some((p) => can(p))) return null;

  const reason = (online && !isOnline ? REASONS.offline : undefined) ?? (blockedBy.find((b) => typeof b === "string" && b) as string | undefined);
  if (reason && hideWhenBlocked) return null;
  const processing = !!busy || running;

  const run = async (e: React.MouseEvent<HTMLButtonElement>) => {
    onClick?.(e);
    if (!onAction || e.defaultPrevented) return;
    // A second click before React re-renders the disabled state is ignored here.
    if (inFlight.current) return;
    inFlight.current = true;
    setRunning(true);
    try {
      await onAction();
      if (mounted.current && doneLabel) setDone(true);
    } catch {
      // The caller reports the error (toast); the button simply becomes ready again.
    } finally {
      inFlight.current = false;
      if (mounted.current) setRunning(false);
    }
  };

  let content: ReactNode = children;
  if (processing) {
    content = (
      <>
        <Loader2 className="animate-spin" />
        {busyLabel ? t(busyLabel) : null}
      </>
    );
  } else if (done && doneLabel) {
    content = (
      <>
        <Check />
        {t(doneLabel)}
      </>
    );
  } else if (reason && !quietReason) {
    content = t(reason);
  }

  return (
    <Button
      {...rest}
      className={cn(reason && !processing && !quietReason && "font-normal", className)}
      disabled={disabled || !!reason || processing}
      aria-busy={processing || undefined}
      title={reason ? t(reason) : title}
      onClick={run}
    >
      {content}
    </Button>
  );
}

/** True when the edited value differs from the saved one (forms: "Nothing to save" until something changed). */
export function isDirty(saved: unknown, draft: unknown): boolean {
  return JSON.stringify(saved ?? null) !== JSON.stringify(draft ?? null);
}
