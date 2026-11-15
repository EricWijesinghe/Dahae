-- Cleaner verification record: the checklist the owner completed before a pro is verified.
-- One row per pro; a later decision overwrites it (the audit log keeps the history).
CREATE TABLE pro_verifications (
    pro_id     UUID PRIMARY KEY REFERENCES pros(id) ON DELETE CASCADE,
    checklist  JSONB NOT NULL DEFAULT '{}'::jsonb,
    decided_by UUID NOT NULL REFERENCES staff(id),
    decided_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Employee profile shown in the console. The emergency contact is encrypted (crypto.rs, AAD-bound).
CREATE TABLE pro_profiles (
    pro_id                UUID PRIMARY KEY REFERENCES pros(id) ON DELETE CASCADE,
    photo_url             TEXT,
    bio                   TEXT NOT NULL DEFAULT '',
    employment_type       TEXT CHECK (employment_type IN ('freelance', 'employee', 'contract')),
    emergency_contact_enc TEXT,
    hired_on              DATE,
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
