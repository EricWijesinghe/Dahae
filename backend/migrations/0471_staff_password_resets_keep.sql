-- 0180 dropped staff_password_resets, but the previous release still reads and writes it on rollback.
-- Recreate it (empty; unused tokens expire in 30 minutes) so both releases work until a later release drops it.
CREATE TABLE IF NOT EXISTS staff_password_resets (
    token_hash TEXT PRIMARY KEY,
    staff_id   UUID NOT NULL REFERENCES staff(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS staff_password_resets_staff_idx ON staff_password_resets (staff_id);
