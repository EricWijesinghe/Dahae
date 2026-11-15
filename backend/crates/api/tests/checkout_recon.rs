//! Daily PortOne payment pull against a recorded provider page: each disagreement with local
//! checkouts is logged once, and the pass runs at most once per KST day.
//! Needs DATABASE_URL (sqlx::test), like the other DB-backed tests.

mod common;

use api::{build_state, config::PortOne, providers::Providers, routes::checkout_recon, state::AppState};
use axum::{Json, Router, extract::Query, routing::get};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use common::{app, customer, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

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

async fn start_gateway() -> String {
    let router = Router::new().route("/payments", get(list));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    tokio::spawn(async move { axum::serve(listener, router).await.expect("mock serves") });
    format!("http://{addr}")
}

fn state_for(pool: PgPool, base: &str) -> AppState {
    let mut cfg = test_config();
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

/// Noon KST of the previous KST day, the day the daily pass reconciles.
fn yesterday_noon_utc() -> DateTime<Utc> {
    let day: NaiveDate = api::util::today_kst() - Duration::days(1);
    day.and_hms_opt(3, 0, 0).expect("time").and_utc()
}

async fn local_checkout(pool: &PgPool, user: Uuid, payment: &str, status: &str, amount: i64) {
    sqlx::query(
        "INSERT INTO checkouts (user_id, kind, method, amount, status, provider_payment_id, expires_at, created_at)
         VALUES ($1, 'booking', 'card', $2, $3, $4, now() + interval '1 day', $5)",
    )
    .bind(user)
    .bind(amount)
    .bind(status)
    .bind(payment)
    .bind(yesterday_noon_utc())
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

#[sqlx::test(migrations = "../../migrations")]
async fn daily_pull_logs_each_disagreement_once_and_runs_once_a_day(pool: PgPool) {
    let base = start_gateway().await;
    let state = state_for(pool.clone(), &base);
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7300-0001").await;
    local_checkout(&pool, c.user, "pay-matched", "paid", 9000).await;
    local_checkout(&pool, c.user, "pay-amount", "paid", 7500).await;
    local_checkout(&pool, c.user, "pay-lost", "paid", 4000).await;

    let found = checkout_recon::payment_diff_pass(&state).await.expect("diff pass");
    assert_eq!(found, 3, "amount, missing locally, missing at PortOne; matched and dropped stay quiet");
    assert_eq!(logged_codes(&pool).await, ["amount_differs", "missing_at_provider", "missing_locally"]);

    let again = checkout_recon::payment_diff_pass(&state).await.expect("second tick");
    assert_eq!(again, 0, "the day is already reconciled");
    assert_eq!(logged_codes(&pool).await.len(), 3, "no duplicate rows");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_clean_day_logs_nothing(pool: PgPool) {
    let base = start_gateway().await;
    let state = state_for(pool.clone(), &base);
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7300-0002").await;
    local_checkout(&pool, c.user, "pay-matched", "paid", 9000).await;
    local_checkout(&pool, c.user, "pay-ghost", "paid", 7000).await;
    local_checkout(&pool, c.user, "pay-amount", "paid", 8000).await;

    let found = checkout_recon::payment_diff_pass(&state).await.expect("diff pass");
    assert_eq!(found, 0);
    assert!(logged_codes(&pool).await.is_empty());
}
