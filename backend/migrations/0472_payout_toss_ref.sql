-- refPayoutId sent to Toss for this payout. NULL means the payout id (every send before this column).
-- Toss never accepts a refPayoutId twice, so a retry after Toss definitively failed the transfer gets a new one.
ALTER TABLE payouts ADD COLUMN toss_ref text;
