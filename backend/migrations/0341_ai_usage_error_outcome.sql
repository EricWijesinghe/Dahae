-- AI usage: a failed model call is recorded as 'error', so it spends no cap and opens no complaint case.
-- NOT VALID adds the rule for new rows without scanning ai_usage; 0342 checks the existing rows.
ALTER TABLE ai_usage DROP CONSTRAINT ai_usage_outcome_check;
ALTER TABLE ai_usage ADD CONSTRAINT ai_usage_outcome_check CHECK (outcome IN ('answered','handoff','error')) NOT VALID;
