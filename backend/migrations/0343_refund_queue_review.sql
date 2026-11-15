-- A queued refund the provider refused for good (its balance moved) waits for staff; the drain skips it.
ALTER TABLE refund_queue ADD COLUMN review_required BOOLEAN NOT NULL DEFAULT false;
