-- no-transaction
-- Member sign-in matches the E.164 copy once the phone read switch is on, like team_members_phone_idx for the old
-- column (the unique index from 0510 leads with pro_id). One CREATE INDEX CONCURRENTLY per file (see 0041).
-- Failed run: DROP INDEX CONCURRENTLY IF EXISTS team_members_phone_e164_idx, then restart.
CREATE INDEX CONCURRENTLY IF NOT EXISTS team_members_phone_e164_idx ON team_members (phone_e164) WHERE phone_e164 IS NOT NULL;
