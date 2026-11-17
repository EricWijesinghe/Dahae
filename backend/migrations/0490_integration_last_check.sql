-- The latest connection check of a setup card, passed or failed (routes/setup.rs test_section). last_test stays
-- the gate record: a failed Toss payout check does not replace its pass, so the console reads this column to show
-- what the newest check said. Nullable and unread by older releases, so a rollback is safe.
ALTER TABLE integration_settings ADD COLUMN last_check JSONB;
