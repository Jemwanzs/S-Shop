import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, Loader2 } from "lucide-react";
import { toast } from "@/lib/toast";
import { api, session } from "@/lib/api";
import { useSession } from "@/lib/session";
import { CURRENCIES, FONTS, type Preferences } from "@/lib/prefs";
import { LANGUAGES, t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, SettingsPage } from "./shared";

/** Settings → User preferences: per person, applies on every device they sign in to. */
export function PreferencesSettings() {
  const qc = useQueryClient();
  const { preferences, fx } = useSession();
  const [draft, setDraft] = useState<Preferences>(preferences);
  useEffect(() => setDraft(preferences), [preferences]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(preferences);

  const save = useMutation({
    mutationFn: (p: Preferences) => api<Preferences>("/auth/preferences", { method: "PUT", body: p }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["me", session.token] });
      toast.success("Preferences saved");
    },
    onError: (e) => toast.error(e),
  });

  const rate = (code: string) => {
    const r = fx?.rates[code];
    return r ? `1 ${code} = ${(1 / r).toLocaleString("en-KE", { maximumFractionDigits: 2 })} KES` : null;
  };

  return (
    <SettingsPage title="User preferences" description="Your own settings — they follow you on every device you sign in to.">
      <Card title="Language">
        <div className="grid grid-cols-2 gap-2 py-2 sm:grid-cols-4">
          {LANGUAGES.map((l) => (
            <Choice key={l.code} active={draft.language === l.code} onClick={() => setDraft({ ...draft, language: l.code })}>
              <span className="block font-semibold" dir={l.code === "ar" ? "rtl" : "ltr"}>{l.label}</span>
              <span className="block text-xs uppercase text-muted-foreground">{l.code}</span>
            </Choice>
          ))}
        </div>
      </Card>

      <Card title="Currency">
        <p className="pb-2 text-xs text-muted-foreground">
          {t("Figures across the app are shown in this currency, without symbols. Sales, payments and prices are still recorded and entered in KES.")}
        </p>
        <div className="grid gap-2 py-2 sm:grid-cols-3">
          {CURRENCIES.map((c) => (
            <Choice key={c.code} active={draft.currency === c.code} onClick={() => setDraft({ ...draft, currency: c.code })}>
              <span className="block font-semibold">{c.code}</span>
              <span className="block text-xs text-muted-foreground">{t(c.label)}</span>
              {c.code !== "KES" && rate(c.code) && <span className="num mt-1 block text-[11px] text-muted-foreground">{rate(c.code)}</span>}
            </Choice>
          ))}
        </div>
        {fx && <p className="pt-1 text-[11px] text-muted-foreground">{t("Rates:")} {fx.source}{fx.updated_at ? ` · ${fx.updated_at.replace(/ \+0000$/, " UTC")}` : ""}{fx.stale ? " · last known rates" : ""}</p>}
      </Card>

      <Card title="Font">
        <div className="grid grid-cols-2 gap-2 py-2 sm:grid-cols-3">
          {FONTS.map((f) => (
            <Choice key={f} active={draft.font === f} onClick={() => setDraft({ ...draft, font: f })}>
              <span className="block text-base font-semibold" style={{ fontFamily: `"${f}", system-ui` }}>{f}</span>
              <span className="block text-xs text-muted-foreground" style={{ fontFamily: `"${f}", system-ui` }}>Aa 123 · 4,500{f === "Outfit" ? " · default" : ""}</span>
            </Choice>
          ))}
        </div>
      </Card>

      {dirty && (
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={() => setDraft(preferences)}>{t("Discard")}</Button>
          <Button disabled={save.isPending} onClick={() => save.mutate(draft)}>{save.isPending ? <Loader2 className="animate-spin" /> : t("Save preferences")}</Button>
        </div>
      )}
    </SettingsPage>
  );
}

function Choice({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn("relative rounded-xl border p-3 text-start transition-colors", active ? "border-primary bg-primary/5 ring-1 ring-primary" : "hover:bg-accent/50")}
    >
      {active && <Check className="absolute end-2.5 top-2.5 h-4 w-4 text-primary" />}
      {children}
    </button>
  );
}
