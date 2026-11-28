-- Checks the rows already in user_consents against 0515's format. Its own transaction, so the scan takes only a
-- SHARE UPDATE EXCLUSIVE lock, which does not block reads or writes.
ALTER TABLE user_consents VALIDATE CONSTRAINT user_consents_kind_check;
