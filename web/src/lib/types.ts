import type { Preferences } from "./prefs";
/** API response shapes (mirrors server/src/routes). Money arrives as strings/numbers. */

export type Money = string | number;
export type Id = string;

export interface Paged<T> {
  items: T[];
  total: number;
}

export interface Outcome<T> {
  pending_approval: boolean;
  approval_id: Id | null;
  result: T | null;
}

export interface Branch {
  id: Id;
  name: string;
  code: string;
  location: string;
  /** Effective trading hours (own override, else the business hours). */
  hours?: Hours;
  own_hours?: boolean;
  /** Present when geofencing is on for this branch. */
  geofence?: { latitude: number; longitude: number; radius_m: number } | null;
}

/** Trading days Monday..Sunday and one opening/closing time ("HH:MM"); a close at or before the open runs past midnight. */
export interface Hours {
  days: boolean[];
  open: string;
  close: string;
}

export interface PaymentMethod {
  key: string;
  label: string;
  enabled: boolean;
}

export interface Tier {
  name: string;
  min_spend: Money;
}

export interface Settings {
  product: { max_photos: number; auto_code_prefix: string; /** Roadmap 109 */ list_show_images?: boolean; list_show_prices?: boolean };
  stock: {
    barcode_requirement: "required" | "optional" | "disabled";
    /** Roadmap 98: typed / pasted barcodes allowed; camera scanning offered; stock takes count barcoded products by barcode. */
    manual_barcode_entry: boolean;
    camera_scanning: boolean;
    require_barcode_stocktake: boolean;
    quantity_entry: "editable" | "locked";
    capture_cost: boolean;
    valuation: "cost" | "selling";
    low_stock_threshold: number;
    allow_negative: boolean;
    transfer_receipt_control: boolean;
    /** Roadmap 133/135: S'Shop generates seven-digit item barcodes; new labels are scanned before a receipt completes. */
    generate_item_barcodes: boolean;
    require_label_verification: boolean;
    /** Roadmap 134/138: sticker size (mm) and label printer (system printing, ZPL or TSPL). */
    labels: { width_mm: number; height_mm: number; printer: "system" | "zpl" | "tspl" };
  };
  sales: {
    quantity_entry: "editable" | "locked";
    require_barcode_clearance: boolean;
    /** Roadmap 137: barcodes of barcode-controlled items may be typed at the till (default off: scan only). */
    manual_barcode_entry: boolean;
    payment_methods: PaymentMethod[];
    mpesa_manual_confirmation: boolean;
    credit_enabled: boolean;
    credit_default_days: number;
    receipt_footer: string;
    /** Receipt configuration (roadmap 66). */
    receipt: { show_logo: boolean; show_branch: boolean; show_contact: boolean; show_customer: boolean; show_salesperson: boolean; show_loyalty: boolean; show_payment_ref: boolean; font: "sans" | "thermal" };
    /** Roadmap 109: Record Sale product display. */
    pos: { style: "compact" | "image" | "list"; show_images: boolean; tablet_columns: number; desktop_columns: number; show_prices: boolean; show_stock?: boolean; images_first?: boolean };
    /** Roadmap 110: top-selling products first, with Gold / Silver / Bronze stars. */
    top_selling: { enabled: boolean; metric: "quantity" | "value" | "sales"; period: "today" | "week" | "month" };
  };
  orders: {
    portal_enabled: boolean;
    default_branch_id: Id | null;
    reserve_stock: boolean;
    sale_on_status: "delivered" | "completed";
    verify_with_otp: boolean;
    show_out_of_stock: boolean;
    show_prices: boolean;
    notify_customer_whatsapp: boolean;
    statuses: { key: string; label: string; enabled: boolean }[];
  };
  customers: { require_email: boolean };
  loyalty: {
    enabled: boolean;
    threshold: Money;
    points_per: number;
    min_spend: Money;
    point_value: Money;
    redemption_enabled: boolean;
    min_redemption_points: number;
    referral_bonus_percent: number;
    expiry_days: number;
    tiers: Tier[];
    award_winners: number;
    show_on_portal: boolean;
    show_value_on_portal: boolean;
    /** Roadmap 106 */
    earn_base: "net" | "gross";
    earn_on_redeemed: boolean;
    rounding: "down" | "nearest";
    redemption_window_open: boolean;
    redemption_opens_at: string | null;
    redemption_closes_at: string | null;
    max_redemption_percent: number;
    redemption_needs_approval: boolean;
  };
  expenses: { require_attachment: boolean; require_description: boolean };
  reports: { hide_financials_without_permission: boolean; medals: MedalSettings };
  notifications: { whatsapp_receipts: boolean; whatsapp_credit_reminders: boolean; whatsapp_loyalty: boolean };
  workspace: { hours: Hours; outside_hours: "allow" | "block"; location: { mode: "anywhere" | "branch"; areas: string[] } };
  /** Roadmap 108 */
  experience?: { landing: string };
  /** Roadmap 112: partial white labelling. */
  /** Roadmap 121: closing the books. */
  books?: { closed_until?: string | null };
  branding?: { enabled: boolean; login: boolean; login_marketing: boolean; sidebar: boolean; dashboard: boolean; pos: boolean; documents: boolean; customer: boolean; website: boolean; favicon: boolean };
}

