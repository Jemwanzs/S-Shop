import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { session } from "./api";

/** Which query caches each live topic invalidates. */
const TOPICS: Record<string, string[][]> = {
  notification: [["notifications"]],
  order: [["orders"], ["order"], ["dashboard"]],
  stock: [["stock"], ["pos-products"], ["products"], ["product"]],
  sale: [["sales"], ["dashboard"], ["customers"]],
  approval: [["approvals"], ["notifications"]],
  mpesa: [["mpesa"]],
};

/** Subscribe to the server's SSE stream while signed in; reconnects automatically. */
export function useLiveEvents(enabled: boolean) {
  const qc = useQueryClient();
  useEffect(() => {
    if (!enabled || !session.token || typeof EventSource === "undefined") return;
    const es = new EventSource(`/api/events?access_token=${encodeURIComponent(session.token)}`);
    for (const [topic, keys] of Object.entries(TOPICS)) {
      es.addEventListener(topic, (ev) => {
        keys.forEach((queryKey) => qc.invalidateQueries({ queryKey }));
        if (topic === "notification") {
          try {
            const n = JSON.parse((ev as MessageEvent).data);
            toast(n.title, { description: n.body });
          } catch {
            /* malformed payload — the list refetch still happens */
          }
        }
      });
    }
    return () => es.close();
  }, [enabled, qc]);
}
