// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Startup job that seals saved-card billing keys written before they were encrypted at rest.

use crate::crypto;
use sqlx::PgPool;
use uuid::Uuid;

/// Seals every live plaintext billing key and fills its digest. Rows already in the v1 format are skipped,
/// so a rerun changes nothing. Returns the number of rows sealed.
/// shortcut: runs once at startup only, so a plaintext row written by an old process after that is refused
/// until the next restart; upgrade to lazy sealing on the charge and delete paths if overlap ever happens.
pub async fn seal_stored(db: &PgPool, data_key: &[u8; 32]) -> Result<u64, sqlx::Error> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, billing_key FROM payment_methods WHERE deleted_at IS NULL AND billing_key <> '' AND left(billing_key, 3) <> 'v1:'")
            .fetch_all(db)
            .await?;
    let mut sealed = 0;
    for (id, plain) in rows {
        let (stored, digest) = crypto::seal_billing_key(data_key, &plain);
        sqlx::query("UPDATE payment_methods SET billing_key = $2, billing_key_digest = $3 WHERE id = $1")
            .bind(id)
            .bind(&stored)
            .bind(&digest)
            .execute(db)
            .await?;
        sealed += 1;
    }
    Ok(sealed)
}
