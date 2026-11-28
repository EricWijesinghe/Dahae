-- Published legal texts: one immutable row per publish of a document in one language (the console's Legal inbox).
-- `version` is the document version consents name; a row with requires_reconsent moves the version every consent
-- kind behind that document needs (privacy.rs). The draft is not stored: it is the pack template rendered on demand.
-- Unread by older releases, so a rollback is safe.
CREATE TABLE legal_documents (
    id                 BIGSERIAL PRIMARY KEY,
    doc                TEXT NOT NULL CHECK (doc ~ '^[a-z_]{1,40}$'),
    lang               TEXT NOT NULL CHECK (lang ~ '^[a-z]{2}(-[A-Z]{2})?$'),
    version            TEXT NOT NULL CHECK (version ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}(\.[0-9]+)?$'),
    template_version   TEXT,
    requires_reconsent BOOLEAN NOT NULL,
    body_md            TEXT NOT NULL,
    body_html          TEXT NOT NULL,
    body_sha256        TEXT NOT NULL CHECK (body_sha256 ~ '^[0-9a-f]{64}$'),
    slots_json         JSONB NOT NULL DEFAULT '{}'::jsonb,
    custom             BOOLEAN NOT NULL DEFAULT false,
    effective_at       DATE NOT NULL,
    published_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_by       UUID
);
CREATE INDEX legal_documents_doc_idx ON legal_documents (doc, lang, published_at DESC, id DESC);

CREATE TRIGGER legal_documents_no_change BEFORE UPDATE OR DELETE ON legal_documents
    FOR EACH ROW EXECUTE FUNCTION append_only_guard();
CREATE TRIGGER legal_documents_no_truncate BEFORE TRUNCATE ON legal_documents
    FOR EACH STATEMENT EXECUTE FUNCTION append_only_guard();
