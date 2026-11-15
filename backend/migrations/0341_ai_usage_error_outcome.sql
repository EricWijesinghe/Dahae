-- AI usage: a failed model call is recorded as 'error', so it spends no cap and opens no complaint case.
ALTER TABLE ai_usage DROP CONSTRAINT ai_usage_outcome_check;
ALTER TABLE ai_usage ADD CONSTRAINT ai_usage_outcome_check CHECK (outcome IN ('answered','handoff','error'));
