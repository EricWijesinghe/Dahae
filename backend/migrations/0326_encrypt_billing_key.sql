-- Saved-card billing keys are stored encrypted (crypto::seal_billing_key). One live card per key moves to a
-- keyed digest, because the ciphertext of one key differs on every write. Existing rows are sealed and get
-- their digest at startup (billing_keys::seal_stored), before the server takes traffic.
-- Expand only: the previous release's unique index stays until the contract migration that follows the
-- rollback window (docs/DEPLOY.md, migrations stay additive). Rolling back after the first start is not safe
-- for saved cards; see the release note in docs/DEPLOY.md.
ALTER TABLE payment_methods ADD COLUMN billing_key_digest TEXT;
