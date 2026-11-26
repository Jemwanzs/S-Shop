/** Roadmap 186: Settings → Products → Manage branch prices (Tenant Administrators). One branch at a time: every product
 * sold there with its stock, default price, the branch's own price and the effective price; change one, the selected
 * ones, a category or the whole branch — reset to 0, back to the default, or new prices. A reset stays 0 (never the
 * default, never sold) until a price is set. Changes follow the Product edit approval when it is on. */
import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RotateCcw, Save, Search } from "lucide-react";
import { api, photoUrl } from "@/lib/api";
import { useSession } from "@/lib/session";
import { toast } from "@/lib/toast";
import { t } from "@/lib/i18n";
import { count, money, toNum } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Category, Outcome } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { ConfirmDialog, Select } from "@/components/Form";
import { Pill, type Tone } from "@/components/Badges";
import { Pager } from "@/components/DataList";
import { Card, SettingsPage } from "./shared";

type Status = "default" | "branch" | "reset" | "no_price";
interface Row {
  product_id: string; name: string; code: string; category_id: string | null; category_name: string | null; photo_id: string | null;
  available: number; default_price: string | null; own_price: string | null; price: string | null; status: Status; pending: boolean;
}
type Action = "reset" | "restore" | "set";
interface Pending { action: Action; rows: Row[]; title: string }

const STATUS: Record<Status, [string, Tone]> = {
  default: ["Default price", "neutral"],
  branch: ["Branch price", "primary"],
  reset: ["Reset to 0", "warning"],
  no_price: ["No price", "danger"],
};
const LIMIT = 50;
const num = (v: string | null) => (v == null ? null : toNum(v));

