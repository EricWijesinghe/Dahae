-- Phone numbers in E.164 (+821012345678) next to each stored phone, step one of two: the release writes both
-- columns and a startup job fills the old rows (api/src/phone.rs); logins still read the old columns. Nullable,
-- so older releases, which ignore the new columns, keep working after a rollback.
ALTER TABLE users ADD COLUMN phone_e164 TEXT;
ALTER TABLE pros ADD COLUMN phone_e164 TEXT;
ALTER TABLE team_members ADD COLUMN phone_e164 TEXT;
ALTER TABLE otp_codes ADD COLUMN phone_e164 TEXT;
ALTER TABLE referrals ADD COLUMN referred_phone_e164 TEXT;
ALTER TABLE notification_outbox ADD COLUMN phone_e164 TEXT;
ALTER TABLE staff ADD COLUMN phone_e164 TEXT, ADD COLUMN alert_phone_e164 TEXT;

-- An older release changes a phone without the E.164 column: clear the copy so it never names the old number,
-- and the startup job fills it again from the new one.
CREATE FUNCTION phone_e164_stale() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.phone IS DISTINCT FROM OLD.phone AND NEW.phone_e164 IS NOT DISTINCT FROM OLD.phone_e164 THEN
        NEW.phone_e164 := NULL;
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER users_phone_e164_stale BEFORE UPDATE OF phone ON users
    FOR EACH ROW EXECUTE FUNCTION phone_e164_stale();
CREATE TRIGGER pros_phone_e164_stale BEFORE UPDATE OF phone ON pros
    FOR EACH ROW EXECUTE FUNCTION phone_e164_stale();
CREATE TRIGGER team_members_phone_e164_stale BEFORE UPDATE OF phone ON team_members
    FOR EACH ROW EXECUTE FUNCTION phone_e164_stale();
CREATE TRIGGER otp_codes_phone_e164_stale BEFORE UPDATE OF phone ON otp_codes
    FOR EACH ROW EXECUTE FUNCTION phone_e164_stale();
CREATE TRIGGER notification_outbox_phone_e164_stale BEFORE UPDATE OF phone ON notification_outbox
    FOR EACH ROW EXECUTE FUNCTION phone_e164_stale();
CREATE TRIGGER staff_phone_e164_stale BEFORE UPDATE OF phone ON staff
    FOR EACH ROW EXECUTE FUNCTION phone_e164_stale();

CREATE FUNCTION alert_phone_e164_stale() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.alert_phone IS DISTINCT FROM OLD.alert_phone AND NEW.alert_phone_e164 IS NOT DISTINCT FROM OLD.alert_phone_e164 THEN
        NEW.alert_phone_e164 := NULL;
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER staff_alert_phone_e164_stale BEFORE UPDATE OF alert_phone ON staff
    FOR EACH ROW EXECUTE FUNCTION alert_phone_e164_stale();

CREATE FUNCTION referred_phone_e164_stale() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.referred_phone IS DISTINCT FROM OLD.referred_phone AND NEW.referred_phone_e164 IS NOT DISTINCT FROM OLD.referred_phone_e164 THEN
        NEW.referred_phone_e164 := NULL;
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER referrals_referred_phone_e164_stale BEFORE UPDATE OF referred_phone ON referrals
    FOR EACH ROW EXECUTE FUNCTION referred_phone_e164_stale();

-- Unique where the source column is unique. The three big tables get theirs concurrently in 0508-0510.
CREATE UNIQUE INDEX staff_phone_e164_unique ON staff (phone_e164) WHERE phone_e164 IS NOT NULL;
CREATE UNIQUE INDEX referrals_referred_phone_e164_unique ON referrals (referred_phone_e164) WHERE referred_phone_e164 IS NOT NULL;
