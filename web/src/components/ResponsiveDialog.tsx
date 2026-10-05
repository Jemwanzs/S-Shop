import type { ReactNode } from "react";
import { useIsDesktop } from "@/lib/hooks";
import { cn } from "@/lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Drawer, DrawerContent, DrawerDescription, DrawerFooter, DrawerHeader, DrawerTitle } from "@/components/ui/drawer";
import { tx } from "@/lib/i18n";

/** Bottom sheet on phones, centred dialog on tablets and desktops. */
export function ResponsiveDialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  wide,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}) {
  const desktop = useIsDesktop();
  if (desktop) {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className={cn("max-h-[90vh] gap-0 overflow-hidden p-0", wide ? "max-w-3xl" : "max-w-lg")}>
          <DialogHeader className="border-b px-6 py-4 text-start">
            <DialogTitle>{tx(title)}</DialogTitle>
            {description ? <DialogDescription>{tx(description)}</DialogDescription> : <DialogDescription className="sr-only">{tx(title)}</DialogDescription>}
          </DialogHeader>
          <div className="max-h-[calc(90vh-9rem)] overflow-y-auto px-6 py-4">{children}</div>
          {footer && <DialogFooter className="border-t bg-muted/30 px-6 py-3">{footer}</DialogFooter>}
        </DialogContent>
      </Dialog>
    );
  }
  return (
    <Drawer open={open} onOpenChange={onOpenChange}>
      <DrawerContent className="max-h-[94vh]">
        <DrawerHeader className="text-start">
          <DrawerTitle>{tx(title)}</DrawerTitle>
          {description ? <DrawerDescription>{tx(description)}</DrawerDescription> : <DrawerDescription className="sr-only">{tx(title)}</DrawerDescription>}
        </DrawerHeader>
        <div className="overflow-y-auto px-4 pb-4">{children}</div>
        {footer && <DrawerFooter className="pb-safe border-t pt-3">{footer}</DrawerFooter>}
      </DrawerContent>
    </Drawer>
  );
}
