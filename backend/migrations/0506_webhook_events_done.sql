-- When a delivery's work finished. A row without it is a claim in progress: a redelivery gets a retryable answer,
-- and a claim older than the request timeout (its request was cut off) is taken again. Nullable, so older
-- releases, which ignore the column, keep working after a rollback.
ALTER TABLE webhook_events ADD COLUMN done_at TIMESTAMPTZ;
