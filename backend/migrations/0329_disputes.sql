-- Cancellations PortOne reports that the app did not make (a refund issued in the PortOne console, or a
-- partial one). One row per provider transaction: a replayed webhook finds its row and changes nothing.
-- Staff resolve the row from the console case it opens.
CREATE TABLE disputes (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id     UUID NOT NULL REFERENCES payments(id),
    provider_tx_id TEXT NOT NULL UNIQUE,
    kind           TEXT NOT NULL CHECK (kind IN ('Transaction.Cancelled', 'Transaction.PartialCancelled')),
    state          TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'resolved')),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX disputes_open_idx ON disputes (created_at) WHERE state = 'open';
