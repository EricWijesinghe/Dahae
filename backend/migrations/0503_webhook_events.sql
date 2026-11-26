-- Ledger of provider webhook deliveries already handled, one row per (adapter, event). A handler records the event
-- before it acts, so a redelivered event is acknowledged and changes nothing. Rows older than 30 days are purged
-- (retention.rs). Unread by older releases, so a rollback is safe.
CREATE TABLE webhook_events (
    adapter_id  TEXT NOT NULL,
    event_id    TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (adapter_id, event_id)
);
CREATE INDEX webhook_events_received_idx ON webhook_events (received_at);
