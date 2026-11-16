-- audit_log and user_consents are evidence: rows are written once and never changed or removed.
-- Same guard as points_ledger (0090). Demo clear is the one deliberate exception and switches these
-- triggers off inside its own transaction (demo/mod.rs).
CREATE FUNCTION append_only_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is append-only', TG_TABLE_NAME;
END
$$;

CREATE TRIGGER audit_log_no_change BEFORE UPDATE OR DELETE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION append_only_guard();
CREATE TRIGGER audit_log_no_truncate BEFORE TRUNCATE ON audit_log
    FOR EACH STATEMENT EXECUTE FUNCTION append_only_guard();

CREATE TRIGGER user_consents_no_change BEFORE UPDATE OR DELETE ON user_consents
    FOR EACH ROW EXECUTE FUNCTION append_only_guard();
CREATE TRIGGER user_consents_no_truncate BEFORE TRUNCATE ON user_consents
    FOR EACH STATEMENT EXECUTE FUNCTION append_only_guard();
