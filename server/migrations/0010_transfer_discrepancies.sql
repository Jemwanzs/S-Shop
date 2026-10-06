-- Roadmap 18: transfer receipt with discrepancies. The receiving branch records units that did not arrive (short)
-- or arrived damaged; received = quantity - short - damaged. Both are written to the ledger at the destination
-- (transfer_in for the full quantity, then loss / damage), so every dispatched unit stays accounted for.
ALTER TABLE transfer_items ADD COLUMN short_qty int NOT NULL DEFAULT 0 CHECK (short_qty >= 0);
ALTER TABLE transfer_items ADD COLUMN damaged_qty int NOT NULL DEFAULT 0 CHECK (damaged_qty >= 0);
ALTER TABLE transfer_items ADD CONSTRAINT transfer_items_discrepancy_within_qty CHECK (short_qty + damaged_qty <= quantity);
ALTER TABLE transfers ADD COLUMN discrepancy_reason text NOT NULL DEFAULT '';
