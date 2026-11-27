-- no-transaction
-- Unique E.164 phone (0507), like the UNIQUE on users.phone. One CREATE INDEX CONCURRENTLY per file (see 0041).
-- Failed run: DROP INDEX CONCURRENTLY IF EXISTS users_phone_e164_unique, then restart.
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS users_phone_e164_unique ON users (phone_e164) WHERE phone_e164 IS NOT NULL;
