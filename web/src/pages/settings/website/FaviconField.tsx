/** Favicon (roadmap 161): choose an image, see it as a browser tab and a phone icon, use it. It joins the website's draft and goes live
 * with Publish, like everything else here. Removing it goes back to the logo, or to the first letter of the business. */
import { useEffect, useRef, useState } from "react";
import { ImagePlus, RotateCcw, X } from "lucide-react";
import { api } from "@/lib/api";
import { toast } from "@/lib/toast";
import { t } from "@/lib/i18n";
import { faviconForm, makeFaviconSet, type FaviconSet } from "@/lib/favicon";
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Block } from "./kit";
import type { TabProps } from "./Website";

/** A pretend browser tab, in light and dark, showing the icon at the size a tab really uses. */
function BrowserTab({ icon, title, dark }: { icon: string | null; title: string; dark?: boolean }) {
  return (
    <div className="flex h-9 max-w-56 items-center gap-2 rounded-t-lg px-3 text-xs" style={{ background: dark ? "#35363a" : "#ffffff", color: dark ? "#e8eaed" : "#3c4043", boxShadow: "0 0 0 1px rgba(0,0,0,.12)" }}>
      {icon ? <img src={icon} alt="" width={16} height={16} /> : <span className="flex h-4 w-4 items-center justify-center rounded-sm bg-primary text-[10px] font-bold text-primary-foreground">{title.charAt(0).toUpperCase()}</span>}
      <span className="truncate">{title}</span>
    </div>
  );
}

export function FaviconField({ c, set, ov }: TabProps) {
  const input = useRef<HTMLInputElement>(null);
  const [pending, setPending] = useState<FaviconSet | null>(null);
  const [urls, setUrls] = useState<{ tab: string; phone: string } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!pending) {
      setUrls(null);
      return;
    }
    const next = { tab: URL.createObjectURL(pending.images[32]), phone: URL.createObjectURL(pending.images[180]) };
    setUrls(next);
    return () => { URL.revokeObjectURL(next.tab); URL.revokeObjectURL(next.phone); };
  }, [pending]);
  const saved = c.brand.favicon ?? null;
  const tab = urls?.tab ?? (saved ? `/api/site/favicon/${saved}/32.png` : null);
  const phone = urls?.phone ?? (saved ? `/api/site/favicon/${saved}/180.png` : null);
  const title = c.brand.name || ov.business;
  const choose = async (file: File | undefined) => {
    setProblem(null);
    if (!file) return;
    try {
      setPending(await makeFaviconSet(file));
    } catch (e) {
      setPending(null);
      setProblem(t(e instanceof Error ? e.message : "This image could not be read. Try a PNG."));
    }
  };
  return (
    <Block title="Favicon" hint="The small picture in browser tabs and bookmarks, and the icon when someone adds your website to their phone. A square image works best, ideally 512 × 512 with a transparent background.">
      <div className="flex flex-wrap items-end gap-4">
        <div className="space-y-2 rounded-lg bg-muted/60 p-3">
          <BrowserTab icon={tab} title={title} />
          <BrowserTab icon={tab} title={title} dark />
        </div>
        <div className="text-center">
          {phone ? <img src={phone} alt={t("Phone icon preview")} width={60} height={60} className="rounded-[14px] shadow-sm" /> : <span className="flex h-[60px] w-[60px] items-center justify-center rounded-[14px] bg-primary text-xl font-bold text-primary-foreground">{title.charAt(0).toUpperCase()}</span>}
          <span className="mt-1 block text-xs text-muted-foreground">{t("On a phone")}</span>
        </div>
      </div>
      {pending && pending.notes.map((n) => <p key={n} className="text-xs text-warning">{t(n)}</p>)}
      {problem && <p role="alert" className="rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">{problem}</p>}
      <input ref={input} type="file" hidden aria-label={t("Choose a favicon")} accept=".png,.ico,.svg,.jpg,.jpeg,.webp,image/png,image/x-icon,image/svg+xml,image/jpeg,image/webp"
        onChange={(e) => { void choose(e.target.files?.[0]); e.target.value = ""; }} />
      <div className="flex flex-wrap gap-2">
        {pending ? (
          <>
            <ActionButton size="sm" online onAction={async () => {
              try {
                const r = await api<{ id: string }>("/website/favicon", { body: faviconForm(pending) });
                set((x) => { x.brand.favicon = r.id; });
                setPending(null);
                toast.success("Favicon ready: save or publish to use it");
              } catch (e) {
                toast.error(e);
              }
            }}>{t("Use this favicon")}</ActionButton>
            <Button size="sm" variant="ghost" onClick={() => setPending(null)}><X /> {t("Cancel")}</Button>
          </>
        ) : (
          <>
            <Button size="sm" variant="outline" onClick={() => input.current?.click()}><ImagePlus /> {t(saved ? "Replace favicon" : "Upload favicon")}</Button>
            {saved && <Button size="sm" variant="ghost" onClick={() => set((x) => { x.brand.favicon = null; })}><RotateCcw /> {t("Use the default")}</Button>}
          </>
        )}
      </div>
      <p className="text-xs text-muted-foreground">{t("Browsers and phones keep icons for a while, so a change can take time to show. An app already installed on a phone may keep its old icon until it is added again. Google decides whether to show your favicon in search results.")}</p>
    </Block>
  );
}
