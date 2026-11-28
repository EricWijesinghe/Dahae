// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Backend HTTP API. `main.rs` calls [`serve`]; integration tests build the router from these modules.
//! New top-level modules are declared here (not in main.rs).

pub mod acquisition;
pub mod adapters;
pub mod address;
pub mod ai;
pub mod analytics;
pub mod app_links;
pub mod auth;
pub mod backup;
pub mod billing_keys;
pub mod client_errors;
pub mod config;
pub mod contract;
pub mod crypto;
pub mod db;
pub mod demo;
pub mod domains;
pub mod entropy;
pub mod error;
pub mod host_jobs;
pub mod host_status;
pub mod i18n;
pub mod idempotency;
pub mod image_meta;
pub mod integrations;
pub mod jobs;
pub mod keys_bundle;
pub mod lifecycle;
pub mod lockout;
pub mod marketing;
pub mod metrics;
pub mod noshow;
pub mod openapi;
pub mod ops;
pub mod ops_alerts;
pub mod owner;
pub mod pack;
pub mod payout;
pub mod payout_sync;
pub mod phone;
pub mod points;
pub mod privacy;
pub mod providers;
pub mod quality;
pub mod ratelimit;
pub mod realtime;
mod realtime_heartbeat;
pub mod realtime_ticket;
pub mod reconciler;
pub mod renewals;
pub mod retention;
pub mod routes;
pub mod safety;
pub mod schedule;
pub mod seed;
pub mod seo;
pub mod settings_registry;
pub mod sms_outbox;
pub mod state;
pub mod telemetry;
pub mod totp;
pub mod util;

use crate::{config::Config, providers::Providers, realtime::Hub, state::AppState};
use sqlx::PgPool;
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::task::JoinHandle;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Pack(#[from] pack::PackError),
}

/// Shared application state over a pool and config. Used by `serve` and the integration tests.
pub fn build_state(db: PgPool, config: Config) -> Result<AppState, StateError> {
    Ok(AppState {
        pack: Arc::new(pack::load(pack::ACTIVE)?),
        providers: Arc::new(Providers::new(config.clone(), db.clone())?),
        hub: Hub::new(db.clone()),
        lockout: Arc::new(lockout::Lockout::new(db.clone())),
        db,
        config: Arc::new(config),
        ai_debounce: Default::default(),
        address_search: Default::default(),
        review_reports: Default::default(),
        phone_e164_reads: Default::default(),
    })
}

/// Applies pending migrations. Rows written by a newer release are ignored, so the previous image
/// still starts after a rollback (migrations stay additive: expand, then contract a release later).
pub async fn migrate(db: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").set_ignore_missing(true).run(db).await
}

/// Reads config, connects, migrates, seeds and serves until SIGTERM or Ctrl+C, then drains.
pub async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    crate::ops::mark_started();
    // An invalid country pack stops the boot before anything touches the database.
    pack::load_all()?;
    let mut config = Config::from_env()?;
    // Migrations get their own single connection without the statement timeout (index builds can be slow).
    let migrate_db = db::connect(&config.database_url, &config.db.for_migrations()).await?;
    migrate(&migrate_db).await?;
    // The zone is fixed for the life of the process; the main pool below carries it on every connection.
    util::set_install_tz(util::load_install_tz(&migrate_db).await?);
    migrate_db.close().await;
    let db = db::connect(&config.database_url, &config.db).await?;
    seed::run(&db, &config).await?;
    let moved = safety::encrypt_legacy_codes(&db, &config.data_key).await?;
    if moved > 0 {
        tracing::warn!(moved, "entry codes moved into encrypted storage");
    }
    let sealed = billing_keys::seal_stored(&db, &config.data_key).await?;
    if sealed > 0 {
        tracing::warn!(sealed, "saved-card billing keys moved into encrypted storage");
    }
    routes::setup::ensure_setup_code(&db, config.setup_code.as_deref(), config.setup_code_at).await?;
    let active_pack = pack::load(pack::ACTIVE)?;
    routes::setup::install::first_boot(&db, &active_pack).await?;
    tokio::fs::create_dir_all(&config.upload_dir).await?;
    if config.dev_mode {
        tracing::warn!("DEV_MODE on: fake SMS, payments and push");
    }
    domains::startup(&db, &mut config).await;
    let bind = config.bind;
    let state = build_state(db, config)?;
    spawn_phone_backfill(state.db.clone(), active_pack.id, state.phone_e164_reads.clone());
    routes::setup::reload(&state).await?;
    let jobs = vec![
        spawn_settings_refresh(state.clone()),
        reconciler::spawn(state.clone()),
        ops_alerts::spawn(state.clone()),
        renewals::spawn(state.clone()),
        payout_sync::spawn(state.clone()),
        retention::spawn(state.clone()),
        noshow::spawn(state.clone()),
        points::spawn(state.clone()),
        marketing::spawn(state.clone()),
        quality::spawn(state.clone()),
        routes::waitlist::spawn(state.clone()),
        routes::partner_jobs::spawn_close_stale_quotes(state.clone()),
        routes::subscription_engine::spawn(state.clone()),
        realtime::spawn_outbox(state.clone()),
        state.hub.spawn_listener(),
    ];
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "listening");
    let hub = state.hub.clone();
    // ConnectInfo gives the rate limiter the peer address.
    let app = routes::router(state).into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            lifecycle::os_signal().await;
            hub.drain();
        })
        .await?;
    // Requests have finished; background jobs stop after their current pass.
    lifecycle::join_jobs(jobs).await;
    tracing::info!("stopped");
    Ok(())
}

/// Fills the E.164 phone copies in the background: a first upgrade touches every stored phone, which must not
/// hold up readiness. A failed run is logged and the next start resumes it. After a complete run, login reads
/// switch to the copies when `phone::reads_ready` holds; until then (and after any failure) they stay on the old
/// columns.
fn spawn_phone_backfill(db: PgPool, region: String, e164_reads: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if let Err(e) = phone::record_writer(&db, env!("CARGO_PKG_VERSION")).await {
            tracing::warn!(error = %e, "phone copy writer not recorded; logins stay on the old columns");
            return;
        }
        match phone::backfill(&db, &region, phone::BACKFILL_BATCH).await {
            Ok(0) => {}
            Ok(filled) => tracing::info!(filled, "phone numbers copied into E.164"),
            Err(e) => {
                tracing::warn!(error = %e, "E.164 phone backfill stopped; the next start resumes it");
                return;
            }
        }
        match phone::reads_ready(&db, &region).await {
            Ok(true) => {
                e164_reads.store(true, Ordering::Relaxed);
                tracing::info!("phone logins read the E.164 copies");
            }
            // reads_ready logged why.
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "phone read switch check failed; logins stay on the old columns"),
        }
    });
}

/// Other replicas pick up keys saved in the setup wizard within this period.
const SETTINGS_REFRESH: Duration = Duration::from_secs(60);

fn spawn_settings_refresh(state: AppState) -> JoinHandle<()> {
    let mut stop = state.hub.drain_signal();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SETTINGS_REFRESH);
        tick.tick().await;
        while lifecycle::next_tick(&mut tick, &mut stop).await {
            let ok = routes::setup::reload(&state)
                .await
                .inspect_err(|e| tracing::warn!(error = %e, "settings refresh failed"))
                .is_ok();
            metrics::job_pass("settings_refresh", ok);
        }
    })
}
