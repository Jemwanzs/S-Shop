import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "@/lib/toast";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { titleCase, toNum } from "@/lib/format";
import type { CustomField, MedalTargets, Settings } from "@/lib/types";
import { useCustomFields, type FieldKind } from "@/components/CustomFields";

const OPTIONAL_STATUSES = ["preparing", "dispatched", "on_delivery", "completed"];
import { Button } from "@/components/ui/button";
import { ActionButton } from "@/components/ActionButton";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, Select, ToggleRow } from "@/components/Form";
import { Pill } from "@/components/Badges";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Card, SettingsPage, useSettingsDraft } from "./shared";
import { HoursEditor, hoursLabel } from "@/components/Hours";
import { t } from "@/lib/i18n";
import { t as tr } from "@/lib/i18n";

type Draft = ReturnType<typeof useSettingsDraft>;

function Page({ d, title, description, children }: { d: Draft; title: string; description?: string; children: (s: Settings) => React.ReactNode }) {
  return (
    <SettingsPage title={title} description={description} loading={!d.draft} dirty={d.dirty} saving={d.saving} onSave={d.save} onReset={d.reset}>
      {d.draft && children(d.draft)}
    </SettingsPage>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2 py-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="sm:w-56">{children}</div>
    </div>
  );
}

/** Keeps the typed text (so "0." can become "0.5") while reporting the number. */
function NumInput({ value, onChange, decimal }: { value: number | string; onChange: (n: number) => void; decimal?: boolean }) {
  const [text, setText] = useState(String(value));
  useEffect(() => {
    setText((t) => (toNum(t) === toNum(value) ? t : String(value)));
  }, [value]);
  return (
    <Input
      inputMode={decimal ? "decimal" : "numeric"}
      className="num"
      value={text}
      onChange={(e) => {
        const t = e.target.value.replace(decimal ? /[^\d.]/g : /\D/g, "");
        setText(t);
        onChange(toNum(t));
      }}
    />
  );
}

const numInput = (value: number | string, onChange: (n: number) => void, decimal = false) => <NumInput value={value} onChange={onChange} decimal={decimal} />;

/** Simple named lists (categories, suppliers, expense categories). */
function NamedList({ endpoint, queryKey, title, extra }: { endpoint: string; queryKey: string; title: string; extra?: boolean }) {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: [queryKey], queryFn: () => api<{ id: string; name: string; is_active: boolean; phone?: string }[]>(endpoint) });
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);
  const done = () => qc.invalidateQueries({ queryKey: [queryKey] });
  const add = useMutation({ mutationFn: () => api(endpoint, { body: { name } }), onSuccess: () => { setName(""); done(); }, onError: (e) => toast.error(e) });
  const save = useMutation({
    mutationFn: (b: { id: string; name: string; is_active?: boolean }) => api(`${endpoint}/${b.id}`, { method: "PUT", body: b }),
    onSuccess: () => { setEditing(null); done(); },
    onError: (e) => toast.error(e),
  });
  return (
    <Card title={title}>
      <form className="flex gap-2 py-3" onSubmit={(e) => { e.preventDefault(); if (name.trim()) add.mutate(); }}>
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={`Add ${title.toLowerCase().replace(/s$/, "")}`} />
        <Button type="submit" variant="outline" disabled={!name.trim()}><Plus /> {tr("Add")}</Button>
      </form>
      {data?.map((c) => (
        <div key={c.id} className="flex items-center gap-3 py-2.5 text-sm">
          {editing?.id === c.id ? (
            <form className="flex flex-1 gap-2" onSubmit={(e) => { e.preventDefault(); save.mutate({ id: c.id, name: editing.name }); }}>
              <Input value={editing.name} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
              <Button type="submit" size="sm">{tr("Save")}</Button>
            </form>
          ) : (
            <>
              <span className={c.is_active ? "flex-1" : "flex-1 text-muted-foreground line-through"}>{c.name}</span>
              {extra && c.phone && <span className="num text-xs text-muted-foreground">{c.phone}</span>}
              <Button variant="ghost" size="icon-sm" onClick={() => setEditing({ id: c.id, name: c.name })} aria-label="Rename"><Pencil /></Button>
              <Switch checked={c.is_active} onCheckedChange={(v) => save.mutate({ id: c.id, name: c.name, is_active: v })} aria-label="Active" />
            </>
          )}
        </div>
      ))}
    </Card>
  );
}

