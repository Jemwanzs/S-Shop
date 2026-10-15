/**
 * Roadmap 121 — Settings → Business → Closing the books: nothing dated on or before the closing date can be recorded,
 * changed or cancelled (backdated sales and their lines, stock received, expenses). Today's trading is never blocked.
 */
import { Lock, LockOpen } from "lucide-react";
import { t } from "@/lib/i18n";
import { dayLabel, todayIn } from "@/lib/dates";
import { useSession } from "@/lib/session";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/Form";
import { Card, SettingsPage, useSettingsDraft } from "./shared";

export function BooksSettings() {
  const { profile } = useSession();
  const d = useSettingsDraft();
  const closed = d.draft?.books?.closed_until ?? null;
  const today = todayIn(profile?.tenant.timezone);
  const yesterday = new Date(new Date(`${today}T00:00:00Z`).getTime() - 86_400_000).toISOString().slice(0, 10);
  const set = (v: string | null) => d.update((x) => { x.books = { ...(x.books ?? {}), closed_until: v }; });

  return (
    <SettingsPage title="Closing the books" description="Lock finished periods so their figures can no longer change."
      loading={!d.draft} dirty={d.dirty} saving={d.saving} onSave={d.save} onReset={d.reset}>
      <Card>
        <div className="space-y-4 py-3">
          <p className="flex items-center gap-2 text-sm font-medium">
            {closed ? <Lock className="h-4 w-4 text-warning" /> : <LockOpen className="h-4 w-4 text-muted-foreground" />}
            {closed ? `${t("Books closed up to")} ${dayLabel(closed)}` : t("No period is closed")}
          </p>
          <Field label="Books closed up to" hint="Yesterday at the latest — today's trading is never blocked">
            <div className="flex flex-wrap gap-2">
              <Input type="date" className="num w-48" value={closed ?? ""} max={yesterday} min="2000-01-01"
                onChange={(e) => { const v = e.target.value; if (!v) set(null); else if (v <= yesterday) set(v); }} />
              {closed && <Button variant="outline" onClick={() => set(null)}><LockOpen /> {t("Open the books")}</Button>}
            </div>
          </Field>
          <ul className="list-disc space-y-1 ps-5 text-xs text-muted-foreground">
            <li>{t("Sales cannot be backdated into the closed period, and its sales cannot be cancelled (a return dated today still works).")}</li>
            <li>{t("Stock cannot be received with a date in the closed period.")}</li>
            <li>{t("Expenses cannot be recorded in, or voided from, the closed period.")}</li>
            <li>{t("Every change to the closing date is in the audit trail.")}</li>
          </ul>
        </div>
      </Card>
    </SettingsPage>
  );
}
