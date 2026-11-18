-- The gateway a subscription visit's stored charge id (payment_ref) was sent to. The card can change while a charge
-- is open (resume with a new card); lookups and refunds of that id must stay on the gateway that saw it. NULL is a
-- charge sent before this column existed, which was PortOne.
-- Rollback note for 0500-0502: the schema change is safe, but a release from before Toss looks Toss checkouts up at
-- PortOne (and expires them) and cannot refund Toss payments. Before rolling back, switch Toss off and wait until no
-- Toss checkout is pending or awaiting deposit and no Toss refund is queued.
ALTER TABLE subscription_visits ADD COLUMN payment_provider TEXT;
