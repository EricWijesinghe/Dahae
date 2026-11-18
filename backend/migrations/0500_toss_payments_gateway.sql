-- Toss Payments next to PortOne: each checkout and saved card keeps the gateway it started on, so confirm, webhooks,
-- refunds and charges never follow a later switch. Defaults keep every existing row on PortOne. Older releases ignore the
-- columns, so a rollback is safe (constant defaults: no table rewrite).
ALTER TABLE checkouts ADD COLUMN gateway TEXT NOT NULL DEFAULT 'portone';
-- Toss charges a billing key with the customerKey it was issued for (random, one per card).
ALTER TABLE payment_methods ADD COLUMN gateway TEXT NOT NULL DEFAULT 'portone', ADD COLUMN customer_key TEXT;
