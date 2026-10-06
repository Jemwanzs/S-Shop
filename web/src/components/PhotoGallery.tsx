import { useState } from "react";
import { ChevronLeft, ChevronRight, ImageOff } from "lucide-react";
import { cn } from "@/lib/utils";
import { ResponsiveDialog } from "./ResponsiveDialog";
import { t } from "@/lib/i18n";

/** Swipeable photo viewer (photos stay hidden in operational screens until opened). */
export function PhotoGallery({ open, onOpenChange, title, urls }: { open: boolean; onOpenChange: (o: boolean) => void; title: string; urls: string[] }) {
  const [i, setI] = useState(0);
  const idx = Math.min(i, Math.max(urls.length - 1, 0));
  return (
    <ResponsiveDialog open={open} onOpenChange={(o) => { if (!o) setI(0); onOpenChange(o); }} title={title} wide>
      {urls.length === 0 ? (
        <div className="flex flex-col items-center gap-2 py-12 text-muted-foreground">
          <ImageOff className="h-8 w-8" /> {t("No photos yet")}
        </div>
      ) : (
        <div className="space-y-3">
          <div className="relative overflow-hidden rounded-xl bg-muted">
            <img src={urls[idx]} alt={title} className="mx-auto max-h-[60vh] w-full object-contain" />
            {urls.length > 1 && (
              <>
                <button className="absolute start-2 top-1/2 -translate-y-1/2 rounded-full bg-background/80 p-2 shadow" onClick={() => setI((idx - 1 + urls.length) % urls.length)} aria-label="Previous photo">
                  <ChevronLeft className="h-5 w-5" />
                </button>
                <button className="absolute end-2 top-1/2 -translate-y-1/2 rounded-full bg-background/80 p-2 shadow" onClick={() => setI((idx + 1) % urls.length)} aria-label="Next photo">
                  <ChevronRight className="h-5 w-5" />
                </button>
              </>
            )}
          </div>
          {urls.length > 1 && (
            <div className="scrollbar-none flex gap-2 overflow-x-auto">
              {urls.map((u, n) => (
                <button key={u} onClick={() => setI(n)} className={cn("h-16 w-16 shrink-0 overflow-hidden rounded-lg border-2", n === idx ? "border-primary" : "border-transparent opacity-70")}>
                  <img src={u} alt="" className="h-full w-full object-cover" loading="lazy" />
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </ResponsiveDialog>
  );
}
