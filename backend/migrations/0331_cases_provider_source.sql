-- Cases opened from the payment provider's own events (a cancellation made in its console).
ALTER TABLE cases DROP CONSTRAINT cases_source_check;
ALTER TABLE cases ADD CONSTRAINT cases_source_check CHECK (source IN ('manual', 'ai', 'no_show', 'provider')) NOT VALID;
ALTER TABLE cases VALIDATE CONSTRAINT cases_source_check;
