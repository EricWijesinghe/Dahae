-- no-transaction
-- One payments row per Toss order, like 0121 for PortOne. One CREATE INDEX CONCURRENTLY per file (see 0041).
-- Failed run: DROP INDEX CONCURRENTLY IF EXISTS payments_toss_tx_uidx; restart.
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS payments_toss_tx_uidx ON payments (provider, provider_tx_id)
  WHERE provider IN ('tosspayments', 'tosspayments_billing') AND provider_tx_id NOT LIKE 'dev:%';
