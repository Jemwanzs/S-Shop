import type { ReactNode } from "react";
import { ThemeToggle } from "@/components/layout/AppShell";
import mark from "@/assets/sshop-mark.png";
import stacked from "@/assets/sshop-logo-stacked.png";
import { t, tx } from "@/lib/i18n";
import { LanguagePicker } from "./LanguagePicker";

/** Compact centred card for the public screens (sign in, request access); brand panel beside it on wide screens. */
export function AuthLayout({ title, subtitle, children }: { title: string; subtitle?: string; children: ReactNode }) {
  return (
    <div className="grid min-h-screen lg:grid-cols-[1.1fr_1fr]">
      {/* Fixed light panel in both themes: the S'Shop logo lettering is dark. */}
      <div className="relative hidden overflow-hidden bg-[#fffaf4] text-[#1c1410] lg:flex lg:flex-col lg:justify-between lg:p-12">
        <div className="bg-brand absolute -end-28 -top-28 h-96 w-96 rounded-full opacity-25 blur-3xl" />
        <div className="bg-brand absolute -bottom-36 -start-24 h-96 w-96 rounded-full opacity-15 blur-3xl" />
        <img src={stacked} alt="S'Shop — Everything you love in one place" className="relative h-auto w-52 xl:w-60" />
        <div className="relative max-w-lg space-y-3">
          <h1 className="text-4xl font-semibold leading-tight">
            {t("Sell, stock, and reward -")} <span className="text-brand">{t("from one phone.")}</span>
          </h1>
          <p className="text-base text-[#1c1410]/70">Inventory, point of sale, customer orders, credit and loyalty for every branch of your business.</p>
        </div>
        <p className="relative text-sm text-[#1c1410]/50">{t("Inventory is the backbone. Customers are the heart.")}</p>
      </div>

      <div className="relative flex min-h-screen flex-col items-center justify-center px-4 py-10">
        <div className="absolute inset-x-3 top-3 flex items-center justify-between">
          <LanguagePicker />
          <ThemeToggle />
        </div>
        <div className="surface w-full max-w-[360px] px-5 py-7 animate-fade-up sm:px-7">
          <div className="mb-6 text-center">
            <img src={mark} alt="" className="mx-auto h-12 w-12" />
            <h2 className="mt-3 text-lg font-semibold">{t(title)}</h2>
            {subtitle && <p className="mt-0.5 text-xs text-muted-foreground">{t(subtitle)}</p>}
          </div>
          {children}
        </div>
      </div>
    </div>
  );
}

export function AuthLabel({ children }: { children: ReactNode }) {
  return <span className="label-caps mb-1.5 block">{tx(children)}</span>;
}
