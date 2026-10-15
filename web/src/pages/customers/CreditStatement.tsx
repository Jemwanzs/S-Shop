/** Roadmap 124 — a customer's credit statement: pick a period, read it on screen, download or print the A4 PDF. */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Download, Printer } from "lucide-react";
import { api } from "@/lib/api";
import { t } from "@/lib/i18n";
import { date, methodLabel, money } from "@/lib/format";
import { toast } from "@/lib/toast";
import { download } from "@/lib/receipt";
import { lineLabel, periodText, statementFile, statementPdf, type Statement } from "@/lib/statement";
import { useSession } from "@/lib/session";
import { todayIn } from "@/lib/dates";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Chip } from "@/components/Filters";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { ErrorState, Loading } from "@/components/Page";

type Period = "all" | "year" | "90";
const startOf = (p: Period, today: string) => {
  if (p === "all") return undefined;
  if (p === "year") return `${today.slice(0, 4)}-01-01`;
  const d = new Date(`${today}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - 89);
  return d.toISOString().slice(0, 10);
};

export function CreditStatementDialog({ customerId, open, onOpenChange }: { customerId: string; open: boolean; onOpenChange: (o: boolean) => void }) {
  const { profile } = useSession();
  const today = todayIn(profile?.tenant.timezone);
  const [period, setPeriod] = useState<Period>("all");
  const [busy, setBusy] = useState<"pdf" | "print" | null>(null);
  const from = startOf(period, today);
  const q = useQuery({
    queryKey: ["credit-statement", customerId, from],
    queryFn: () => api<Statement>(`/customers/${customerId}/credit-statement`, { query: { from, to: today } }),
    enabled: open,
  });
  const s = q.data;
  const run = async (kind: "pdf" | "print") => {
    if (!s) return;
    setBusy(kind);
    try {
      const pdf = await statementPdf(s);
      if (kind === "pdf") download(pdf, statementFile(s));
      else {
        const url = URL.createObjectURL(pdf);
        const w = window.open(url, "_blank");
        if (!w) download(pdf, statementFile(s));
        setTimeout(() => URL.revokeObjectURL(url), 60_000);
      }
    } catch (e) {
      toast.error(e);
    } finally {
      setBusy(null);
    }
  };

  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange} wide confirmDiscard={false} title="Credit statement"
      description={s ? `${s.customer.name} · ${periodText(s)}` : undefined}
      footer={
        <div className="flex w-full flex-wrap justify-end gap-2">
          <Button variant="outline" disabled={!s || !!busy} onClick={() => void run("print")}><Printer /> {t("Print")}</Button>
          <Button disabled={!s || !!busy} onClick={() => void run("pdf")}><Download /> {t("Download PDF")}</Button>
        </div>
      }>
      <div className="space-y-4">
        <div className="flex flex-wrap gap-1.5">
          {([["all", "All time"], ["year", "This year"], ["90", "Last 90 days"]] as [Period, string][]).map(([k, label]) => (
            <Chip key={k} active={period === k} onClick={() => setPeriod(k)}>{t(label)}</Chip>
          ))}
        </div>
        {q.isLoading ? <Loading /> : q.error ? <ErrorState error={q.error} retry={() => void q.refetch()} /> : s && (
          <>
            <div className="grid grid-cols-2 gap-2 text-sm">
              <div className="rounded-lg bg-muted/50 p-3"><div className="text-xs text-muted-foreground">{t("Opening balance")}</div><div className="num font-semibold">{money(s.opening_balance)}</div></div>
              <div className="rounded-lg bg-muted/50 p-3"><div className="text-xs text-muted-foreground">{t("Closing balance")}</div><div className="num font-semibold">{money(s.closing_balance)}</div></div>
            </div>
            <div className="overflow-x-auto rounded-lg border">
              <table className="w-full text-sm">
                <thead className="bg-muted/50 text-xs text-muted-foreground">
                  <tr><th className="p-2 text-start">{t("Date")}</th><th className="p-2 text-start">{t("Description")}</th><th className="p-2 text-end">{t("Charged")}</th><th className="p-2 text-end">{t("Paid / reduced")}</th><th className="p-2 text-end">{t("Balance")}</th></tr>
                </thead>
                <tbody className="divide-y">
                  {s.lines.length === 0 && <tr><td colSpan={5} className="p-4 text-center text-muted-foreground">{t("No credit activity in this period")}</td></tr>}
                  {s.lines.map((l, i) => (
                    <tr key={i}>
                      <td className="num whitespace-nowrap p-2">{date(l.date)}</td>
                      <td className="p-2"><span className="num text-xs text-muted-foreground">{l.reference}</span><div>{lineLabel(l, t, (m) => t(methodLabel(m)))}</div></td>
                      <td className="num p-2 text-end">{l.charge ? money(l.charge) : ""}</td>
                      <td className={cn("num p-2 text-end", l.credit && "text-success")}>{l.credit ? money(l.credit) : ""}</td>
                      <td className="num p-2 text-end font-medium">{money(l.balance)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {s.open.length > 0 && (
              <div className="space-y-1 text-sm">
                <p className="label-caps">{t("Still to pay")}</p>
                {s.open.map((o) => (
                  <div key={o.receipt_no} className="flex justify-between">
                    <span><span className="num">{o.receipt_no}</span> · {t("due")} {date(o.due_date)} {o.overdue && <span className="text-destructive">· {t("Overdue")}</span>}</span>
                    <span className="num font-medium">{money(o.balance)}</span>
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </ResponsiveDialog>
  );
}
