-- Saved-card billing keys are stored encrypted (crypto::seal_billing_key). One live card per key moves to a
-- keyed digest, because the ciphertext of one key differs on every write. Existing rows are sealed and get
-- their digest at startup (billing_keys::seal_stored), before the server takes traffic.
ALTER TABLE payment_methods ADD COLUMN billing_key_digest TEXT;
DROP INDEX payment_methods_billing_key_live;
CREATE UNIQUE INDEX payment_methods_billing_key_digest_live ON payment_methods (billing_key_digest) WHERE deleted_at IS NULL;
