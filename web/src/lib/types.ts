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
  product: { max_photos: number; auto_code_prefix: string };
  stock: {
    barcode_requirement: "required" | "optional" | "disabled";
    quantity_entry: "editable" | "locked";
    capture_cost: boolean;
    valuation: "cost" | "selling";
    low_stock_threshold: number;
    allow_negative: boolean;
    transfer_receipt_control: boolean;
  };
  sales: {
    quantity_entry: "editable" | "locked";
    require_barcode_clearance: boolean;
    payment_methods: PaymentMethod[];
    mpesa_manual_confirmation: boolean;
    credit_enabled: boolean;
    credit_default_days: number;
    receipt_footer: string;
  };
  orders: {
    portal_enabled: boolean;
    default_branch_id: Id | null;
    reserve_stock: boolean;
    sale_on_status: "delivered" | "completed";
    verify_with_otp: boolean;
    show_out_of_stock: boolean;
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
  };
  expenses: { require_attachment: boolean; require_description: boolean };
  reports: { hide_financials_without_permission: boolean; medals: MedalSettings };
  notifications: { whatsapp_receipts: boolean; whatsapp_credit_reminders: boolean; whatsapp_loyalty: boolean };
  workspace: { hours: Hours; outside_hours: "allow" | "block"; location: { mode: "anywhere" | "branch"; areas: string[] } };
}

export interface Profile {
  user: { id: Id; name: string; email: string; role: string; all_branches: boolean; platform_admin: boolean; preferences: Preferences };
  tenant: { id: Id; name: string; slug: string; tagline: string; currency: string; logo_url: string | null; is_demo: boolean; timezone: string };
  branches: Branch[];
  permissions: string[];
  settings: Settings;
  integrations: { mpesa_stk: boolean; whatsapp: boolean };
  /** Present while a platform admin works inside another business. */
  acting: { home_tenant_id: Id; home_tenant_name: string } | null;
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
  marked_price: Money;
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
  marked_price: Money;
  max_discount: Money | null;
  loyalty_eligible: boolean;
  loyalty_threshold: Money | null;
  loyalty_points_per: number | null;
  on_hand: number;
  reserved: number;
  available: number;
  primary_photo_id: Id | null;
  photo_count: number;
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
  user_name: string | null;
  status: string;
  total: Money;
  discount_total: Money;
  payment_method: string;
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
    user_name: string | null;
    gross_total: Money;
    discount_total: Money;
    total: Money;
    redeemed_points: number;
    redeemed_value: Money;
    amount_paid: Money;
    payment_method: string;
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
    product_code: string;
    barcode: string | null;
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
    created_at: string;
    user_name: string | null;
  }[];
  credit: { id: Id; original_amount: Money; amount_paid: Money; adjustments: Money; balance: Money; due_date: string; status: string } | null;
  business: { name: string; phone: string; address: string; currency: string; logo_url: string | null; receipt_footer: string };
  pending_approval_id: Id | null;
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
  marked_price: Money;
  cost_price: Money | null;
  on_hand: number;
  reserved: number;
  available: number;
  low_threshold: number;
  value: Money | null;
  primary_photo_id: Id | null;
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
  level: number;
  levels: number;
  decisions: { level: number; user_name: string; decision: "approved" | "rejected"; comments: string; at: string }[];
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