export interface Profile {
  user: { id: Id; name: string; email: string; role: string; all_branches: boolean; platform_admin: boolean; preferences: Preferences;
    /** Signed in with a one-time PIN: a new PIN is required before anything else (roadmap 59). */
    must_change_pin?: boolean;
    /** Roadmap 105: offer Quick PIN once after the person set up their account. */
    quick_pin_offer?: boolean;
    /** Current Branch after sign-in when the user works at several branches. */
    default_branch_id?: Id | null };
  tenant: { id: Id; name: string; slug: string; tagline: string; currency: string; logo_url: string | null; is_demo: boolean; timezone: string; previous_slugs?: string[] };
  branches: Branch[];
  permissions: string[];
  settings: Settings;
  /** Roadmap 91: STK Push with the business's own verified M-Pesa account; `mpesa_stk_branches` null = every branch. */
  integrations: { mpesa_stk: boolean; mpesa_stk_branches?: string[] | null; whatsapp: boolean };
  /** Package and billing access (roadmap 41–45). `modules` null = every module. */
  billing?: {
    modules: string[] | null;
    suspended: boolean;
    ownership: "customer" | "platform";
    access_mode: "billed" | "free" | "trial" | null;
    trial_end: string | null;
    catalogue: { key: string; label: string; perms: string[] }[];
  };
  /** Present while a platform admin works inside another business. */
  acting: {
    home_tenant_id: Id;
    home_tenant_name: string;
    /** Roadmap 71: the support session in force (banner with countdown). */
    support: { id: Id; reason: string; scope: "view" | "full"; expires_at: string } | null;
  } | null;
  /** Roadmap 72: businesses of the same tenant this person can switch to (with the current one). */
  businesses?: { id: Id; name: string; slug?: string; current: boolean }[];
  /** Signs in through their account in this other business of the tenant. */
  linked_from?: string | null;
  /** Roadmap 83: this session was opened with a Quick PIN (sensitive actions need a full sign-in). */
  quick?: boolean;
  /** Roadmap 87: automatic logout after this many seconds without activity, with a warning this long before. */
  session?: { idle_seconds: number; warning_seconds: number };
}

export interface Product {
  id: Id;
  code: string;
  name: string;
  nickname: string;
  description: string;
  category_id: Id | null;
  category_name: string | null;
  supplier_id: Id | null;
  supplier_name: string | null;
  /** Roadmap 131: null = no selling price set yet. */
  marked_price: Money | null;
  max_discount: Money | null;
  cost_price: Money | null;
  barcode: string | null;
  track_items: boolean;
  is_active: boolean;
  available_for_orders: boolean;
  transfer_allowed: boolean;
  loyalty_eligible: boolean;
  loyalty_threshold: Money | null;
  loyalty_points_per: number | null;
  low_stock_threshold: number | null;
  all_branches: boolean;
  custom_fields: Record<string, unknown>;
  on_hand: number;
  reserved: number;
  available: number;
  primary_photo_id: Id | null;
  photo_count: number;
  updated_at: string;
}

export interface PosProduct {
  id: Id;
  code: string;
  name: string;
  nickname: string;
  category_name: string | null;
  barcode: string | null;
  track_items: boolean;
  /** Roadmap 131: null = no selling price set yet. */
  marked_price: Money | null;
  max_discount: Money | null;
  loyalty_eligible: boolean;
  loyalty_threshold: Money | null;
  loyalty_points_per: number | null;
  on_hand: number;
  reserved: number;
  available: number;
  primary_photo_id: Id | null;
  photo_count: number;
  /** Roadmap 110: sales performance rank at this branch (1 = top) and its score. */
  rank?: number | null;
  rank_score?: Money | null;
}

