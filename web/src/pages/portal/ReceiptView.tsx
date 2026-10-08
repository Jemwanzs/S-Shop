/** Public receipt by secure link (/r/{token}) — what a customer opens from WhatsApp or email (roadmap 65). */
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Download, Printer } from "lucide-react";
import { api } from "@/lib/api";
import { download, fileName, printReceipt, receiptPdf, type ReceiptSnapshot } from "@/lib/receipt";
import { Button } from "@/components/ui/button";
import { ErrorState, Loading } from "@/components/Page";
import { Receipt } from "@/components/Receipt";
import { t } from "@/lib/i18n";

export default function ReceiptView() {
  const { token } = useParams();
  const { data, isLoading, error } = useQuery({
    queryKey: ["public-receipt", token],
    queryFn: () => api<{ snapshot: ReceiptSnapshot; kind: string }>(`/r/${token}`, { token: null }),
    retry: false,
  });
  if (isLoading) return <Loading className="min-h-screen" />;
  if (error || !data) return <ErrorState error={error ?? new Error("Receipt not found")} />;
  const s = data.snapshot;
  return (
    <div className="min-h-screen bg-muted/40 px-3 py-6">
      <div className="mx-auto max-w-md space-y-4">
        <div className="rounded-2xl bg-white p-3 shadow-sm">
          <Receipt snapshot={s} zoom={1.7} />
        </div>
        <div className="grid grid-cols-2 gap-2">
          <Button variant="outline" onClick={async () => download(await receiptPdf(s), fileName(s, "pdf"))}><Download /> {t("Download PDF")}</Button>
          <Button variant="outline" onClick={() => printReceipt(s)}><Printer /> {t("Print")}</Button>
        </div>
      </div>
    </div>
  );
}
