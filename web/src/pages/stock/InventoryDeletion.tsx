/** Roadmap 172: deleting inventory captured in error. The person sees what would change (product, branch, barcode,
 * quantity, stock before and after), gives a reason and submits; the request goes through the *Inventory deletion*
 * approval chain and nothing changes until the final approval. The original receipt stays in history, marked Reversed. */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye, History, MoreHorizontal, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { count, dateTime, signed, titleCase } from "@/lib/format";
import { t } from "@/lib/i18n";
import { toast } from "@/lib/toast";
import { useSession } from "@/lib/session";
import { useNavigate } from "@/lib/router";
import type { Outcome } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ActionButton } from "@/components/ActionButton";
import { Field } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Loading } from "@/components/Page";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

interface Check {
  movement_id: string; product: string; branch: string; barcode: string | null; tracked: boolean;
  max_quantity: number; on_hand: number; available: number; eligible: boolean; problem: string | null;
}

/** A receipt line in the movements list, as far as deletion is concerned. */
export interface VoidableRow {
  id: string; created_at: string; product_id: string; product_name: string; branch_name: string; kind: string; quantity: number;
  barcode: string | null; notes: string; user_name: string | null; void_status: string | null; reversed_qty: number;
}

const explained = (s: string) => s.replace(/\s/g, "").length >= 10;

/** The state of a line's deletion, shown beside it. */
export function voidState(r: VoidableRow): [Tone, string] | null {
  if (r.reversed_qty > 0) return ["neutral", r.reversed_qty >= r.quantity ? "Reversed" : `${r.reversed_qty} reversed`];
  if (r.void_status === "pending") return ["warning", "Deletion pending approval"];
  if (r.void_status === "rejected") return ["danger", "Deletion rejected"];
  return null;
}

export function MovementActions({ row }: { row: VoidableRow }) {
  const { can } = useSession();
  const navigate = useNavigate();
  const [details, setDetails] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const receipt = ["received", "opening"].includes(row.kind) && row.quantity > 0;
  return (
    <div onClick={(e) => e.stopPropagation()}>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={t("Actions")}><MoreHorizontal /></Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={() => setDetails(true)}><Eye /> {t("View details")}</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => navigate(`/stock?tab=movements&product=${row.product_id}`)}><History /> {t("View history")}</DropdownMenuItem>
          {receipt && can("stock.adjust") && row.void_status !== "pending" && row.reversed_qty < row.quantity && (
            <DropdownMenuItem className="text-destructive" onSelect={() => setDeleting(true)}><Trash2 /> {t("Request inventory deletion")}</DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {details && (
        <ResponsiveDialog open onOpenChange={(o) => !o && setDetails(false)} title="Stock movement" confirmDiscard={false}
          footer={<Button onClick={() => setDetails(false)}>{t("Done")}</Button>}>
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
            <dt className="text-muted-foreground">{t("When")}</dt><dd>{dateTime(row.created_at)}</dd>
            <dt className="text-muted-foreground">{t("Product")}</dt><dd>{row.product_name}</dd>
            {row.barcode && <><dt className="text-muted-foreground">{t("Barcode")}</dt><dd className="num">{row.barcode}</dd></>}
            <dt className="text-muted-foreground">{t("Movement")}</dt><dd>{titleCase(row.kind)}</dd>
            <dt className="text-muted-foreground">{t("Qty")}</dt><dd className="num">{signed(row.quantity)}</dd>
            <dt className="text-muted-foreground">{t("Branch")}</dt><dd>{row.branch_name}</dd>
            <dt className="text-muted-foreground">{t("By")}</dt><dd>{row.user_name ?? "—"}</dd>
            {row.notes && <><dt className="text-muted-foreground">{t("Reference")}</dt><dd>{row.notes}</dd></>}
            {voidState(row) && <><dt className="text-muted-foreground">{t("Deletion")}</dt><dd><Pill tone={voidState(row)![0]}>{t(voidState(row)![1])}</Pill></dd></>}
          </dl>
        </ResponsiveDialog>
      )}
      {deleting && <DeletionDialog path={`/stock/movements/${row.id}/void`} onClose={() => setDeleting(false)} />}
    </div>
  );
}

/** Request a deletion — for a receipt line (`/stock/movements/{id}/void`) or the unit it brought in (`/stock/items/{id}/void`). */
export function DeletionDialog({ path, onClose, onDone }: { path: string; onClose: () => void; onDone?: () => void }) {
  const qc = useQueryClient();
  const [reason, setReason] = useState("");
  const [qty, setQty] = useState("");
  const check = useQuery({ queryKey: ["void-check", path], queryFn: () => api<Check>(path) });
  const c = check.data;
  const n = c?.tracked ? 1 : parseInt(qty || String(c?.max_quantity ?? 0), 10);
  const okQty = !!c && n >= 1 && n <= c.max_quantity;
  return (
    <ResponsiveDialog open onOpenChange={(o) => !o && onClose()} title="Request inventory deletion" confirmDiscard={!!reason}
      description="For stock captured in error. Nothing changes until the deletion is approved; the original receipt stays in history."
      footer={
        <div className="flex w-full flex-wrap justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t("Cancel")}</Button>
          <ActionButton variant="destructive" online blockedBy={[!c?.eligible && "Cannot delete", !okQty && "Check the quantity", !explained(reason) && "Give the reason"]}
            onAction={async () => {
              try {
                const r = await api<Outcome<unknown>>(path, { body: { reason, quantity: c?.tracked ? undefined : n } });
                toast.success(r.pending_approval ? "Deletion sent for approval — stock unchanged until it is approved" : "Stock deleted and the barcode released where possible");
                qc.invalidateQueries({ queryKey: ["stock"] });
                onDone?.();
                onClose();
              } catch (e) {
                toast.error(e);
                throw e;
              }
            }}><Trash2 /> {t("Submit deletion request")}</ActionButton>
        </div>
      }>
      {check.isLoading || !c ? <Loading /> : (
        <div className="space-y-3 text-sm">
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 rounded-xl bg-muted/60 p-3">
            <dt className="text-muted-foreground">{t("Product")}</dt><dd className="font-medium">{c.product}</dd>
            <dt className="text-muted-foreground">{t("Branch")}</dt><dd>{c.branch}</dd>
            {c.barcode && <><dt className="text-muted-foreground">{t("Barcode")}</dt><dd className="num">{c.barcode}</dd></>}
            <dt className="text-muted-foreground">{t("Stock impact")}</dt>
            <dd className="num">{count(c.on_hand)} → {count(c.on_hand - (okQty ? n : 0))} {t("on hand")}</dd>
          </dl>
          {c.problem ? (
            <p role="alert" className="rounded-lg bg-destructive/10 p-2.5 text-xs text-destructive">{t(c.problem)}</p>
          ) : (
            <>
              {!c.tracked && (
                <Field label="Quantity to delete" hint={`${t("Up to")} ${count(c.max_quantity)}`}>
                  <Input inputMode="numeric" className="num" value={qty} placeholder={String(c.max_quantity)} onChange={(e) => setQty(e.target.value.replace(/\D/g, ""))} />
                </Field>
              )}
              {c.tracked && <p className="text-xs text-muted-foreground">{t("This item's barcode is released for reuse once the deletion is approved, unless something still needs it.")}</p>}
              <Field label="Reason for deletion" hint="e.g. received twice by mistake, wrong product captured">
                <Textarea value={reason} onChange={(e) => setReason(e.target.value)} />
              </Field>
            </>
          )}
        </div>
      )}
    </ResponsiveDialog>
  );
}
