-- no-transaction
-- One live card per billing key digest. Built concurrently so saved-card writes are not blocked on a big table.
-- If a run dies, the index is left INVALID: DROP INDEX CONCURRENTLY IF EXISTS payment_methods_billing_key_digest_live;
-- then restart the API so the migration runs again.
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS payment_methods_billing_key_digest_live ON payment_methods (billing_key_digest) WHERE deleted_at IS NULL;