export function ProductSettings() {
  const d = useSettingsDraft();
  return (
    <Page d={d} title="Products" description="Catalogue options, product fields, categories and suppliers.">
      {(s) => (
        <>
          <Card>
            <Row label="Photos per product" hint="Settings → Product Configuration → Product Photos">{numInput(s.product.max_photos, (n) => d.update((x) => { x.product.max_photos = n; }))}</Row>
            <Row label="Auto product code prefix" hint="e.g. PRD → PRD0001"><Input value={s.product.auto_code_prefix} onChange={(e) => d.update((x) => { x.product.auto_code_prefix = e.target.value.toUpperCase(); })} /></Row>
          </Card>
          <FieldsCard kind="product" empty="No product fields yet — add things like Size, Colour, Brand or Expiry date." />
          <div className="grid gap-5 xl:grid-cols-2">
            <NamedList endpoint="/categories" queryKey="categories" title="Categories" />
            <NamedList endpoint="/suppliers" queryKey="suppliers" title="Suppliers" extra />
          </div>
        </>
      )}
    </Page>
  );
}

export function SalesSettings() {
  const d = useSettingsDraft();
  const [newMethod, setNewMethod] = useState("");
  return (
    <Page d={d} title="Sales & payments" description="How the counter works.">
      {(s) => (
        <>
          <Card title="Sale entry">
            <Row label="Quantity entry" hint="Locked = every item captured individually">
              <Select value={s.sales.quantity_entry} onChange={(v) => d.update((x) => { x.sales.quantity_entry = v as "editable"; })}>
                <option value="editable">{tr("Editable")}</option><option value="locked">Locked to 1</option>
              </Select>
            </Row>
            <ToggleRow label="Require barcode clearance" hint="Products with a barcode must be scanned before they can be sold" checked={s.sales.require_barcode_clearance} onChange={(v) => d.update((x) => { x.sales.require_barcode_clearance = v; })} />
            <Row label="Receipt footer"><Input value={s.sales.receipt_footer} onChange={(e) => d.update((x) => { x.sales.receipt_footer = e.target.value; })} /></Row>
          </Card>
          <Card title="Payment methods">
            {s.sales.payment_methods.map((m, i) => (
              <div key={m.key} className="flex items-center gap-3 py-2.5">
                <Input className="flex-1" value={m.label} onChange={(e) => d.update((x) => { x.sales.payment_methods[i].label = e.target.value; })} />
                <Pill>{m.key}</Pill>
                <Switch checked={m.enabled} onCheckedChange={(v) => d.update((x) => { x.sales.payment_methods[i].enabled = v; })} />
                {!["mpesa", "cash", "credit"].includes(m.key) && (
                  <Button variant="ghost" size="icon-sm" onClick={() => d.update((x) => { x.sales.payment_methods.splice(i, 1); })} aria-label="Remove"><Trash2 /></Button>
                )}
              </div>
            ))}
            <form className="flex gap-2 py-3" onSubmit={(e) => {
              e.preventDefault();
              const key = newMethod.trim().toLowerCase().replace(/[^a-z0-9]+/g, "_");
              if (!key || s.sales.payment_methods.some((m) => m.key === key)) return;
              d.update((x) => { x.sales.payment_methods.push({ key, label: newMethod.trim(), enabled: true }); });
              setNewMethod("");
            }}>
              <Input value={newMethod} onChange={(e) => setNewMethod(e.target.value)} placeholder="Add a method, e.g. Bank transfer, Card" />
              <Button type="submit" variant="outline"><Plus /> {tr("Add")}</Button>
            </form>
            <ToggleRow label="Allow manual M-Pesa confirmation" hint="Cashier types the M-Pesa code when STK Push is unavailable" checked={s.sales.mpesa_manual_confirmation} onChange={(v) => d.update((x) => { x.sales.mpesa_manual_confirmation = v; })} />
          </Card>
          <Card title="Credit sales">
            <ToggleRow label="Allow credit sales" checked={s.sales.credit_enabled} onChange={(v) => d.update((x) => { x.sales.credit_enabled = v; })} />
            <Row label="Default days to pay">{numInput(s.sales.credit_default_days, (n) => d.update((x) => { x.sales.credit_default_days = n; }))}</Row>
          </Card>
        </>
      )}
    </Page>
  );
}

