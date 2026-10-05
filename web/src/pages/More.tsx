import { useState } from "react";
import { Link } from "react-router-dom";
import { ExternalLink, KeyRound, LogOut, Moon, Sun } from "lucide-react";
import { useSession } from "@/lib/session";
import { initials } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { ChangePin, useTheme } from "@/components/layout/AppShell";
import { allowed, BOTTOM, NAV } from "@/components/layout/nav";

/** Phone/tablet menu for everything not on the bottom bar. */
export default function More() {
  const { profile, can, signOut, branch, displayCurrency } = useSession();
  const { dark, toggle } = useTheme();
  const [pinOpen, setPinOpen] = useState(false);
  const bottom = new Set(BOTTOM.map((b) => b.to));
  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div className="surface flex items-center gap-3 p-4">
        <span className="flex h-12 w-12 items-center justify-center rounded-full bg-primary/15 font-semibold text-primary">{initials(profile?.user.name)}</span>
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold">{profile?.user.name}</div>
          <div className="truncate text-sm text-muted-foreground">{profile?.user.role} · {branch?.name}</div>
          <Link to="/settings/preferences" className="text-xs font-medium text-primary">Figures in {displayCurrency} · Preferences</Link>
        </div>
        <Button variant="ghost" size="icon" onClick={toggle} aria-label="Toggle theme">{dark ? <Sun /> : <Moon />}</Button>
      </div>
      {NAV.map((g) => {
        const items = g.items.filter((i) => allowed(i, can) && !bottom.has(i.to));
        if (!items.length) return null;
        return (
          <section key={g.group}>
            <p className="label-caps mb-2 px-1">{g.group}</p>
            <div className="grid grid-cols-3 gap-2 sm:grid-cols-4">
              {items.map((i) => (
                <Link key={i.to} to={i.to} className="surface flex aspect-square flex-col items-center justify-center gap-2 p-2 text-center text-xs font-medium transition active:scale-95">
                  <span className="rounded-xl bg-primary/10 p-2.5 text-primary"><i.icon className="h-5 w-5" /></span>
                  {i.label}
                </Link>
              ))}
            </div>
          </section>
        );
      })}
      {profile && (
        <a href={`/order/${profile.tenant.slug}`} target="_blank" rel="noreferrer" className="surface flex items-center justify-between p-4 text-sm">
          <span>Customer ordering link<span className="block text-xs text-muted-foreground">/order/{profile.tenant.slug}</span></span>
          <ExternalLink className="h-4 w-4 text-muted-foreground" />
        </a>
      )}
      {(profile?.branches.length ?? 0) > 1 && <Button variant="outline" className="w-full" asChild><Link to="/select-branch">Switch branch</Link></Button>}
      <Button variant="outline" className="w-full" onClick={() => setPinOpen(true)}><KeyRound /> Change PIN</Button>
      <Button variant="outline" className="w-full text-destructive" onClick={signOut}><LogOut /> Sign out</Button>
      <ChangePin open={pinOpen} onOpenChange={setPinOpen} />
    </div>
  );
}
