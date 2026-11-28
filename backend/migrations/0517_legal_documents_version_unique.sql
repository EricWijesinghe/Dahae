-- One version is one text: a document in one language records each version once (the publish lock already keeps it
-- so). Should an older build have recorded a version twice, the index is left out with a warning rather than failing
-- every boot, since published rows are append-only and cannot be repaired here. Unread by older releases.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM legal_documents GROUP BY doc, lang, version HAVING count(*) > 1) THEN
        RAISE WARNING 'legal_documents records a version twice, so legal_documents_version_key is not created';
    ELSE
        CREATE UNIQUE INDEX IF NOT EXISTS legal_documents_version_key ON legal_documents (doc, lang, version);
    END IF;
END $$;
