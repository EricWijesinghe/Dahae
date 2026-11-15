//! Provider webhooks over HTTP. PortOne: the Standard Webhooks signature is the gate, and a
//! replayed event changes nothing. Toss: the body is ignored and each delivery kicks one payout pass.
//! Events name unknown payments, so no provider API is called; the replay test settles a real checkout.
//! Needs DATABASE_URL (sqlx::test).

mod common;

use api::{build_state, payout_sync, routes};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use common::{call, customer, seed_catalog};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use sqlx::PgPool;
use std::{net::SocketAddr, sync::atomic::Ordering};
use tower::ServiceExt;

/// Raw key behind the configured `whsec_` secret (base64 of the bytes below).
const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";
const OTHER_KEY: &[u8] = b"fedcba9876543210fedcba9876543210";
const PAYMENT_ID: &str = "pay_unknown_for_webhook_test";

fn secret_for(key: &[u8]) -> String {
    format!("whsec_{}", STANDARD.encode(key))
}

/// The router with a PortOne webhook secret configured (dev mode stays on for the other providers).
fn app(db: PgPool) -> Router {
    let mut cfg = common::test_config();
    cfg.portone.webhook_secret = Some(secret_for(KEY));
    let state = build_state(db, cfg).expect("http client");
    routes::router(state)
}

/// Standard Webhooks `webhook-signature` value for `body`, signed with `key`.
fn sign(key: &[u8], id: &str, ts: i64, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(format!("{id}.{ts}.").as_bytes());
    mac.update(body);
    format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()))
}

fn payment_event(payment_id: &str) -> Vec<u8> {
    json!({ "type": "Transaction.Paid", "data": { "paymentId": payment_id } })
        .to_string()
        .into_bytes()
}

/// POSTs `body` with the given webhook headers and returns the status.
async fn post(app: &Router, uri: &str, headers: &[(&str, String)], body: Vec<u8>) -> StatusCode {
    let mut req = Request::builder().method("POST").uri(uri).header(header::CONTENT_TYPE, "application/json");
    for (name, value) in headers {
        req = req.header(*name, value.as_str());
    }
    let mut request = req.body(Body::from(body)).expect("valid request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    app.clone().oneshot(request).await.expect("router is infallible").status()
}

/// The three headers of a delivery signed now with `key`.
fn signed_headers(key: &[u8], id: &str, body: &[u8]) -> Vec<(&'static str, String)> {
    let ts = Utc::now().timestamp();
    vec![
        ("webhook-id", id.to_string()),
        ("webhook-timestamp", ts.to_string()),
        ("webhook-signature", sign(key, id, ts, body)),
    ]
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_event_with_a_valid_signature_is_accepted(pool: PgPool) {
    let app = app(pool);
    let body = payment_event(PAYMENT_ID);
    let headers = signed_headers(KEY, "msg_valid_1", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body).await, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_event_signed_with_another_key_is_rejected(pool: PgPool) {
    let app = app(pool);
    let body = payment_event(PAYMENT_ID);
    let headers = signed_headers(OTHER_KEY, "msg_wrong_key", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_body_changed_after_signing_is_rejected(pool: PgPool) {
    let app = app(pool);
    let signed = payment_event(PAYMENT_ID);
    let headers = signed_headers(KEY, "msg_tampered", &signed);
    let tampered = payment_event("pay_someone_else");
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, tampered).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_event_without_signature_headers_is_rejected(pool: PgPool) {
    let app = app(pool);
    let body = payment_event(PAYMENT_ID);
    assert_eq!(post(&app, "/v1/webhooks/portone", &[], body).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_event_with_a_stale_timestamp_is_rejected(pool: PgPool) {
    let app = app(pool);
    let body = payment_event(PAYMENT_ID);
    let ts = Utc::now().timestamp() - 3_600;
    let headers = vec![
        ("webhook-id", "msg_stale".to_string()),
        ("webhook-timestamp", ts.to_string()),
        ("webhook-signature", sign(KEY, "msg_stale", ts, &body)),
    ];
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_replayed_event_settles_a_checkout_once(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-7100-0009").await;
    let visit = api::util::today_kst() + chrono::Duration::days(10);
    let intent_body = json!({ "kind": "booking", "method": "card",
        "booking": { "service_code": "studio", "address_id": c.address, "visit_date": visit, "start_hour": 9, "visit_notes": {} } });
    let (st, intent) = call(&app, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(intent_body)).await;
    assert_eq!(st, StatusCode::OK, "{intent}");
    let checkout = intent["id"].as_str().expect("id").to_string();
    let body = payment_event(&checkout);
    let headers = signed_headers(KEY, "msg_replay", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body.clone()).await, StatusCode::OK);
    let (status, booking): (String, Option<String>) = sqlx::query_as("SELECT status, result_booking_id::text FROM checkouts WHERE id = $1::uuid")
        .bind(&checkout)
        .fetch_one(&pool)
        .await
        .expect("checkout");
    assert_eq!(status, "paid", "the first delivery settles the checkout");
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body).await, StatusCode::OK);
    let (after_status, after_booking): (String, Option<String>) = sqlx::query_as("SELECT status, result_booking_id::text FROM checkouts WHERE id = $1::uuid")
        .bind(&checkout)
        .fetch_one(&pool)
        .await
        .expect("checkout");
    assert_eq!((after_status, after_booking), (status, booking), "the replay changes nothing");
    let (payments,): (i64,) = sqlx::query_as("SELECT count(*) FROM payments WHERE provider_tx_id = $1")
        .bind(&checkout)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(payments, 1, "one PortOne payment is recorded once");
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_webhook_ignores_the_body_and_kicks_one_pass(pool: PgPool) {
    let app = app(pool);
    let before = payout_sync::KICK_CALLS.load(Ordering::SeqCst);
    let status = post(&app, "/v1/webhooks/toss", &[], b"not even json".to_vec()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(payout_sync::KICK_CALLS.load(Ordering::SeqCst) - before, 1);
}
