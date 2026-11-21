// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Daily PortOne payment pull and stale checkout sweep against a recorded provider: each
//! disagreement with local checkouts is logged once, the pass runs at most once per KST day after
//! the settle hour, and the stale sweep alerts only on paid checkouts inside its window.
//! Needs DATABASE_URL (sqlx::test), like the other DB-backed tests.

mod common;

use api::{build_state, config::PortOne, providers::Providers, routes::checkout_recon, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::get,
};
use chrono::{DateTime, Duration, Utc};
use common::{app, customer, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

/// The KST day the pass reconciles in these tests (2026-11-15), and the run times around it.
const RUN_AT: &str = "2026-11-15T18:00:00Z"; // 03:00 KST on the 16th, past the settle hour
const TOO_EARLY: &str = "2026-11-15T17:00:00Z"; // 02:00 KST on the 16th
const NOON_KST_ON_DAY: &str = "2026-11-15T03:00:00Z";

/// A recorded `GET /payments` page: one matched payment, one paid at PortOne but unknown locally,
/// one whose amount differs from the local checkout, and one dropped (not received, so not a mismatch).
fn recorded_page() -> Value {
    json!({
        "items": [
            { "id": "pay-matched", "status": "PAID", "amount": { "total": 9000 } },
            { "id": "pay-ghost", "status": "PAID", "amount": { "total": 7000 } },
            { "id": "pay-amount", "status": "PAID", "amount": { "total": 8000 } },
            { "id": "pay-dropped", "status": "FAILED", "amount": { "total": 6000 } }
        ],
        "page": { "number": 0, "size": 100, "totalCount": 4 }
    })
}

async fn list(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    if q.get("pageNumber").map(String::as_str) == Some("0") {
        Json(recorded_page())
    } else {
        Json(json!({ "items": [], "page": { "number": 1, "size": 100, "totalCount": 4 } }))
    }
}

/// `GET /payments/{id}`: ids ending in `failed` are failed at PortOne, the rest are paid; ids ending in `test` were
/// paid on a PortOne test channel.
async fn one(Path(id): Path<String>) -> Json<Value> {
    let status = if id.ends_with("failed") { "FAILED" } else { "PAID" };
    let kind = if id.ends_with("test") { "TEST" } else { "LIVE" };
    Json(json!({ "status": status, "amount": { "total": 9000, "cancelled": 0 }, "currency": "KRW", "channel": { "type": kind } }))
}

async fn start_gateway() -> String {
    let router = Router::new().route("/payments", get(list)).route("/payments/{id}", get(one));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    tokio::spawn(async move { axum::serve(listener, router).await.expect("mock serves") });
    format!("http://{addr}")
}

fn state_for(pool: PgPool, base: &str) -> AppState {
    let mut cfg = test_config();
    // The dev fake settles nothing real: the provider lookups must go to the recorded gateway.
    cfg.dev_mode = false;
    cfg.portone = PortOne {
        api_secret: Some("test-secret".into()),
        store_id: Some("store-test".into()),
        ..Default::default()
    };
    let state = build_state(pool.clone(), cfg.clone()).expect("state");
    AppState {
        providers: Arc::new(Providers::new(cfg, pool).expect("providers").with_portone_api(base)),
        ..state
    }
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).expect("rfc3339").with_timezone(&Utc)
}

async fn insert_checkout(pool: &PgPool, user: Uuid, payment: &str, status: &str, amount: i64, created: DateTime<Utc>) {
    sqlx::query(
        "INSERT INTO checkouts (user_id, kind, method, amount, status, provider_payment_id, expires_at, created_at)
         VALUES ($1, 'booking', 'card', $2, $3, $4, now() + interval '1 day', $5)",
    )
    .bind(user)
    .bind(amount)
    .bind(status)
    .bind(payment)
    .bind(created)
    .execute(pool)
    .await
    .expect("checkout row");
}

async fn logged_codes(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT code FROM ops_failures WHERE integration = 'payment_mismatch' ORDER BY code")
        .fetch_all(pool)
        .await
        .expect("failure log")
}

/// Whether the stale sweep claimed this checkout (its `reconciled_at` is stamped).
async fn claimed(pool: &PgPool, payment: &str) -> bool {
    sqlx::query_scalar("SELECT reconciled_at IS NOT NULL FROM checkouts WHERE provider_payment_id = $1")
        .bind(payment)
        .fetch_one(pool)
        .await
        .expect("checkout claim flag")
}

async fn stuck_alert(pool: &PgPool) -> Option<String> {
    sqlx::query_scalar("SELECT last_error FROM ops_health WHERE integration = 'payment_stuck' AND last_fail_at IS NOT NULL")
        .fetch_optional(pool)
        .await
        .expect("ops health")
}