export function StockSettings() {
  const d = useSettingsDraft();
  return (
    <Page d={d} title="Stock" description="Barcodes, quantities, valuation and transfer control.">
      {(s) => (
        <Card>
          <Row label="Barcode requirement">
            <Select value={s.stock.barcode_requirement} onChange={(v) => d.update((x) => { x.stock.barcode_requirement = v as "optional"; })}>
              <option value="required">{tr("Required")}</option><option value="optional">{tr("Optional")}</option><option value="disabled">{tr("Disabled")}</option>
            </Select>
          </Row>
          <Row label="Quantity entry when receiving" hint="Locked = capture each physical item individually">
            <Select value={s.stock.quantity_entry} onChange={(v) => d.update((x) => { x.stock.quantity_entry = v as "editable"; })}>
              <option value="editable">{tr("Editable")}</option><option value="locked">Locked to 1</option>
            </Select>
          </Row>
          <ToggleRow label="Capture cost price" hint="Enables profit and cost valuation" checked={s.stock.capture_cost} onChange={(v) => d.update((x) => { x.stock.capture_cost = v; })} />
          <Row label="Stock valuation">
            <Select value={s.stock.valuation} onChange={(v) => d.update((x) => { x.stock.valuation = v as "cost"; })}>
              <option value="cost">{tr("At cost")}</option><option value="selling">{tr("At selling price")}</option>
            </Select>
          </Row>
          <Row label="Default low-stock alert">{numInput(s.stock.low_stock_threshold, (n) => d.update((x) => { x.stock.low_stock_threshold = n; }))}</Row>
          <ToggleRow label="Transfer receipt control" hint="Stock stays “in transit” until the destination confirms receipt" checked={s.stock.transfer_receipt_control} onChange={(v) => d.update((x) => { x.stock.transfer_receipt_control = v; })} />
          <ToggleRow label="Allow negative stock" hint="Not recommended — sales could exceed physical stock" checked={s.stock.allow_negative} onChange={(v) => d.update((x) => { x.stock.allow_negative = v; })} />
        </Card>
      )}
    </Page>
  );
}

