-- no-transaction
-- Finance day-range filters (finance.rs EVENTS_SQL) scan payments by created_at.
-- One CREATE INDEX CONCURRENTLY per file (see 0043). Failed run: DROP INDEX CONCURRENTLY IF EXISTS, restart.
CREATE INDEX CONCURRENTLY IF NOT EXISTS payments_created_idx ON payments (created_at);
