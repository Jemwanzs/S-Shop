-- Roadmap 171: "Others" — a stock adjustment with an explicit direction and a written explanation (kept in `reason`).
ALTER TABLE stock_adjustments DROP CONSTRAINT stock_adjustments_kind_check;
ALTER TABLE stock_adjustments ADD CONSTRAINT stock_adjustments_kind_check
    CHECK (kind IN ('count','damage','loss','customer_return','supplier_return','manual','write_off','other'));
