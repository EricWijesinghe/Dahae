// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Process lifecycle: liveness and readiness probes, the shutdown signal, and stopping background jobs.
//!
//! Shutdown order on SIGTERM (or Ctrl+C): the hub starts draining, so WebSockets close with 1012
//! (service restart) and clients reconnect elsewhere; the listener stops taking new connections and
//! in-flight requests finish (each is bounded by the request timeout); then the background jobs stop
//! after their current pass, joined under a deadline.

use crate::{
    providers::{TOSS, TOSS_BILLING},
    routes::APP_VERSION,
    state::AppState,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures_util::future::join_all;
use serde_json::{Value, json};
use sqlx::{PgPool, migrate::Migrator};
use std::time::Duration;
use tokio::{sync::watch, task::JoinHandle, time::Interval};

/// The migrations this binary ships; readiness compares them with `_sqlx_migrations`.
static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const JOB_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Resolves once the hub started draining. Jobs select on it beside their timers.
pub async fn drained(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|d| *d).await;
}

/// Waits for the next tick. False once draining started, so the job returns instead of starting a pass.
pub async fn next_tick(tick: &mut Interval, stop: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tick.tick() => true,
        () = drained(stop) => false,
    }
}

/// Ctrl+C, or SIGTERM on Unix (what Docker and systemd send when stopping a container or unit).
pub async fn os_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    () = wait_ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "SIGTERM handler unavailable; only Ctrl+C stops the server");
                wait_ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    wait_ctrl_c().await;
    tracing::info!("shutdown signal received; draining");
}

async fn wait_ctrl_c() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::warn!(error = %e, "Ctrl+C handler failed; treating it as a shutdown request");
    }
}

/// Waits for the jobs to finish their current pass, up to a deadline. Panicked jobs are logged.
pub async fn join_jobs(jobs: Vec<JoinHandle<()>>) {
    match tokio::time::timeout(JOB_STOP_TIMEOUT, join_all(jobs)).await {
        Ok(results) => {
            for r in results.into_iter().filter_map(Result::err) {
                tracing::error!(error = %r, "background job panicked");
            }
        }
        Err(_) => tracing::warn!("background jobs still running at the stop deadline; exiting"),
    }
}

/// Liveness: the process answers. Never touches the database, so a DB outage does not restart the container.
pub async fn live() -> Json<Value> {
    Json(json!({ "ok": true, "version": APP_VERSION }))
}

/// Every adapter code a live row may name: `payments.provider`, `payment_methods.gateway`, `refund_queue.provider`
/// (the payment gateways) and `payouts.provider`. Codes are never removed (D12): this is every code any release ever
/// wrote there, including `pg`, the pre-release fake gateway's `payments.provider` (still in long-lived dev databases).
/// shortcut: static list, replaced by the adapter registry when it lands (R2).
pub const KNOWN_ADAPTERS: [&str; 6] = ["portone", TOSS, TOSS_BILLING, "toss", "manual", "pg"];

/// Readiness: the database answers, every shipped migration is applied, and this binary knows every adapter a
/// live row still needs (`adapters_ok`: refundable payments, saved cards, queued refunds, open payouts). After an
/// update to a binary that lacks one, the failed probe makes the installer roll back. Failing it takes the node
/// out of rotation; it does not restart it. `pack_ok` is a warning only: false after a rollback to a binary
/// whose country pack is older than the one the install applied. The install keeps running on this binary's pack.
pub async fn ready(State(s): State<AppState>) -> Response {
    let db_ok = tokio::time::timeout(PROBE_TIMEOUT, sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&s.db))
        .await
        .is_ok_and(|r| r.is_ok());
    let migrated = db_ok && migrations_applied(&s.db).await;
    let adapters_ok = migrated && adapters_known(&s.db).await;
    let ready = db_ok && migrated && adapters_ok;
    let pack_ok = !db_ok || applied_pack_ok(&s).await;
    let status = if ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let body = json!({ "ready": ready, "db": db_ok, "migrations": migrated, "adapters_ok": adapters_ok, "pack_ok": pack_ok });
    (status, Json(body)).into_response()
}

/// False when a live row names an adapter code outside `KNOWN_ADAPTERS`, or when the check cannot run in time.
async fn adapters_known(db: &PgPool) -> bool {
    let unknown = sqlx::query_scalar::<_, String>(
        "SELECT code FROM (
            SELECT provider AS code FROM payments WHERE status IN ('paid', 'partially_refunded') AND refunded_amount < amount
            UNION SELECT gateway FROM payment_methods WHERE deleted_at IS NULL AND billing_key <> ''
            UNION SELECT provider FROM refund_queue WHERE done_at IS NULL
            UNION SELECT provider FROM payouts WHERE state <> 'paid'
         ) live WHERE code <> ALL($1)",
    )
    .bind(&KNOWN_ADAPTERS[..])
    .fetch_all(db);
    match tokio::time::timeout(PROBE_TIMEOUT, unknown).await {
        Ok(Ok(codes)) if codes.is_empty() => true,
        Ok(Ok(codes)) => {
            tracing::error!(?codes, "live rows name adapters this binary does not know");
            false
        }
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "adapter check failed");
            false
        }
        Err(_) => {
            tracing::warn!("adapter check timed out");
            false
        }
    }
}

/// Whether the pack version this install applied is no newer than this binary's pack. Unset counts as fine.
pub fn pack_ok(applied: &str, binary: &str) -> bool {
    applied.is_empty() || crate::pack::version_key(applied) <= crate::pack::version_key(binary)
}

async fn applied_pack_ok(s: &AppState) -> bool {
    match crate::settings_registry::get_text(&s.db, &s.config, "install.pack_version_applied").await {
        Ok(applied) if pack_ok(&applied, &s.pack.version) => true,
        Ok(applied) => {
            tracing::warn!(applied, binary = %s.pack.version, "install applied a newer country pack than this binary carries");
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "pack version check failed");
            true
        }
    }
}

/// True when the database has applied every migration this binary knows about.
pub async fn migrations_applied(db: &PgPool) -> bool {
    let Some(latest) = MIGRATOR.migrations.iter().map(|m| m.version).max() else {
        return true;
    };
    // A missing bookkeeping table (never migrated) is a failed probe, not a crash.
    let applied: Result<Option<i64>, sqlx::Error> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success")
        .fetch_one(db)
        .await;
    matches!(applied, Ok(Some(v)) if v >= latest)
}

#[cfg(test)]
mod tests {
    use super::MIGRATOR;

    #[test]
    fn pack_ok_warns_only_when_the_applied_pack_is_newer() {
        assert!(super::pack_ok("", "2026.11.0"));
        assert!(super::pack_ok("2026.11.0", "2026.11.0"));
        assert!(super::pack_ok("2026.10.3", "2026.11.0"));
        assert!(!super::pack_ok("2026.11.1", "2026.11.0"));
        assert!(!super::pack_ok("2027.1.0", "2026.12.9"));
    }

    #[test]
    fn migrator_embeds_the_shipped_migrations() {
        assert!(MIGRATOR.migrations.len() >= 15, "migrations directory is embedded");
    }
}
