/** The 50 mm digital receipt (roadmap 65): drawn from the stored snapshot, row for row the same as the PDF. On screen it
 * is enlarged for readability (`zoom`) without changing its proportions; it is never wider than 50 mm when printed. */
import { forwardRef } from "react";
import mark from "@/assets/sshop-mark.png";
import { cn } from "@/lib/utils";
import { logoUrl, receiptRows, when, type ReceiptSnapshot } from "@/lib/receipt";

export const Receipt = forwardRef<HTMLDivElement, { snapshot: ReceiptSnapshot; zoom?: number; className?: string }>(function Receipt({ snapshot: s, zoom = 1.6, className }, ref) {
  const thermal = s.font === "thermal";
  const phone = s.branch.phone || s.business.phone;
  return (
    <div className={cn("flex justify-center", className)}>
      <div style={{ zoom }}>
        <div
          ref={ref}
          className={cn("receipt-50 bg-white text-[#111]", thermal ? "font-mono" : "font-sans")}
          style={{ width: "50mm", padding: "2.5mm", fontSize: "7pt", lineHeight: 1.3, fontVariantNumeric: "tabular-nums" }}
        >
          <div className="text-center">
            {s.business.logo && <img src={logoUrl(s.business.logo)!} alt="" crossOrigin="anonymous" className="mx-auto mb-[1.5mm] max-h-[14mm] max-w-[14mm] object-contain" />}
            <div style={{ fontSize: "8.5pt" }} className="font-bold leading-tight">{s.business.name}</div>
            {s.branch.name && <div>{s.branch.name}</div>}
            {phone && <div>{phone}</div>}
          </div>
          <Rule />
          <div className="text-center font-bold" style={{ fontSize: "7.5pt", letterSpacing: "0.04em" }}>{s.title}</div>
          <KV k={s.kind === "adjustment" ? "Ref:" : "Receipt:"} v={s.number} />
          <KV k="Date:" v={when(s.at, s.timezone)} />
          {s.customer && <KV k="Customer:" v={s.customer} />}
          <Rule />
          {s.kind === "original" && (
            <div className="flex justify-between font-bold" style={{ fontSize: "6pt" }}><span>Item</span><span>Total</span></div>
          )}
          {receiptRows(s).map((r, i) => {
            if (r.t === "rule") return <Rule key={i} />;
            if (r.t === "heading") return <div key={i} className="mt-[1mm] font-bold uppercase" style={{ fontSize: "6.2pt" }}>{r.text}</div>;
            if (r.t === "note") return <div key={i} className="mt-[0.5mm] text-[#555]" style={{ fontSize: "5.8pt" }}>{r.text}</div>;
            if (r.t === "item") {
              return (
                <div key={i} className={cn("py-[0.4mm]", r.muted && "text-[#666]")}>
                  <div className="break-words font-semibold">{r.name}</div>
                  <div className="flex justify-between ps-[1mm]" style={{ fontSize: "6.2pt" }}><span>{r.detail}</span><span>{r.total}</span></div>
                </div>
              );
            }
            return <KV key={i} k={r.k} v={r.v} strong={r.strong} big={r.big} />;
          })}
          {s.served_by && (
            <>
              <Rule />
              <KV k="Served by:" v={s.served_by} />
            </>
          )}
          <Rule />
          {s.footer && <div className="text-center font-bold">{s.footer}</div>}
          <div className="mt-[0.8mm] text-center text-[#666]" style={{ fontSize: "5.6pt" }}>Digitally signed by {s.signed_by}</div>
          <img src={mark} alt="S'" className="mx-auto mt-[1.2mm] h-[4mm] w-[4mm] object-contain opacity-80" />
        </div>
      </div>
    </div>
  );
});

function Rule() {
  return <div className="my-[1.3mm] border-t border-dashed border-[#999]" />;
}

function KV({ k, v, strong, big }: { k: string; v: string; strong?: boolean; big?: boolean }) {
  return (
    <div className={cn("flex items-start justify-between gap-[2mm]", strong && "font-bold", big && "my-[0.6mm]")} style={big ? { fontSize: "8pt" } : undefined}>
      <span className="min-w-0">{k}</span>
      <span className="shrink-0 text-end">{v}</span>
    </div>
  );
}
