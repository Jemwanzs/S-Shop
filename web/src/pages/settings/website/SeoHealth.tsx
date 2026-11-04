/** Analytics & SEO Health and search-engine set-up (roadmap 160). Only what S'Shop can really count: no scores, rankings or traffic
 * estimates. A search engine's own figures stay in its own tools, which the business connects itself; nothing here pretends to be connected. */
import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, ExternalLink } from "lucide-react";
import { api } from "@/lib/api";
import { count } from "@/lib/format";
import { t } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { Loading } from "@/components/Page";
import { StatCard as Stat } from "@/components/Stat";
import { Block } from "./kit";
import type { TabProps } from "./Website";

interface Issue { level: "high" | "medium" | "low"; code: string; message: string; count: number; tab: string; items: string[] }
interface Health {
  live: boolean; public_url: string; indexing: boolean; sitemap_url: string; pages: number; products: number; categories: number;
  articles: { published_30_days: number; by_status: Record<string, number> }; visits_30_days: number; issues: Issue[];
}
const TONE: Record<Issue["level"], Tone> = { high: "danger", medium: "warning", low: "neutral" };
const LEVEL: Record<Issue["level"], string> = { high: "Fix first", medium: "Worth fixing", low: "Good to have" };

export function HealthTab({ onOpen }: { onOpen: (tab: string) => void }) {
  const { data, isLoading } = useQuery({ queryKey: ["website-seo-health"], queryFn: () => api<Health>("/website/seo/health") });
  if (isLoading || !data) return <Loading />;
  if (!data.live) return <Block title="SEO health"><p className="text-sm text-muted-foreground">{t("Publish your website to see its health.")}</p></Block>;
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat label="Pages for search engines" value={count(data.pages)} />
        <Stat label="Products" value={count(data.products)} />
        <Stat label="Articles, last 30 days" value={count(data.articles.published_30_days)} />
        <Stat label="Website visits, last 30 days" value={count(data.visits_30_days)} />
      </div>
      <Block title="What to look at" hint="Counted from your live website. Publishing changes updates this.">
        {data.issues.length === 0 ? (
          <p className="flex items-center gap-2 text-sm text-success"><CheckCircle2 className="h-4 w-4" /> {t("Nothing to fix right now.")}</p>
        ) : (
          <ul className="divide-y">
            {data.issues.map((i) => (
              <li key={i.code} className="space-y-1 py-3">
                <div className="flex flex-wrap items-center gap-2">
                  <Pill tone={TONE[i.level]}>{t(LEVEL[i.level])}</Pill>
                  <span className="text-sm font-medium">{t(i.message)}{i.count > 1 ? ` (${count(i.count)})` : ""}</span>
                </div>
                {i.items.length > 0 && <p className="line-clamp-2 text-xs text-muted-foreground">{i.items.join(" · ")}</p>}
                <Button size="sm" variant="ghost" onClick={() => onOpen(i.tab)}>{t("Open")}</Button>
              </li>
            ))}
          </ul>
        )}
      </Block>
      <Block title="Search performance" hint="S'Shop does not estimate rankings or search traffic. The real figures are in your search engine's own tools.">
        <ul className="space-y-2 text-sm">
          <li><a className="inline-flex items-center gap-1.5 text-primary hover:underline" href="https://search.google.com/search-console" target="_blank" rel="noreferrer"><ExternalLink className="h-3.5 w-3.5" /> {t("Clicks and positions: Google Search Console")}</a></li>
          <li><a className="inline-flex items-center gap-1.5 text-primary hover:underline" href={`https://pagespeed.web.dev/analysis?url=${encodeURIComponent(data.public_url)}`} target="_blank" rel="noreferrer"><ExternalLink className="h-3.5 w-3.5" /> {t("Mobile speed: test this website on Google PageSpeed Insights")}</a></li>
          <li><a className="inline-flex items-center gap-1.5 text-primary hover:underline" href={`https://search.google.com/test/rich-results?url=${encodeURIComponent(data.public_url)}`} target="_blank" rel="noreferrer"><ExternalLink className="h-3.5 w-3.5" /> {t("Structured data: test this website on Google's Rich Results Test")}</a></li>
        </ul>
      </Block>
    </div>
  );
}

/** Set-up that the business does itself, in each service's own account. A code here only proves ownership or starts counting: the service decides the rest. */
export function SearchEnginesBlock({ c, set, ov }: TabProps) {
  const seo = c.seo;
  const sitemap = `${ov.public_url}/sitemap.xml`;
  return (
    <>
      <Block title="Google Search Console" hint="See how people find you on Google. You do this in your own Google account.">
        <ol className="list-decimal space-y-1 ps-5 text-sm">
          <li>{t("Open Search Console, add a URL-prefix property with your main address, and choose HTML tag.")}</li>
          <li>{t("Paste only the code from the content part of that tag below, then Publish.")}</li>
          <li>{t("Press Verify on Google, then submit your sitemap:")} <span className="break-all font-mono text-xs">{sitemap}</span></li>
        </ol>
        <Field label="Google verification code" optional><Input value={seo.google_verification ?? ""} maxLength={100} onChange={(e) => set((x) => { x.seo.google_verification = e.target.value.trim(); })} /></Field>
        <Button size="sm" variant="outline" asChild><a href="https://search.google.com/search-console" target="_blank" rel="noreferrer"><ExternalLink /> {t("Open Search Console")}</a></Button>
      </Block>
      <Block title="Bing Webmaster Tools" hint="The same for Bing, which can also import your Search Console set-up.">
        <Field label="Bing verification code" optional><Input value={seo.bing_verification ?? ""} maxLength={100} onChange={(e) => set((x) => { x.seo.bing_verification = e.target.value.trim(); })} /></Field>
        <Button size="sm" variant="outline" asChild><a href="https://www.bing.com/webmasters" target="_blank" rel="noreferrer"><ExternalLink /> {t("Open Bing Webmaster Tools")}</a></Button>
      </Block>
      <Block title="Google Analytics 4" hint="Optional. It runs only for visitors who accept analytics cookies, so switch on the cookie choice in Content first.">
        <Field label="Measurement ID" optional hint="Starts with G-"><Input value={seo.ga4_id ?? ""} maxLength={24} placeholder="G-" onChange={(e) => set((x) => { x.seo.ga4_id = e.target.value.trim().toUpperCase(); })} /></Field>
      </Block>
      <Block title="Google Merchant Center" hint="Optional, for showing products on Google Shopping. You do this in your own account, and Google decides what is eligible.">
        <p className="text-sm">{t("You will need products with real prices, your delivery and return terms on your website, and your verified address. Your product pages already carry their details for Google to read. S'Shop does not send a product feed yet.")}</p>
        <Button size="sm" variant="outline" asChild><a href="https://merchants.google.com" target="_blank" rel="noreferrer"><ExternalLink /> {t("Open Merchant Center")}</a></Button>
      </Block>
    </>
  );
}
