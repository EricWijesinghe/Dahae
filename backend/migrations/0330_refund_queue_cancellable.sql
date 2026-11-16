-- The gateway balance a queued refund expects. Queued refunds are full refunds, so it is the amount itself.
-- PortOne cancels only while its balance still matches, so a refund never lands on top of a cancel made elsewhere.
ALTER TABLE refund_queue ADD COLUMN cancellable BIGINT;
UPDATE refund_queue SET cancellable = amount;
ALTER TABLE refund_queue ALTER COLUMN cancellable SET NOT NULL;
ALTER TABLE refund_queue ADD CONSTRAINT refund_queue_cancellable_check CHECK (cancellable >= amount);