export interface Customer {
  id: Id;
  mobile: string;
  first_name: string;
  other_names: string;
  nickname: string;
  email: string;
  custom_fields: Record<string, unknown>;
  total_spend: Money;
  purchase_count: number;
  last_purchase_at: string | null;
  own_points: number;
  referral_points: number;
  points_redeemed: number;
  points_expired: number;
  points_available: number;
  tier: string;
  is_active: boolean;
  credit_balance: Money;
  created_at: string;
}

export interface CustomField {
  id: Id;
  key: string;
  label: string;
  field_type: "text" | "number" | "date" | "dropdown" | "boolean" | "email";
  options: string[];
  required: boolean;
  is_active: boolean;
  display_order: number;
}

export interface SaleRow {
  id: Id;
  receipt_no: string;
  created_at: string;
  /** Trading day the sale counts for (YYYY-MM-DD); differs from the calendar date for late trading after midnight. */
  business_date: string;
  branch_name: string;
  customer_id: Id | null;
  customer_name: string | null;
  customer_mobile: string | null;
  /** Sale Owner (credited). */
  user_name: string | null;
  owner_id?: Id | null;
  recorded_by_name?: string | null;
  status: string;
  total: Money;
  discount_total: Money;
  payment_method: string;
  /** Roadmap 107: paid · partially_paid · credit_outstanding · refunded · cancelled … */
  payment_status?: string;
  points_earned: number;
  item_count: number;
  order_no: string | null;
  is_legacy: boolean;
}

export interface SaleDetail {
  sale: {
    id: Id;
    receipt_no: string;
    created_at: string;
    status: string;
    branch_id: Id;
    branch_name: string;
    branch_location: string;
    branch_phone: string;
    /** Sale Owner — credited with the sale (roadmap 62). */
    user_name: string | null;
    owner_id: Id | null;
    /** Who physically entered the sale. */
    recorded_by_name: string | null;
    business_date: string;
    /** Roadmap 115: recorded with an earlier sale date (business_date is that date). */
    backdated?: boolean;
    gross_total: Money;
    discount_total: Money;
    total: Money;
    redeemed_points: number;
    redeemed_value: Money;
    amount_paid: Money;
    payment_method: string;
    payment_status?: string;
    points_earned: number;
    notes: string;
    is_legacy: boolean;
    approved_by_name: string | null;
    cancel_reason: string | null;
    order_id: Id | null;
    order_no: string | null;
    customer: { id: Id; name: string; nickname: string; mobile: string; points_available: number } | null;
  };
  items: {
    id: Id;
    product_id: Id;
    product_name: string;
    /** Roadmap 115: this line's own date when it differs from the sale's. */
    sale_date?: string | null;
    product_code: string;
    /** Roadmap 136: an individually tracked unit's barcode arrives masked (`****047`). */
    barcode: string | null;
    tracked?: boolean;
    quantity: number;
    returned_qty: number;
    marked_price: Money;
    unit_price: Money;
    discount: Money;
    line_total: Money;
    points: number;
    unit_cost: Money | null;
  }[];
  payments: { id: Id; method: string; amount: Money; reference: string; created_at: string; user_name: string | null }[];
  returns: {
    id: Id;
    return_no: string;
    kind: string;
    reason: string;
    refund_amount: Money;
    refund_method: string;
    restock: boolean;
    points_reversed: number;
    /** Points already redeemed by the customer, so not taken back (roadmap 67). */
    points_unrecovered?: number;
    created_at: string;
    user_name: string | null;
  }[];
  credit: { id: Id; original_amount: Money; amount_paid: Money; adjustments: Money; balance: Money; due_date: string; status: string } | null;
  business: { name: string; phone: string; address: string; currency: string; logo_url: string | null; receipt_footer: string };
  pending_approval_id: Id | null;
  owner_changes?: { id: Id; from_owner: string | null; to_owner: string | null; reason: string; status: string; requested_by: string | null; decided_by: string | null; decided_at: string | null; created_at: string; approval_id: Id | null }[];
}

