-- Professional inventory: scope by place, operator on scans, archived barcode resolution

ALTER TABLE inventory_sessions
    ADD COLUMN scope_place SMALLINT;

ALTER TABLE inventory_scans
    ADD COLUMN scanned_by BIGINT REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE inventory_scans DROP CONSTRAINT inventory_scans_result_chk;

ALTER TABLE inventory_scans ADD CONSTRAINT inventory_scans_result_chk CHECK (
    result IN ('found', 'unknown_barcode', 'found_archived')
);

CREATE INDEX idx_inventory_scans_scanned_by ON inventory_scans (scanned_by);
