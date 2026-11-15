//! A partner job carries the owner's counts options (key and label) in catalogue order, including options added in the console.

mod common;

use axum::{Router, http::StatusCode};
use common::{Customer, book, call, customer, seed_catalog, verified_pro};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn book_counts(app: &Router, c: &Customer, code: &str, counts: Value) -> (Uuid, Value) {
    let visit = api::util::today_kst() + chrono::Duration::days(10);
    let (st, b) = call(
        app,
        "POST",
        "/v1/bookings",
        Some(c.token.as_str()),
        Some(json!({
            "service_code": code,
            "counts": counts,
            "address_id": c.address,
            "visit_date": visit,
            "start_hour": 9,
            "visit_notes": {},
            "payment_method_id": c.card
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "booking: {b}");
    (b["id"].as_str().expect("booking id").parse().expect("uuid"), b)
}

#[sqlx::test(migrations = "../../migrations")]
async fn job_lists_the_owner_catalogue_options_including_added_ones(pool: PgPool) {
    seed_catalog(&pool).await;
    sqlx::query(
        "UPDATE services SET pricing = jsonb_set(pricing, '{units}', pricing->'units' || '[{\"label_key\":\"opt_1\",\"label\":\"특수 에어컨\",\"minutes\":30,\"price\":10000}]'::jsonb) WHERE code = 'aircon'",
    )
    .execute(&pool)
    .await
    .expect("add an option");
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5000-0901").await;
    let (id, b) = book_counts(&app, &c, "aircon", json!([1, 0, 0, 0, 1])).await;
    assert_eq!(b["unit_counts"], json!([1, 0, 0, 0, 1]));
    let (pro, _) = verified_pro(&app, &pool, "010-5000-0902").await;
    let (st, acc) = call(&app, "POST", &format!("/v1/partner/jobs/{id}/accept"), Some(pro.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{acc}");
    let (st, job) = call(&app, "GET", &format!("/v1/partner/jobs/{id}"), Some(pro.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{job}");
    let units = &job["booking"]["count_units"];
    assert_eq!(units[0], json!({"label_key": "wall", "label": null}));
    assert_eq!(units[4], json!({"label_key": "opt_1", "label": "특수 에어컨"}));
    assert_eq!(units.as_array().map(Vec::len), Some(5));
}

#[sqlx::test(migrations = "../../migrations")]
async fn non_counts_job_has_no_count_units(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5000-0903").await;
    let (id, _) = book(&app, &c, 10, None).await;
    let (pro, _) = verified_pro(&app, &pool, "010-5000-0904").await;
    let (st, acc) = call(&app, "POST", &format!("/v1/partner/jobs/{id}/accept"), Some(pro.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{acc}");
    let (st, job) = call(&app, "GET", &format!("/v1/partner/jobs/{id}"), Some(pro.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{job}");
    assert!(job["booking"]["count_units"].is_null(), "{job}");
}
