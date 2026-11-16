-- no-transaction
-- Finance day-range filters (finance.rs EVENTS_SQL) scan settlements by created_at.
-- One CREATE INDEX CONCURRENTLY per file (see 0043). Failed run: DROP INDEX CONCURRENTLY IF EXISTS, restart.
CREATE INDEX CONCURRENTLY IF NOT EXISTS settlements_created_idx ON settlements (created_at);
