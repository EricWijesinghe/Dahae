-- Checks the rows already in checkouts and payments against the currency format. Runs in its own transaction so
-- the scan takes only a SHARE UPDATE EXCLUSIVE lock, which does not block reads or writes.
ALTER TABLE checkouts VALIDATE CONSTRAINT checkouts_currency_check;
ALTER TABLE payments VALIDATE CONSTRAINT payments_currency_check;