export interface CreditRow {
  id: Id;
  sale_id: Id;
  receipt_no: string;
  customer_id: Id;
  customer_name: string;
  customer_mobile: string;
  branch_name: string;
  salesperson: string | null;
  original_amount: Money;
  amount_paid: Money;
  adjustments: Money;
  balance: Money;
  due_date: string;
  days_outstanding: number;
  status: string;
  created_at: string;
  recall_state: "partially_recalled" | "recalled" | null;
}

export interface OrderRow {
  id: Id;
  order_no: string;
  status: string;
  source: string;
  branch_id: Id;
  branch_name: string;
  customer_id: Id;
  customer_name: string;
  customer_mobile: string;
  delivery_location: string;
  notes: string;
  total: Money;
  item_count: number;
  reserved: boolean;
  /** Imported order history: shown, never moved again. */
  is_legacy?: boolean;
  sale_id: Id | null;
  receipt_no: string | null;
  track_token: Id;
  created_at: string;
  updated_at: string;
}

export interface StockLevel {
  product_id: Id;
  code: string;
  name: string;
  nickname: string;
  category_name: string | null;
  track_items: boolean;
  is_active: boolean;
  /** Roadmap 131: null = no selling price set yet. */
  marked_price: Money | null;
  cost_price: Money | null;
  on_hand: number;
  reserved: number;
  available: number;
  low_threshold: number;
  value: Money | null;
  primary_photo_id: Id | null;
  /** Roadmap 98: has a product-level barcode. */
  has_barcode?: boolean;
}

export interface Approval {
  id: Id;
  action: string;
  entity_type: string;
  entity_id: Id;
  branch_id: Id | null;
  branch_name: string | null;
  summary: string;
  amount: Money | null;
  status: string;
  requested_by_name: string | null;
  decided_by_name: string | null;
  decided_at: string | null;
  comments: string;
  created_at: string;
  can_decide: boolean;
  /** Who approves next (pending requests). */
  next_approvers: string[];
  /** Roadmap 148: who the current step is assigned to ("Administrator", "Role: Manager", …). */
  assigned_to: string;
  /** Why the viewer cannot decide it (pending only), with the explanation. */
  why: "own_request" | "decided_earlier_level" | "not_in_business" | "not_designated" | "no_permission" | null;
  why_message: string;
  /** Nobody can decide the current step. */
  stuck: "only_requester" | "no_one" | null;
  /** Set when a workflow change needed an exception for this request (roadmap 49). */
  sync_note: string;
  synced_at: string | null;
  level: number;
  levels: number;
  decisions: { level: number; step_id?: string | null; user_name: string; decision: "approved" | "rejected"; comments: string; at: string }[];
  /** The chain of steps the request follows (ids are stable across workflow edits). */
  steps: { id?: string | null }[];
}

export interface Notification {
  id: Id;
  kind: string;
  title: string;
  body: string;
  link: string;
  read_at: string | null;
  created_at: string;
}

export interface Category {
  id: Id;
  name: string;
  is_active: boolean;
  product_count?: number;
}

export interface Supplier {
  id: Id;
  name: string;
  phone: string;
  email: string;
  notes: string;
  is_active: boolean;
}

export interface UserRow {
  id: Id;
  name: string;
  email: string;
  phone: string;
  role_id: Id;
  role_name: string;
  is_active: boolean;
  all_branches: boolean;
  branch_ids: Id[];
  last_login_at: string | null;
  created_at: string;
  /** Roadmap 72: signs in through their account in this other business of the tenant (no PIN here). */
  linked_from?: string | null;
  /** Roadmap 83: has a Quick Login PIN. */
  quick_pin?: boolean;
  /** Roadmap 105: invitation state (the account is not set up until the person creates their PIN). */
  status?: "active" | "invitation_pending" | "invitation_sent" | "activation_pending" | "invitation_expired" | "disabled";
  invite_sent_at?: string | null;
  invite_expires_at?: string | null;
}

export interface Role {
  id: Id;
  name: string;
  description: string;
  permissions: string[];
  is_system: boolean;
  is_active: boolean;
  user_count: number;
}

export interface MedalTargets {
  basis: "revenue" | "units";
  gold: Money;
  silver: Money;
  bronze: Money;
}

export interface MedalSettings {
  mode: "rank" | "targets";
  products: MedalTargets;
  staff: MedalTargets;
}
