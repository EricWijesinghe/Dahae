-- Payee identifier for the 지급명세서 (주민등록번호 or 사업자등록번호). Encrypted with crypto.rs, AAD-bound.
-- The kind and the last four digits stay in clear so the console can show a masked value without a decrypt.
-- A profile update that omits the identifier keeps the stored one; it is only replaced, never silently cleared.
ALTER TABLE pro_profiles
    ADD COLUMN payee_identifier_enc   TEXT,
    ADD COLUMN payee_identifier_kind  TEXT CHECK (payee_identifier_kind IN ('rrn', 'business')),
    ADD COLUMN payee_identifier_last4 TEXT,
    ADD CONSTRAINT pro_profiles_payee_identifier_complete CHECK (
        (payee_identifier_enc IS NULL) = (payee_identifier_kind IS NULL)
        AND (payee_identifier_enc IS NULL) = (payee_identifier_last4 IS NULL)
    );
