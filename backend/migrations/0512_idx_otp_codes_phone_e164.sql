-- no-transaction
-- OTP reads match the E.164 copy once the phone read switch is on (api/src/phone.rs reads_ready), like otp_codes_phone_idx
-- for the old column. One CREATE INDEX CONCURRENTLY per file (see 0041).
-- Failed run: DROP INDEX CONCURRENTLY IF EXISTS otp_codes_phone_e164_idx, then restart.
CREATE INDEX CONCURRENTLY IF NOT EXISTS otp_codes_phone_e164_idx ON otp_codes (phone_e164, created_at DESC) WHERE phone_e164 IS NOT NULL;