export function BranchPricesSettings() {
  const { profile, branch: current } = useSession();
  const qc = useQueryClient();
  const branches = profile?.branches ?? [];
  const [branchId, setBranchId] = useState(current?.id ?? branches[0]?.id ?? "");
  const [q, setQ] = useState("");
  const [category, setCategory] = useState("");
  const [stock, setStock] = useState("");
  const [status, setStatus] = useState("");
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [edits, setEdits] = useState<Record<string, string | null>>({});
  const [confirm, setConfirm] = useState<Pending | null>(null);
  const data = useQuery({
    queryKey: ["branch-prices", branchId],
    queryFn: () => api<{ branch_name: string; items: Row[] }>("/branch-prices", { query: { branch_id: branchId } }),
    enabled: !!branchId,
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const rows = useMemo(() => {
    const k = q.trim().toLowerCase();
    return (data.data?.items ?? []).filter((r) =>
      (!k || r.name.toLowerCase().includes(k) || r.code.toLowerCase().includes(k))
      && (!category || r.category_id === category)
      && (!stock || (stock === "in" ? r.available > 0 : r.available <= 0))
      && (!status || (status === "pending" ? r.pending : r.status === status)));
  }, [data.data, q, category, stock, status]);
  const page = rows.slice(offset, offset + LIMIT);
  const branchName = data.data?.branch_name ?? branches.find((b) => b.id === branchId)?.name ?? "";
  const reset = (fn: () => void) => { fn(); setOffset(0); };
  const changeBranch = (id: string) => { setBranchId(id); setSelected(new Set()); setEdits({}); setOffset(0); };
  const editsList = Object.entries(edits);
  const invalid = editsList.some(([, v]) => v != null && (v.trim() === "" || !(toNum(v) >= 0)));
  const byId = new Map((data.data?.items ?? []).map((r) => [r.product_id, r]));

  const run = async (action: Action, items: { product_id: string; price: number | null; expected: number | null }[]) => {
    const r = await api<Outcome<{ changed: number }>>("/branch-prices", { body: { branch_id: branchId, action, items } });
    if (r.pending_approval) toast.success("Sent for approval — the prices change once it is approved");
    else toast.success(`${count(r.result?.changed ?? 0)} ${t(r.result?.changed === 1 ? "price changed at" : "prices changed at")} ${branchName}`);
    setSelected(new Set());
    setEdits({});
    await qc.invalidateQueries({ queryKey: ["branch-prices"] });
    qc.invalidateQueries({ queryKey: ["pos-products"] });
    qc.invalidateQueries({ queryKey: ["products"] });
  };
  const ask = (action: Action, list: Row[], title: string) => list.length ? setConfirm({ action, rows: list, title }) : toast.error("No products to change");
  const saveEdits = () => run("set", editsList.map(([id, v]) => ({ product_id: id, price: v == null ? null : toNum(v), expected: num(byId.get(id)?.own_price ?? null) })));
  const chosen = (data.data?.items ?? []).filter((r) => selected.has(r.product_id));
  /** One product's pieces (the table on wider screens, a card on phones). */
  const view = (r: Row) => {
    const edited = r.product_id in edits;
    const own = edited ? edits[r.product_id] : r.own_price == null ? null : String(toNum(r.own_price));
    const on = own != null;
    const [label, tone] = STATUS[r.status];
    return {
      edited,
      effective: on ? toNum(own) : num(r.default_price),
      select: <Checkbox checked={selected.has(r.product_id)} aria-label={r.name} onCheckedChange={(c) => setSelected((s) => { const n = new Set(s); if (c) n.add(r.product_id); else n.delete(r.product_id); return n; })} />,
      photo: r.photo_id ? <img src={photoUrl(r.photo_id)} alt="" className="h-8 w-8 shrink-0 rounded object-cover" /> : <span className="h-8 w-8 shrink-0 rounded bg-muted" />,
      status: <div className="flex flex-wrap justify-end gap-1"><Pill tone={tone}>{t(label)}</Pill>{r.pending && <Pill tone="info">{t("Waiting for approval")}</Pill>}</div>,
      editor: (
        <div className="flex items-center gap-2">
          <Switch checked={on} aria-label={`${t("Own price at")} ${branchName}: ${r.name}`}
            onCheckedChange={(v) => setEdits((e) => {
              const n = { ...e };
              const wantedOwn = v ? String(toNum(r.own_price ?? r.default_price ?? 0)) : null;
              if ((r.own_price == null) === (wantedOwn == null) && (wantedOwn == null || toNum(wantedOwn) === toNum(r.own_price))) delete n[r.product_id];
              else n[r.product_id] = wantedOwn;
              return n;
            })} />
          <Input inputMode="decimal" className="num h-8 w-28" disabled={!on} value={own ?? ""} aria-label={`${t("Branch price")}: ${r.name}`}
            onChange={(e) => { const x = e.target.value.replace(/[^\d.]/g, ""); setEdits((m) => ({ ...m, [r.product_id]: x })); }} />
        </div>
      ),
    };
  };
  const allOnPage = page.length > 0 && page.every((r) => selected.has(r.product_id));

  return (
    <SettingsPage title="Manage branch prices" description="Each branch's own selling prices. A branch without its own price uses the product's default price; a price reset to 0 stays 0 — the product is not sold there until a new price is set.">
      <Card>
        <div className="grid gap-2 md:grid-cols-[minmax(0,14rem)_1fr]">
          <Select value={branchId} label={t("Branch")} onChange={changeBranch}>
            {branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
          </Select>
          <div className="relative">
            <Search className="absolute inset-s-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input value={q} onChange={(e) => reset(() => setQ(e.target.value))} placeholder={t("Product name or code")} className="ps-9" />
          </div>
        </div>
        <div className="mt-2 grid grid-cols-1 gap-2 sm:grid-cols-3">
          <Select value={category} label={t("Category")} onChange={(v) => reset(() => setCategory(v))}>
            <option value="">{t("All categories")}</option>
            {categories.data?.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
          <Select value={stock} label={t("Stock")} onChange={(v) => reset(() => setStock(v))}>
            <option value="">{t("Any stock")}</option>
            <option value="in">{t("In stock")}</option>
            <option value="out">{t("Out of stock")}</option>
          </Select>
          <Select value={status} label={t("Price status")} onChange={(v) => reset(() => setStatus(v))}>
            <option value="">{t("Any price status")}</option>
            {(Object.keys(STATUS) as Status[]).map((k) => <option key={k} value={k}>{t(STATUS[k][0])}</option>)}
            <option value="pending">{t("Waiting for approval")}</option>
          </Select>
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button size="sm" variant="outline" disabled={!chosen.length} onClick={() => ask("reset", chosen, "Reset selected prices to 0")}><RotateCcw /> {t("Reset selected to 0")}{chosen.length ? ` (${chosen.length})` : ""}</Button>
          <Button size="sm" variant="outline" disabled={!chosen.length} onClick={() => ask("restore", chosen, "Restore default pricing for the selected products")}>{t("Restore default (selected)")}</Button>
          <Button size="sm" variant="outline" disabled={!category} onClick={() => ask("reset", (data.data?.items ?? []).filter((r) => r.category_id === category), "Reset every price in this category to 0")}>{t("Reset category to 0")}</Button>
          <Button size="sm" variant="outline" className="text-destructive" onClick={() => ask("reset", data.data?.items ?? [], "Reset all branch prices to 0")}>{t("Reset all branch prices to 0")}</Button>
          <Button size="sm" variant="outline" onClick={() => ask("restore", (data.data?.items ?? []).filter((r) => r.own_price != null), "Restore default pricing for the whole branch")}>{t("Restore default (all)")}</Button>
        </div>
      </Card>

      <Card>
        {data.isLoading ? <p className="py-4 text-sm text-muted-foreground">{t("Loading…")}</p> : (
          <>
            <p className="mb-2 text-xs text-muted-foreground">{count(rows.length)} {t("products at")} {branchName}</p>
            {/* Phones: one card per product. */}
            <div className="divide-y md:hidden">
              {page.map((r) => {
                const v = view(r);
                return (
                  <div key={r.product_id} className={cn("space-y-2 py-3", v.edited && "bg-primary/5")} data-testid="branch-price-card">
                    <div className="flex items-start gap-2.5">
                      {v.select}
                      {v.photo}
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-medium">{r.name}</div>
                        <div className="truncate text-xs text-muted-foreground">{r.code} · {r.category_name ?? "—"}</div>
                      </div>
                      {v.status}
                    </div>
                    <div className="grid grid-cols-3 gap-2 text-xs">
                      <div><div className="text-muted-foreground">{t("Stock")}</div><div className={cn("num font-medium", r.available <= 0 && "text-destructive")}>{count(r.available)}</div></div>
                      <div><div className="text-muted-foreground">{t("Default")}</div><div className="num">{r.default_price == null ? "—" : money(r.default_price)}</div></div>
                      <div><div className="text-muted-foreground">{t("Effective")}</div><div className="num font-semibold">{v.effective == null ? "—" : money(v.effective)}</div></div>
                    </div>
                    <div className="flex items-center gap-2 text-sm"><span className="text-muted-foreground">{t("Branch price")}</span>{v.editor}</div>
                  </div>
                );
              })}
            </div>
            <div className="-mx-1 hidden overflow-x-auto md:block">
              <table className="w-full text-sm">
                <thead>
                  <tr className="text-start text-[11px] uppercase tracking-wide text-muted-foreground">
                    <th className="w-8 p-2"><Checkbox checked={allOnPage} aria-label={t("Select all on this page")} onCheckedChange={(c) => setSelected((s) => { const n = new Set(s); page.forEach((r) => (c ? n.add(r.product_id) : n.delete(r.product_id))); return n; })} /></th>
                    <th className="p-2 text-start">{t("Product")}</th>
                    <th className="p-2 text-start">{t("Category")}</th>
                    <th className="p-2 text-end">{t("Stock")}</th>
                    <th className="p-2 text-end">{t("Default")}</th>
                    <th className="p-2 text-start">{t("Branch price")}</th>
                    <th className="p-2 text-end">{t("Effective")}</th>
                    <th className="p-2 text-start">{t("Status")}</th>
                  </tr>
                </thead>
                <tbody className="divide-y">
                  {page.map((r) => {
                    const v = view(r);
                    return (
                      <tr key={r.product_id} className={cn(v.edited && "bg-primary/5")} data-testid="branch-price-row">
                        <td className="p-2">{v.select}</td>
                        <td className="p-2">
                          <div className="flex min-w-0 items-center gap-2">
                            {v.photo}
                            <div className="min-w-0"><div className="truncate font-medium">{r.name}</div><div className="text-xs text-muted-foreground">{r.code}</div></div>
                          </div>
                        </td>
                        <td className="p-2 text-muted-foreground">{r.category_name ?? "—"}</td>
                        <td className={cn("num p-2 text-end", r.available <= 0 && "text-destructive")}>{count(r.available)}</td>
                        <td className="num p-2 text-end text-muted-foreground">{r.default_price == null ? "—" : money(r.default_price)}</td>
                        <td className="p-2">{v.editor}</td>
                        <td className="num p-2 text-end font-semibold">{v.effective == null ? "—" : money(v.effective)}</td>
                        <td className="p-2">{v.status}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {rows.length === 0 && <p className="py-4 text-center text-sm text-muted-foreground">{t("No products match these filters.")}</p>}
            <div className="mt-2"><Pager total={rows.length} limit={LIMIT} offset={offset} onChange={setOffset} /></div>
          </>
        )}
      </Card>

      {editsList.length > 0 && (
        <div className="sticky bottom-20 z-20 flex items-center justify-between gap-3 rounded-xl border bg-card p-3 shadow-lg lg:bottom-4">
          <span className="text-sm">{count(editsList.length)} {t("price changes at")} {branchName}</span>
          <div className="flex gap-2">
            <Button variant="ghost" size="sm" onClick={() => setEdits({})}>{t("Discard")}</Button>
            <Button size="sm" disabled={invalid} onClick={() => setConfirm({ action: "set", rows: editsList.map(([id]) => byId.get(id)!).filter(Boolean), title: "Set branch prices" })}><Save /> {t("Save prices")}</Button>
          </div>
        </div>
      )}

      <ConfirmDialog open={!!confirm} onOpenChange={(o) => !o && setConfirm(null)} title={confirm ? t(confirm.title) : ""} destructive={confirm?.action === "reset"}
        confirmLabel={confirm?.action === "reset" ? "Reset to 0" : confirm?.action === "restore" ? "Restore default" : "Save prices"}
        description={confirm && <>{count(confirm.rows.length)} {t("products at")} <b>{branchName}</b>. {confirm.action === "reset"
          ? t("Their price here becomes 0 and stays 0 — they are not sold here until a new price is set. Products, stock and other branches are not changed.")
          : confirm.action === "restore" ? t("They use the product's default price here again.") : t("The new prices apply as soon as they are saved (or approved, when approval is required).")}</>}
        onConfirm={async () => {
          const c = confirm!;
          if (c.action === "set") await saveEdits();
          else await run(c.action, c.rows.map((r) => ({ product_id: r.product_id, price: c.action === "reset" ? 0 : null, expected: num(r.own_price) })));
          setConfirm(null);
        }} />
    </SettingsPage>
  );
}
