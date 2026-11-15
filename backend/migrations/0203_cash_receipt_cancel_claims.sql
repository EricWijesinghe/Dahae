-- Cash receipt cancels. `cancel_sent_at` is stamped before the provider call, so a provider success is never
-- sent twice; a row still stamped after a failed local write is counted for a manual check.
-- `cancel_attempts` and `next_cancel_at` space out retries after a provider refusal.
ALTER TABLE cash_receipts
    ADD COLUMN cancel_sent_at  TIMESTAMPTZ,
    ADD COLUMN cancel_attempts INT NOT NULL DEFAULT 0,
    ADD COLUMN next_cancel_at  TIMESTAMPTZ;
CREATE INDEX cash_receipts_issued_updated_idx ON cash_receipts (updated_at) WHERE state = 'issued';
