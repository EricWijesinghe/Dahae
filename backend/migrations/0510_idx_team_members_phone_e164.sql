-- no-transaction
-- Unique E.164 phone (0507), like the UNIQUE on team_members (pro_id, phone). One CREATE INDEX CONCURRENTLY per file (see 0041).
-- Failed run: DROP INDEX CONCURRENTLY IF EXISTS team_members_phone_e164_unique, then restart.
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS team_members_phone_e164_unique ON team_members (pro_id, phone_e164) WHERE phone_e164 IS NOT NULL;