export function OrderSettings() {
  const d = useSettingsDraft();
  const { profile } = useSession();
  return (
    <Page d={d} title="Orders & ordering link" description="How customer orders flow and when they become sales.">
      {(s) => (
        <>
        <Card>
          <ToggleRow label="Ordering link open" hint="Customers can place orders online" checked={s.orders.portal_enabled} onChange={(v) => d.update((x) => { x.orders.portal_enabled = v; })} />
          <Row label="Fulfilling branch">
            <Select value={s.orders.default_branch_id ?? ""} onChange={(v) => d.update((x) => { x.orders.default_branch_id = v || null; })}>
              <option value="">{tr("First active branch")}</option>
              {profile?.branches.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
            </Select>
          </Row>
          <ToggleRow label="Reserve stock for confirmed orders" hint="Available to sell = physical − reserved" checked={s.orders.reserve_stock} onChange={(v) => d.update((x) => { x.orders.reserve_stock = v; })} />
          <Row label="Order becomes a sale at">
            <Select value={s.orders.sale_on_status} onChange={(v) => d.update((x) => { x.orders.sale_on_status = v as "delivered"; })}>
              <option value="delivered">{tr("Delivered")}</option><option value="completed">{tr("Completed")}</option>
            </Select>
          </Row>
          <ToggleRow label="Verify customers with a WhatsApp code" hint="Requires the WhatsApp integration" checked={s.orders.verify_with_otp} onChange={(v) => d.update((x) => { x.orders.verify_with_otp = v; })} />
          <ToggleRow label="Show out-of-stock products" checked={s.orders.show_out_of_stock} onChange={(v) => d.update((x) => { x.orders.show_out_of_stock = v; })} />
          <ToggleRow label="Show product prices" hint="When off, customers see no prices anywhere on the ordering link; staff screens are not affected" checked={s.orders.show_prices} onChange={(v) => d.update((x) => { x.orders.show_prices = v; })} />
        </Card>
        <Card title="Order statuses">
          <p className="py-2 text-xs text-muted-foreground">Rename any status (customers see these names). Optional steps can be switched off; core steps and the stage at which an order becomes a sale are always on.</p>
          {s.orders.statuses.map((st, i) => {
            const optional = OPTIONAL_STATUSES.includes(st.key) && st.key !== s.orders.sale_on_status;
            return (
              <div key={st.key} className="flex items-center gap-3 py-2.5">
                <Input className="flex-1" value={st.label} onChange={(e) => d.update((x) => { x.orders.statuses[i].label = e.target.value; })} />
                <Pill>{st.key}</Pill>
                {optional ? (
                  <Switch checked={st.enabled} onCheckedChange={(v) => d.update((x) => { x.orders.statuses[i].enabled = v; })} aria-label={`Use ${st.label}`} />
                ) : (
                  <span className="w-11 text-center text-xs text-muted-foreground">{tr("Core")}</span>
                )}
              </div>
            );
          })}
        </Card>
        </>
      )}
    </Page>
  );
}

/** Custom field editor shared by customers and products (Settings → … → Fields). */
function FieldsCard({ kind, empty }: { kind: FieldKind; empty: string }) {
  const qc = useQueryClient();
  const fields = useCustomFields(kind);
  const [editing, setEditing] = useState<Partial<CustomField> | null>(null);
  const save = useMutation({
    mutationFn: (f: Partial<CustomField>) =>
      api(f.id ? `/${kind}-fields/${f.id}` : `/${kind}-fields`, {
        method: f.id ? "PUT" : "POST",
        body: { label: f.label, field_type: f.field_type, options: f.options ?? [], required: !!f.required, is_active: f.is_active ?? true, display_order: f.display_order ?? 0 },
      }),
    onSuccess: () => { setEditing(null); qc.invalidateQueries({ queryKey: [`${kind}-fields`] }); toast.success("Field saved"); },
    onError: (e) => toast.error(e),
  });
  return (
    <>
      <Card title={kind === "product" ? "Product fields" : "Custom fields"} action={<Button size="sm" variant="outline" onClick={() => setEditing({ field_type: "text", is_active: true, options: [] })}><Plus /> {tr("Add field")}</Button>}>
        {fields.data?.map((f) => (
          <div key={f.id} className="flex items-center gap-3 py-2.5 text-sm">
            <span className={f.is_active ? "flex-1 font-medium" : "flex-1 text-muted-foreground line-through"}>{f.label}</span>
            <Pill>{titleCase(f.field_type)}</Pill>
            {f.required && <Pill tone="warning">{tr("Required")}</Pill>}
            <Button variant="ghost" size="icon-sm" onClick={() => setEditing(f)} aria-label="Edit"><Pencil /></Button>
          </div>
        ))}
        {!fields.data?.length && <p className="py-4 text-sm text-muted-foreground">{empty}</p>}
      </Card>
      <ResponsiveDialog
        open={!!editing}
        onOpenChange={(o) => !o && setEditing(null)}
        title={editing?.id ? "Edit field" : "New field"}
        footer={<ActionButton className="w-full md:w-auto" online busy={save.isPending} busyLabel="Saving…" blockedBy={[!editing?.label?.trim() && "Enter a label"]}
          onAction={() => editing && save.mutateAsync(editing)}>{tr("Save")}</ActionButton>}
      >
        {editing && (
          <div className="space-y-4">
            <Field label="Field name"><Input value={editing.label ?? ""} onChange={(e) => setEditing({ ...editing, label: e.target.value })} /></Field>
            <Field label="Type">
              <Select value={editing.field_type ?? "text"} onChange={(v) => setEditing({ ...editing, field_type: v as CustomField["field_type"] })}>
                {["text", "number", "date", "dropdown", "boolean", "email"].map((t) => <option key={t} value={t}>{t === "boolean" ? "Yes / No" : titleCase(t)}</option>)}
              </Select>
            </Field>
            {editing.field_type === "dropdown" && (
              <Field label="Options" hint="Comma separated"><Input value={(editing.options ?? []).join(", ")} onChange={(e) => setEditing({ ...editing, options: e.target.value.split(",").map((o) => o.trim()) })} /></Field>
            )}
            <Field label="Display order">{numInput(editing.display_order ?? 0, (n) => setEditing({ ...editing, display_order: n }))}</Field>
            <ToggleRow label="Required" checked={!!editing.required} onChange={(v) => setEditing({ ...editing, required: v })} />
            <ToggleRow label="Active" checked={editing.is_active ?? true} onChange={(v) => setEditing({ ...editing, is_active: v })} />
          </div>
        )}
      </ResponsiveDialog>
    </>
  );
}

export function CustomerSettings() {
  const d = useSettingsDraft();
  return (
    <Page d={d} title="Customers" description="Mobile number, first name and nickname are always captured. Add your own fields below.">
      {(s) => (
        <>
          <Card>
            <ToggleRow label="Require email" checked={s.customers.require_email} onChange={(v) => d.update((x) => { x.customers.require_email = v; })} />
          </Card>
          <FieldsCard kind="customer" empty="No custom fields yet. Imported customers keep location, city and country." />
        </>
      )}
    </Page>
  );
}

export function LoyaltySettings() {
  const d = useSettingsDraft();
  return (
    <Page d={d} title="Loyalty & rewards" description="Rules apply to new activity; history is never recalculated.">
      {(s) => (
        <>
          <Card title="Earning">
            <ToggleRow label="Loyalty enabled" checked={s.loyalty.enabled} onChange={(v) => d.update((x) => { x.loyalty.enabled = v; })} />
            <Row label="Spend per block" hint="Every X spent earns the points below">{numInput(s.loyalty.threshold, (n) => d.update((x) => { x.loyalty.threshold = n; }), true)}</Row>
            <Row label="Points per block">{numInput(s.loyalty.points_per, (n) => d.update((x) => { x.loyalty.points_per = n; }))}</Row>
            <Row label="Minimum sale to earn">{numInput(s.loyalty.min_spend, (n) => d.update((x) => { x.loyalty.min_spend = n; }), true)}</Row>
            <Row label="Referral bonus (%)" hint="Share of a referred customer's points given to the referrer">{numInput(s.loyalty.referral_bonus_percent, (n) => d.update((x) => { x.loyalty.referral_bonus_percent = n; }))}</Row>
            <Row label="Points expire after (days)" hint="0 = never">{numInput(s.loyalty.expiry_days, (n) => d.update((x) => { x.loyalty.expiry_days = n; }))}</Row>
          </Card>
          <Card title="Redemption">
            <ToggleRow label="Allow redemption" checked={s.loyalty.redemption_enabled} onChange={(v) => d.update((x) => { x.loyalty.redemption_enabled = v; })} />
            <Row label="Value of 1 point">{numInput(s.loyalty.point_value, (n) => d.update((x) => { x.loyalty.point_value = n; }), true)}</Row>
            <Row label="Minimum balance to redeem">{numInput(s.loyalty.min_redemption_points, (n) => d.update((x) => { x.loyalty.min_redemption_points = n; }))}</Row>
          </Card>
          <Card title="Tiers" action={<Button size="sm" variant="outline" onClick={() => d.update((x) => { x.loyalty.tiers.push({ name: "", min_spend: 0 }); })}><Plus /> {tr("Add tier")}</Button>}>
            {s.loyalty.tiers.map((t, i) => (
              <div key={i} className="flex items-center gap-2 py-2.5">
                <Input value={t.name} onChange={(e) => d.update((x) => { x.loyalty.tiers[i].name = e.target.value; })} placeholder="Tier name, e.g. Gold" />
                <span className="shrink-0 text-xs text-muted-foreground">from</span>
                <div className="w-36 shrink-0">{numInput(t.min_spend, (n) => d.update((x) => { x.loyalty.tiers[i].min_spend = n; }), true)}</div>
                <Button variant="ghost" size="icon-sm" onClick={() => d.update((x) => { x.loyalty.tiers.splice(i, 1); })} aria-label="Remove"><Trash2 /></Button>
              </div>
            ))}
          </Card>
          <Card title="Awards & ordering link">
            <Row label="Winners per award period" hint="Gold, Silver, Bronze, then Bronze">{numInput(s.loyalty.award_winners, (n) => d.update((x) => { x.loyalty.award_winners = n; }))}</Row>
            <ToggleRow label="Show points on the ordering link" checked={s.loyalty.show_on_portal} onChange={(v) => d.update((x) => { x.loyalty.show_on_portal = v; })} />
            <ToggleRow label="Show the money value of points" checked={s.loyalty.show_value_on_portal} onChange={(v) => d.update((x) => { x.loyalty.show_value_on_portal = v; })} />
          </Card>
        </>
      )}
    </Page>
  );
}

const LOCATION_AREAS: [string, string][] = [
  ["sales", "Record sales"], ["returns", "Returns & cancellations"], ["stock", "Receive & adjust stock"], ["transfers", "Transfers"],
  ["expenses", "Expenses"], ["orders", "Process orders"], ["credit", "Collect credit payments"],
];

export function WorkspaceSettings() {
  const d = useSettingsDraft();
  const { profile } = useSession();
  const own = profile?.branches.filter((b) => b.own_hours) ?? [];
  return (
    <Page
      d={d}
      title="Workspace"
      description="Working days and trading hours. Each sale, order, payment and stock movement also records the business day it belongs to, so late trading after midnight counts for the day that opened."
    >
      {(s) => (
        <>
          <Card title="Business hours">
            <div className="py-3">
              <HoursEditor value={s.workspace.hours} onChange={(h) => d.update((x) => { x.workspace.hours = h; })} />
            </div>
          </Card>
          <Card title="Outside trading hours">
            <Row label="Sales outside trading hours" hint="Blocking applies to counter sales; people with “Sell outside trading hours” can still sell.">
              <Select value={s.workspace.outside_hours} onChange={(v) => d.update((x) => { x.workspace.outside_hours = v as "allow" | "block"; })}>
                <option value="allow">{t("Allow")}</option>
                <option value="block">{t("Block")}</option>
              </Select>
            </Row>
          </Card>
          <Card title="Where staff can work">
            <Row label="Location rule" hint="“At the branch” accepts the chosen actions only from devices within the branch radius. Set each branch's location under Branches.">
              <Select value={s.workspace.location.mode} onChange={(v) => d.update((x) => { x.workspace.location.mode = v as "anywhere" | "branch"; })}>
                <option value="anywhere">{t("Anywhere")}</option>
                <option value="branch">{t("At the branch")}</option>
              </Select>
            </Row>
            {s.workspace.location.mode === "branch" && (
              <div className="py-3">
                <p className="mb-2 text-sm font-medium">{t("Actions that need to be at the branch")}</p>
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                  {LOCATION_AREAS.map(([key, label]) => (
                    <label key={key} className="flex items-center gap-2 text-sm">
                      <Checkbox
                        checked={s.workspace.location.areas.includes(key)}
                        onCheckedChange={(v) => d.update((x) => {
                          const set = new Set(x.workspace.location.areas);
                          if (v) set.add(key); else set.delete(key);
                          x.workspace.location.areas = LOCATION_AREAS.map(([k]) => k).filter((k) => set.has(k));
                        })}
                      />
                      {t(label)}
                    </label>
                  ))}
                </div>
                <p className="mt-3 text-xs text-muted-foreground">
                  {t("People with “Work away from the branch” are not restricted. The device location is saved in the audit trail with each action.")}
                </p>
              </div>
            )}
          </Card>
          <Card title="Branches with their own hours">
            {own.length ? (
              own.map((b) => (
                <div key={b.id} className="flex items-center justify-between gap-3 py-2.5 text-sm">
                  <span className="font-medium">{b.name}</span>
                  <span className="text-xs text-muted-foreground">{b.hours && hoursLabel(b.hours)}</span>
                </div>
              ))
            ) : (
              <p className="py-3 text-sm text-muted-foreground">{t("All branches follow the business hours. Set a branch's own hours under Branches.")}</p>
            )}
          </Card>
          <p className="text-xs text-muted-foreground">
            {t("Changing hours applies to new transactions; business days already recorded never move.")}
          </p>
        </>
      )}
    </Page>
  );
}

export function ExpenseSettings() {
  const d = useSettingsDraft();
  return (
    <Page d={d} title="Expenses" description="Categories and required fields. Approval rules live in the Workflow engine.">
      {(s) => (
        <>
          <Card>
            <ToggleRow label="Require a description" checked={s.expenses.require_description} onChange={(v) => d.update((x) => { x.expenses.require_description = v; })} />
            <ToggleRow label="Require a receipt attachment" checked={s.expenses.require_attachment} onChange={(v) => d.update((x) => { x.expenses.require_attachment = v; })} />
          </Card>
          <NamedList endpoint="/expense-categories" queryKey="expense-categories" title="Expense categories" />
        </>
      )}
    </Page>
  );
}

export function ReportSettings() {
  const d = useSettingsDraft();
  const { currency } = useSession();
  return (
    <Page d={d} title="Reports" description="Who sees which figures. Report access and export follow role permissions (reports.view, reports.export).">
      {(s) => {
        const targets = s.reports.medals.mode === "targets";
        const targetCard = (title: string, key: "products" | "staff", t: MedalTargets) => (
          <Card title={title}>
            <Row label="Measured by">
              <Select value={t.basis} onChange={(v) => d.update((x) => { x.reports.medals[key].basis = v as MedalTargets["basis"]; })}>
                <option value="revenue">{tr("Sales value (")}{currency})</option>
                <option value="units">{tr("Units sold")}</option>
              </Select>
            </Row>
            {(["gold", "silver", "bronze"] as const).map((level) => (
              <Row key={level} label={`${titleCase(level)} — per day`} hint={level === "bronze" ? "0 switches a medal off" : undefined}>
                {numInput(t[level], (n) => d.update((x) => { x.reports.medals[key][level] = n; }), t.basis === "revenue")}
              </Row>
            ))}
          </Card>
        );
        return (
          <>
            <Card>
              <ToggleRow
                label="Hide cost & profit from users without financial access"
                hint="Users need “View cost & profit figures” to see them"
                checked={s.reports.hide_financials_without_permission}
                onChange={(v) => d.update((x) => { x.reports.hide_financials_without_permission = v; })}
              />
            </Card>
            <Card title="Medals on dashboard leaderboards">
              <Row label="Best sellers & staff earn medals by" hint={targets ? "Anyone reaching a target, scaled to the period viewed (a week needs 7× the daily target)" : "Position: 1st Gold, 2nd Silver, 3rd Bronze"}>
                <Select value={s.reports.medals.mode} onChange={(v) => d.update((x) => { x.reports.medals.mode = v as "rank" | "targets"; })}>
                  <option value="rank">{tr("Rank (top three)")}</option>
                  <option value="targets">{tr("Targets")}</option>
                </Select>
              </Row>
            </Card>
            {targets && targetCard("Product targets", "products", s.reports.medals.products)}
            {targets && targetCard("Staff targets", "staff", s.reports.medals.staff)}
          </>
        );
      }}
    </Page>
  );
}
