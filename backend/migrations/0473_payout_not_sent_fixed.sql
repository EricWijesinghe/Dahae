-- The owner's "not sent" on an ambiguous Toss send. The idempotency key and refPayoutId are kept, and Toss may
-- still hold that send, so while this is set the batch keeps its amount (cancels wait) and a refused re-send
-- leaves it ambiguous. Cleared when Toss reports that transfer failed.
ALTER TABLE payouts ADD COLUMN owner_not_sent_at timestamptz;
-- Transfer memo (transactionDescription) as first sent to Toss: a re-send under the same key carries the same body.
ALTER TABLE payouts ADD COLUMN toss_memo text;
