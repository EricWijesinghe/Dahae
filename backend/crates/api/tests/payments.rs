//! Payment method offer: a method is listed to customers only when an admin switched it on and its PortOne
//! channel key is configured, and the admin list cannot be saved empty. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, build_state, routes};
use axum::{Router, http::StatusCode};
use common::{app, call, seed_catalog, staff_token, test_config};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;

/// Router whose PortOne config has channel keys for exactly `channels`.
fn app_with_channels(db: PgPool, channels: &[&str]) -> Router {
    let mut cfg = test_config();
    cfg.portone.channel_keys = channels.iter().map(|c| (c.to_string(), format!("ch-{c}"))).collect::<HashMap<_, _>>();
    routes::router(build_state(db, cfg).expect("state"))
}

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

#[sqlx::test(migrations = "../../migrations")]
async fn wallet_without_channel_key_is_not_offered(pool: PgPool) {
    seed_catalog(&pool).await;
    let dev = app_with_channels(pool.clone(), &["card", "kakaopay"]);
    let codes = offered(&dev).await;
    assert!(codes.contains(&"card".to_string()), "{codes:?}");
    assert!(codes.contains(&"kakaopay".to_string()), "{codes:?}");
    assert!(!codes.contains(&"naverpay".to_string()), "naverpay has no channel key: {codes:?}");
    assert!(!codes.contains(&"applepay".to_string()), "applepay has no channel key: {codes:?}");
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
