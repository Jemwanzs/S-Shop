/** Roadmap 185: a person's round avatar — their profile photo (never stretched) or their initials. */
import { useState } from "react";
import { initials } from "@/lib/format";
import { cn } from "@/lib/utils";

const SIZES = { xs: "h-7 w-7 text-[11px]", sm: "h-9 w-9 text-sm", md: "h-10 w-10 text-sm", lg: "h-12 w-12 text-base", xl: "h-24 w-24 text-2xl" } as const;

export function UserAvatar({ name, url, size = "sm", className }: { name?: string | null; url?: string | null; size?: keyof typeof SIZES; className?: string }) {
  const [broken, setBroken] = useState<string | null>(null);
  const showPhoto = !!url && broken !== url;
  return (
    <span className={cn("relative flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-primary/15 font-semibold text-primary", SIZES[size], className)}>
      {showPhoto ? <img src={url!} alt="" className="h-full w-full object-cover" onError={() => setBroken(url!)} draggable={false} /> : initials(name)}
    </span>
  );
}
