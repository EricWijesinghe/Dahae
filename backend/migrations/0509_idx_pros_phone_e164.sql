-- no-transaction
-- Unique E.164 phone (0507), like the UNIQUE on pros.phone. One CREATE INDEX CONCURRENTLY per file (see 0041).
-- Failed run: DROP INDEX CONCURRENTLY IF EXISTS pros_phone_e164_unique, then restart.
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS pros_phone_e164_unique ON pros (phone_e164) WHERE phone_e164 IS NOT NULL;
