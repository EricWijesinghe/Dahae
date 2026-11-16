-- Card lock for the console payment keys. A saved card is locked unless an owner unlock is still open.
-- unlocked_until: window end set by an owner unlock, cleared by save or lock. unlocked_by: the owner who opened it.
ALTER TABLE integration_settings ADD COLUMN unlocked_until timestamptz;
ALTER TABLE integration_settings ADD COLUMN unlocked_by uuid REFERENCES staff(id);
