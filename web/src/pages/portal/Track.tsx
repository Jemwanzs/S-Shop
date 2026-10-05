import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { RefreshCw, XCircle } from "lucide-react";
import { api } from "@/lib/api";
import { amount, dateTime } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { ErrorState, Loading } from "@/components/Page";
import { PortalHeader, PoweredBy, Steps, type Step } from "./shared";

interface TrackData {
  business: { name: string; slug: string; logo_url: string | null };
  order: { order_no: string; status: string; status_label: string; total: string; created_at: string; delivery_location: string; terminal: boolean };
  steps: Step[];
  items: { name: string; quantity: number; unit_price: string; line_total: string }[];
  events: { status: string; label: string; notes: string; created_at: string }[];
}

/** Public order tracking — the link is the credential, no account needed. */
export default function Track() {
  const { token } = useParams();
  const { data, isLoading, error, refetch, isFetching } = useQuery({
    queryKey: ["track", token],
    queryFn: () => api<TrackData>(`/portal/track/${token}`, { token: null }),
    refetchInterval: 60_000,
  });
  if (isLoading) return <Loading className="min-h-screen" />;
  if (error || !data) return <ErrorState error={error ?? new Error("Order not found")} />;
  const o = data.order;
  return (
    <div className="min-h-screen bg-background">
      <div className="mx-auto max-w-lg">
        <PortalHeader name={data.business.name} logo={data.business.logo_url} />
        <main className="space-y-6 px-5 py-8">
          <div className="text-center">
            <p className="num text-muted-foreground">{o.order_no}</p>
            <h2 className="mt-1 text-2xl font-semibold">{o.status_label}</h2>
            <p className="mt-1 text-sm text-muted-foreground">Placed {dateTime(o.created_at)}</p>
          </div>
          {o.terminal ? (
            <div className="flex items-center gap-3 rounded-xl bg-destructive/10 p-4 text-destructive"><XCircle className="h-5 w-5" /> This order was {o.status}.</div>
          ) : (
            <div className="px-2"><Steps steps={data.steps} /></div>
          )}
          {o.delivery_location && (
            <div className="surface card-body">
              <p className="font-medium">Delivering to</p>
              <p className="mt-1 text-muted-foreground">{o.delivery_location}</p>
            </div>
          )}
          <div className="surface divide-y overflow-hidden">
            {data.items.map((i) => (
              <div key={i.name} className="flex justify-between gap-3 px-5 py-4">
                <span>{i.name}{i.quantity > 1 && <span className="text-muted-foreground"> × {i.quantity}</span>}</span>
                <span className="num">{amount(i.line_total, true)}</span>
              </div>
            ))}
            <div className="flex justify-between px-5 py-4 font-semibold"><span>Total</span><span className="num">{amount(o.total, true)}</span></div>
          </div>
          <div className="text-center">
            <Button variant="ghost" className="text-muted-foreground" onClick={() => refetch()} disabled={isFetching}>
              <RefreshCw className={isFetching ? "animate-spin" : ""} /> Reload this page any time for the latest status.
            </Button>
          </div>
        </main>
        <PoweredBy />
      </div>
    </div>
  );
}
