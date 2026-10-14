import { useCallback, useEffect, useId, useState, type FormEvent, type ReactNode } from "react";
import { useIsDesktop } from "@/lib/hooks";
import { useUnsaved } from "@/lib/unsaved";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Drawer, DrawerContent, DrawerDescription, DrawerFooter, DrawerHeader, DrawerTitle } from "@/components/ui/drawer";
import { t, tx } from "@/lib/i18n";

/**
 * Bottom sheet on phones, centred dialog on tablets and desktops.
 *
 * Dismissal (roadmap 104): clicking or tapping outside never closes it. The ×, Escape and (on phones, when nothing is
 * typed) swiping down ask to close; when something has been typed into it and not saved, they first ask
 * *Discard changes?*. Typed input is noticed automatically; `dirty` overrides that, `confirmDiscard={false}` turns the
 * question off (a dialog whose inputs are not data, e.g. a search). Closing it from the screen (after a save, or a
 * screen's own Cancel) is unchanged.
 */
export function ResponsiveDialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  wide,
  dirty,
  confirmDiscard = true,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  /** Unsaved changes, when the screen knows better than the typed-input check. */
  dirty?: boolean;
  confirmDiscard?: boolean;
}) {
  const desktop = useIsDesktop();
  const key = useId();
  const [typed, setTyped] = useState(false);
  const [asking, setAsking] = useState(false);
  const unsaved = confirmDiscard && open && (dirty ?? typed);
  useUnsaved(`dialog:${key}`, unsaved);

  // A fresh start each time it opens.
  useEffect(() => {
    if (!open) {
      setTyped(false);
      setAsking(false);
    }
  }, [open]);

  const requestClose = useCallback(() => {
    if (unsaved) setAsking(true);
    else onOpenChange(false);
  }, [unsaved, onOpenChange]);
  const change = (o: boolean) => (o ? onOpenChange(true) : requestClose());
  // Something typed, chosen in a native control or a file picked (bubbles from anything rendered inside).
  const noticeInput = (e: FormEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest?.("[data-dirty-ignore]") || (el as HTMLInputElement).type === "search") return;
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) setTyped(true);
  };
  const escape = (e: KeyboardEvent) => {
    if (unsaved) {
      e.preventDefault();
      setAsking(true);
    }
  };

  const discard = asking && (
    <Dialog open onOpenChange={(o) => !o && setAsking(false)}>
      <DialogContent hideClose className="w-[calc(100%-2rem)] max-w-sm gap-3 rounded-2xl p-5" role="alertdialog" onOpenAutoFocus={(e) => e.preventDefault()}>
        <DialogHeader className="text-start">
          <DialogTitle>{t("Discard changes?")}</DialogTitle>
          <DialogDescription>{t("What you entered here has not been saved. Close and lose it?")}</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <Button variant="outline" autoFocus onClick={() => setAsking(false)}>{t("Keep editing")}</Button>
          <Button variant="destructive" onClick={() => { setAsking(false); onOpenChange(false); }}>{t("Discard")}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );

  if (desktop) {
    return (
      <Dialog open={open} onOpenChange={change}>
        <DialogContent className={cn("max-h-[90vh] gap-0 overflow-hidden p-0", wide ? "max-w-3xl" : "max-w-lg")} onInputCapture={noticeInput} onChangeCapture={noticeInput}>
          <DialogHeader className="border-b py-4 pe-12 ps-6 text-start">
            <DialogTitle>{tx(title)}</DialogTitle>
            {description ? <DialogDescription>{tx(description)}</DialogDescription> : <DialogDescription className="sr-only">{tx(title)}</DialogDescription>}
          </DialogHeader>
          <div className="max-h-[calc(90vh-9rem)] overflow-y-auto px-6 py-4">{children}</div>
          {footer && <DialogFooter className="border-t bg-muted/30 px-6 py-3">{footer}</DialogFooter>}
        </DialogContent>
        {discard}
      </Dialog>
    );
  }
  return (
    // Swiping down closes only while nothing unsaved would be lost; otherwise the × asks first.
    <Drawer open={open} onOpenChange={change} dismissible={!unsaved}>
      <DrawerContent className="max-h-[94vh]" onClose={requestClose} onEscapeKeyDown={escape} onInputCapture={noticeInput} onChangeCapture={noticeInput}>
        <DrawerHeader className="pe-12 text-start">
          <DrawerTitle>{tx(title)}</DrawerTitle>
          {description ? <DrawerDescription>{tx(description)}</DrawerDescription> : <DrawerDescription className="sr-only">{tx(title)}</DrawerDescription>}
        </DrawerHeader>
        <div className="overflow-y-auto px-4 pb-4">{children}</div>
        {footer && <DrawerFooter className="pb-safe border-t pt-3">{footer}</DrawerFooter>}
      </DrawerContent>
      {discard}
    </Drawer>
  );
}
