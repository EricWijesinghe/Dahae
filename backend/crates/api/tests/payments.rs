//! Payment method offer: a method is listed to customers only when an admin switched it on and its PortOne
//! channel key is configured, a checkout for an unconfigured method is refused before any row exists, and the
//! admin list cannot be saved empty. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, build_state, routes};
use axum::{Router, http::StatusCode};
use common::{Customer, app, app_real_gateway_with_channels, call, customer, seed_catalog, staff_token, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;

/// Codes listed by `GET /v1/payments/methods`.
async fn offered(app: &Router) -> Vec<String> {
    let (st, body) = call(app, "GET", "/v1/payments/methods", None, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    body.as_array()
        .expect("list")
        .iter()
        .map(|m| m["code"].as_str().expect("code").to_string())
        .collect()
}

/// The admin list entry for `code`.
async fn admin_entry(app: &Router, token: &str, code: &str) -> Value {
    let (st, listed) = call(app, "GET", "/v1/admin/payment-methods", Some(token), None).await;
    assert_eq!(st, StatusCode::OK, "{listed}");
    listed
        .as_array()
        .expect("list")
        .iter()
        .find(|m| m["code"] == code)
        .cloned()
        .expect("method listed")
}

fn checkout_body(c: &Customer, method: &str) -> Value {
    let visit = api::util::today_kst() + chrono::Duration::days(10);
    json!({ "kind": "booking", "method": method,
            "booking": { "service_code": "studio", "address_id": c.address, "visit_date": visit, "start_hour": 9, "visit_notes": {} } })
}

#[sqlx::test(migrations = "../../migrations")]
async fn fake_provider_offers_every_enabled_method_without_keys(pool: PgPool) {
    seed_catalog(&pool).await;
    let (token, _) = staff_token(&pool, Role::StaffAdmin).await;
    let dev = app(pool.clone());
    assert_eq!(offered(&dev).await.len(), 10, "dev fake takes no keys, so a local demo can pay");
    let naver = admin_entry(&dev, &token, "naverpay").await;
    assert_eq!(naver["configured"], true, "{naver}");
    let c = customer(&dev, &pool, "010-7100-0010").await;
    let (st, body) = call(&dev, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(checkout_body(&c, "naverpay"))).await;
    assert_ne!(body["error"]["code"], "payment_method_disabled", "{st} {body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn wallet_without_channel_key_is_not_offered(pool: PgPool) {
    seed_catalog(&pool).await;
    let dev = app_real_gateway_with_channels(pool.clone(), &["card", "kakaopay"]);
    let codes = offered(&dev).await;
    assert!(codes.contains(&"card".to_string()), "{codes:?}");
    assert!(codes.contains(&"kakaopay".to_string()), "{codes:?}");
    assert!(!codes.contains(&"naverpay".to_string()), "naverpay has no channel key: {codes:?}");
    assert!(!codes.contains(&"applepay".to_string()), "applepay has no channel key: {codes:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn blank_channel_key_is_not_offered(pool: PgPool) {
    seed_catalog(&pool).await;
    let mut cfg = test_config();
    cfg.dev_mode = false;
    cfg.portone.api_secret = Some("test-secret".into());
    cfg.portone.store_id = Some("store-test".into());
    cfg.portone.channel_keys = HashMap::from([("card".to_string(), "ch-card".to_string()), ("naverpay".to_string(), "   ".to_string())]);
    let dev = routes::router(build_state(pool.clone(), cfg).expect("state"));
    let codes = offered(&dev).await;
    assert_eq!(codes, vec!["card".to_string()], "a blank naverpay key must not offer naverpay");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_list_flags_a_switched_on_method_without_a_key(pool: PgPool) {
    seed_catalog(&pool).await;
    let (token, _) = staff_token(&pool, Role::StaffAdmin).await;
    let dev = app_real_gateway_with_channels(pool.clone(), &["card"]);
    let naver = admin_entry(&dev, &token, "naverpay").await;
    assert_eq!(naver["enabled"], true, "{naver}");
    assert_eq!(naver["configured"], false, "switched on but no key: {naver}");
    let card = admin_entry(&dev, &token, "card").await;
    assert_eq!(card["configured"], true, "{card}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn checkout_for_unconfigured_method_is_refused_without_a_row(pool: PgPool) {
    seed_catalog(&pool).await;
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7100-0009").await;
    let real = app_real_gateway_with_channels(pool.clone(), &["card"]);
    let (st, body) = call(&real, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(checkout_body(&c, "naverpay"))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "payment_method_disabled", "{body}");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM checkouts").fetch_one(&pool).await.expect("count");
    assert_eq!(rows, 0, "a refused checkout must not leave a pending row");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_cannot_save_an_empty_method_list(pool: PgPool) {
    seed_catalog(&pool).await;
    let (token, _) = staff_token(&pool, Role::StaffAdmin).await;
    let dev = app(pool.clone());
    let (st, body) = call(&dev, "PUT", "/v1/admin/payment-methods", Some(token.as_str()), Some(json!({ "enabled": [] }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "payment_methods_empty", "{body}");

    let (st, listed) = call(&dev, "GET", "/v1/admin/payment-methods", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{listed}");
    assert!(
        listed.as_array().expect("list").iter().any(|m| m["enabled"] == true),
        "previous list kept: {listed}"
    );
}
