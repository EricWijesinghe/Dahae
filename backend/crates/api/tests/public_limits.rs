//! Public write routes that anonymous clients can reach: a body cap on error reports and analytics batches,
//! and a per-IP budget on analytics ingest. Over the real router and a migrated database.
//! Needs DATABASE_URL (sqlx::test).

mod common;

use axum::http::StatusCode;
use common::call;
use serde_json::json;
use sqlx::PgPool;

/// The ingest budget per IP per minute (`INGEST_PER_MIN` in src/ratelimit.rs).
const INGEST_PER_MIN: usize = 120;

#[sqlx::test(migrations = "../../migrations")]
async fn oversized_error_reports_are_refused_before_parsing(db: PgPool) {
    let app = common::app(db);
    let huge = "x".repeat(100 * 1024);
    let (st, _) = call(
        &app,
        "POST",
        "/v1/client-errors",
        None,
        Some(json!({ "app": "customer", "version": "1.0.0", "message": huge })),
    )
    .await;
    assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_normal_error_report_is_not_cut_by_the_body_cap(db: PgPool) {
    let app = common::app(db);
    let stack = "at main (app.dart:1)\n".repeat(150);
    let (st, _) = call(
        &app,
        "POST",
        "/v1/client-errors",
        None,
        Some(json!({ "app": "customer", "version": "1.0.0", "message": "boom", "stack": stack })),
    )
    .await;
    assert_ne!(st, StatusCode::PAYLOAD_TOO_LARGE, "a 4 KB stack must fit");
}

#[sqlx::test(migrations = "../../migrations")]
async fn analytics_ingest_refuses_requests_past_its_per_ip_budget(db: PgPool) {
    let app = common::app(db);
    for i in 0..INGEST_PER_MIN {
        let (st, _) = call(&app, "POST", "/v1/events", None, Some(json!({}))).await;
        assert_ne!(st, StatusCode::TOO_MANY_REQUESTS, "request {i} is inside the budget");
    }
    let (st, b) = call(&app, "POST", "/v1/events", None, Some(json!({}))).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "{b}");
    assert_eq!(b["error"]["code"], "rate_limited");
}
