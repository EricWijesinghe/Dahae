-- The currency a checkout or payment was made in (ISO 4217). Rows from before this column, and inserts by an older
-- release, read as KRW, which is right on a KR install; a confirm on any other install fails closed on the
-- currency check. The format check is added NOT VALID here and validated in 0505, because both tables are big.
ALTER TABLE checkouts ADD COLUMN currency CHAR(3) NOT NULL DEFAULT 'KRW';
ALTER TABLE payments ADD COLUMN currency CHAR(3) NOT NULL DEFAULT 'KRW';
ALTER TABLE checkouts ADD CONSTRAINT checkouts_currency_check CHECK (currency ~ '^[A-Z]{3}$') NOT VALID;
ALTER TABLE payments ADD CONSTRAINT payments_currency_check CHECK (currency ~ '^[A-Z]{3}$') NOT VALID;