#[sqlx::test(migrations = "../../migrations")]
async fn daily_pull_logs_each_disagreement_once_and_runs_once_a_day(pool: PgPool) {
    let base = start_gateway().await;
    let state = state_for(pool.clone(), &base);
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7300-0001").await;
    let day = at(NOON_KST_ON_DAY);
    insert_checkout(&pool, c.user, "pay-matched", "paid", 9000, day).await;
    insert_checkout(&pool, c.user, "pay-amount", "paid", 7500, day).await;
    insert_checkout(&pool, c.user, "pay-lost", "paid", 4000, day).await;

    let found = checkout_recon::payment_diff_at(&state, at(RUN_AT)).await.expect("diff pass");
    assert_eq!(found, 3, "amount, missing locally, missing at PortOne; matched and dropped stay quiet");
    assert_eq!(logged_codes(&pool).await, ["amount_differs", "missing_at_provider", "missing_locally"]);

    let again = checkout_recon::payment_diff_at(&state, at(RUN_AT)).await.expect("second tick");
    assert_eq!(again, 0, "the day is already reconciled");
    assert_eq!(logged_codes(&pool).await.len(), 3, "no duplicate rows");
}

#[sqlx::test(migrations = "../../migrations")]
async fn daily_pull_waits_for_the_settle_hour(pool: PgPool) {
    let base = start_gateway().await;
    let state = state_for(pool.clone(), &base);
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7300-0004").await;
    insert_checkout(&pool, c.user, "pay-lost", "paid", 4000, at(NOON_KST_ON_DAY)).await;

    let early = checkout_recon::payment_diff_at(&state, at(TOO_EARLY)).await.expect("early tick");
    assert_eq!(early, 0, "02:00 KST is too early: in-flight payments have not settled");
    assert!(logged_codes(&pool).await.is_empty());

    let found = checkout_recon::payment_diff_at(&state, at(RUN_AT)).await.expect("settled tick");
    assert_eq!(found, 4, "the same day runs once the settle hour has passed: four PortOne rows, one local row");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_clean_day_logs_nothing(pool: PgPool) {
    let base = start_gateway().await;
    let state = state_for(pool.clone(), &base);
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7300-0002").await;
    let day = at(NOON_KST_ON_DAY);
    insert_checkout(&pool, c.user, "pay-matched", "paid", 9000, day).await;
    insert_checkout(&pool, c.user, "pay-ghost", "paid", 7000, day).await;
    insert_checkout(&pool, c.user, "pay-amount", "paid", 8000, day).await;

    let found = checkout_recon::payment_diff_at(&state, at(RUN_AT)).await.expect("diff pass");
    assert_eq!(found, 0);
    assert!(logged_codes(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_sweep_covers_only_unfinished_checkouts_inside_its_window(pool: PgPool) {
    let base = start_gateway().await;
    let state = state_for(pool.clone(), &base);
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7300-0003").await;
    let two_days = Utc::now() - Duration::days(2);
    insert_checkout(&pool, c.user, "stale_paid", "pending", 9000, two_days).await;
    insert_checkout(&pool, c.user, "stale_failed", "awaiting_deposit", 9000, two_days).await;
    insert_checkout(&pool, c.user, "ancient_paid", "pending", 9000, Utc::now() - Duration::days(40)).await;
    insert_checkout(&pool, c.user, "fresh_paid", "pending", 9000, Utc::now()).await;
    insert_checkout(&pool, c.user, "stale_settled", "paid", 9000, two_days).await;

    let checked = checkout_recon::reconcile_stale(&state).await.expect("stale pass");
    assert_eq!(checked, 2, "only the two-day-old unfinished checkouts inside the 30 day window");
    assert!(claimed(&pool, "stale_paid").await);
    assert!(claimed(&pool, "stale_failed").await, "checked, though not paid");
    assert!(!claimed(&pool, "ancient_paid").await, "older than the window is left alone");
    assert!(!claimed(&pool, "fresh_paid").await, "under 24 h is the open sweep's job");
    assert!(!claimed(&pool, "stale_settled").await, "already settled checkouts are not re-polled");
    assert_eq!(stuck_alert(&pool).await.as_deref(), Some("paid_unfulfilled_after_day"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_payment_on_a_portone_test_channel_alerts_the_owner(pool: PgPool) {
    let base = start_gateway().await;
    let state = state_for(pool.clone(), &base);
    api::providers::portone::payment(&state.providers, "pay-live").await.expect("live payment");
    assert_eq!(stuck_alert(&pool).await, None, "a live channel is quiet");
    let paid = api::providers::portone::payment(&state.providers, "pay-test").await.expect("test payment");
    assert_eq!(paid.status, "PAID", "the payment itself still reads as paid");
    assert_eq!(stuck_alert(&pool).await.as_deref(), Some("portone_test_channel"));
}
