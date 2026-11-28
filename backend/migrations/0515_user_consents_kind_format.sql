-- Consent kinds come from the country pack: the fixed list becomes a format check under the same name (the pattern
-- 0095 used). Every kind an older release writes matches, and an older release reads an unknown kind as not granted.
-- NOT VALID here, validated in 0516 under a lighter lock (user_consents is a big table).
ALTER TABLE user_consents DROP CONSTRAINT user_consents_kind_check;
ALTER TABLE user_consents ADD CONSTRAINT user_consents_kind_check CHECK (kind ~ '^[a-z_]{1,40}$') NOT VALID;
