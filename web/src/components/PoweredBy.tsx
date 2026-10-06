import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import mark from "@/assets/sshop-mark.png";

/** Platform credit: the business's own brand stays top-left; this quietly says S'Shop runs the app. */
export function PoweredBy({ className }: { className?: string }) {
  return (
    <span className={cn("inline-flex select-none items-center gap-1.5 px-2 py-1 text-[0.7rem] text-muted-foreground", className)}>
      <span>{t("Powered by")}</span>
      <img src={mark} alt="" className="h-4 w-4" />
      <span className="text-brand font-semibold tracking-tight">{t("S'Shop")}</span>
    </span>
  );
}
