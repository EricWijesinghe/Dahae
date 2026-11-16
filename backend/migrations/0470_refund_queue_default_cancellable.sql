-- v1.4 binaries queue a compensating refund without `cancellable` (0330 added it after v1.4 was cut). Fill it from
-- the amount before the NOT NULL check, so a rollback to v1.4 can still queue the refund.
CREATE FUNCTION refund_queue_default_cancellable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.cancellable IS NULL THEN
        NEW.cancellable := NEW.amount;
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER refund_queue_default_cancellable BEFORE INSERT ON refund_queue
    FOR EACH ROW EXECUTE FUNCTION refund_queue_default_cancellable();
