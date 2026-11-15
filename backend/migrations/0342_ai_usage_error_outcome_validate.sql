-- Checks the rows already in ai_usage against the outcome rule. Runs in its own transaction so the
-- scan takes only a SHARE UPDATE EXCLUSIVE lock, which does not block reads or writes.
ALTER TABLE ai_usage VALIDATE CONSTRAINT ai_usage_outcome_check;
