// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Install facts at first boot of a pack-aware release. An install that already has real staff was running the
//! Korean product, so it gets `install.country` and its companions from the active pack (and `install.locked_at`
//! if it ever took a payment). A fresh install is left unset, so the setup wizard asks. Runs once per database:
//! a marker row decides, so a fresh install that creates its owner later is never switched to KR silently.

use super::REAL_STAFF_EXISTS;
use crate::{
    error::{ApiError, ApiResult},
    pack::Pack,
    settings_registry::{self, storage_key},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};

/// Written once, in the same transaction as the facts; its presence means first boot already ran.
pub const MARKER_KEY: &str = "install.first_boot";

/// The first time a non-demo payment was taken, if any. Refunded payments were taken too.
const FIRST_PAYMENT_AT: &str = "SELECT to_char(MIN(p.created_at) AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') FROM payments p \
     WHERE p.status IN ('paid', 'partially_refunded', 'refunded') \
     AND NOT EXISTS (SELECT 1 FROM demo_rows d WHERE d.tbl = 'payments' AND d.row_key = p.id::text)";

/// Records install facts on the first boot. Returns whether the facts were written (an existing install).
pub async fn first_boot(db: &PgPool, pack: &Pack) -> ApiResult<bool> {
    let mut tx = db.begin().await?;
    let first: Option<String> = sqlx::query_scalar("INSERT INTO settings (key, value) VALUES ($1, '{}'::jsonb) ON CONFLICT (key) DO NOTHING RETURNING key")
        .bind(MARKER_KEY)
        .fetch_optional(&mut *tx)
        .await?;
    if first.is_none() {
        return Ok(false);
    }
    let existing: bool = sqlx::query_scalar(REAL_STAFF_EXISTS).fetch_one(&mut *tx).await?;
    let locked_at: Option<String> = if existing {
        sqlx::query_scalar(FIRST_PAYMENT_AT).fetch_one(&mut *tx).await?
    } else {
        None
    };
    if existing {
        let mut facts = vec![
            ("install.country", json!(pack.id)),
            ("install.currency", json!(pack.currency.code)),
            ("install.tz", json!(pack.zones.first())),
            ("install.locales", json!(pack.locales)),
            ("install.pack_version_applied", json!(pack.version)),
        ];
        facts.extend(locked_at.as_ref().map(|at| ("install.locked_at", json!(at))));
        for (key, value) in &facts {
            write_fact(&mut tx, key, value).await?;
        }
        let data = json!({ "country": pack.id, "pack_version": pack.version, "locked_at": locked_at });
        sqlx::query("INSERT INTO audit_log (actor_role, action, target, data) VALUES ('system', 'install.auto_country', 'install.country', $1)")
            .bind(data)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE settings SET value = $2 WHERE key = $1")
        .bind(MARKER_KEY)
        .bind(json!({ "auto_country": existing.then_some(pack.id.as_str()) }))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if existing {
        tracing::info!(country = %pack.id, locked = locked_at.is_some(), "existing install: country facts recorded");
    }
    Ok(existing)
}

/// One registry value, checked against the registry first so a pack value outside its options stops the boot.
/// Never overwrites a stored value.
async fn write_fact(tx: &mut Transaction<'_, Postgres>, key: &str, value: &Value) -> ApiResult<()> {
    let spec = settings_registry::find(key).ok_or_else(|| ApiError::Internal(format!("unknown setting {key}")))?;
    let stored = settings_registry::validate(spec, value).map_err(|_| ApiError::Internal(format!("pack value for {key} fails the registry")))?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING")
        .bind(storage_key(key))
        .bind(stored)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
