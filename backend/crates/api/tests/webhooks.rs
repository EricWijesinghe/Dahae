//! Provider webhooks over HTTP. PortOne: the Standard Webhooks signature is the gate, and a
//! replayed event changes nothing. Toss: the body is ignored and each delivery kicks one payout pass.
//! Events name unknown payments, so no provider API is called; the replay test settles a real checkout.
//! Needs DATABASE_URL (sqlx::test).

mod common;

use api::{build_state, config::PortOne, payout_sync, providers::Providers, routes};
use axum::{
    Json, Router,
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
use std::{
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
};
use tower::ServiceExt;
use uuid::Uuid;

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

const TOTAL: i64 = 110_000;
const CANCEL_PAYMENT: &str = "pay_console_cancel_test";

fn cancel_event(kind: &str, payment_id: &str, tx: &str) -> Vec<u8> {
    json!({ "type": kind, "data": { "paymentId": payment_id, "transactionId": tx } })
        .to_string()
        .into_bytes()
}

/// Production settings (DEV_MODE off) with signed webhooks; PortOne calls go to the stand-in at `base`.
fn prod_app(db: PgPool, base: &str) -> Router {
    let mut cfg = common::test_config();
    cfg.dev_mode = false;
    cfg.portone = PortOne {
        api_secret: Some(common::PORTONE_SECRET.into()),
        webhook_secret: Some(secret_for(KEY)),
        ..PortOne::default()
    };
    let mut state = build_state(db.clone(), cfg.clone()).expect("state");
    state.providers = Arc::new(Providers::new(cfg, db).expect("providers").with_portone_api(base));
    routes::router(state)
}

/// PortOne stand-in: every payment read answers a payment of `TOTAL` with `cancelled` taken back.
async fn portone_stand_in(cancelled: i64) -> String {
    let status = if cancelled == TOTAL { "CANCELLED" } else { "PARTIAL_CANCELLED" };
    let body = json!({
        "id": CANCEL_PAYMENT,
        "status": status,
        "amount": { "total": TOTAL, "cancelled": cancelled },
        "currency": "KRW",
        "method": { "type": "PaymentMethodCard" }
    });
    let app = Router::new().fallback(move || {
        let body = body.clone();
        async move { Json(body) }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

/// A paid PortOne booking payment whose provider transaction is `tx`, with `refunded` already given back.
/// Returns (booking, payment).
async fn paid_booking(db: &PgPool, tx: &str, refunded: i64) -> (Uuid, Uuid) {
    seed_catalog(db).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000009', 'webhook') RETURNING id")
        .fetch_one(db)
        .await
        .expect("user");
    let booking: Uuid = sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, created_at)
         VALUES ('BKW-1', $1, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, 1, $2, $2, 0, now()) RETURNING id",
    )
    .bind(user)
    .bind(TOTAL)
    .fetch_one(db)
    .await
    .expect("booking");
    let status = if refunded > 0 { "partially_refunded" } else { "paid" };
    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (booking_id, amount, kind, status, refunded_amount, provider, provider_tx_id, idempotency_key)
         VALUES ($1, $2, 'full', $3, $4, 'portone', $5, $5) RETURNING id",
    )
    .bind(booking)
    .bind(TOTAL)
    .bind(status)
    .bind(refunded)
    .bind(tx)
    .fetch_one(db)
    .await
    .expect("payment");
    (booking, payment)
}

/// The payment's recorded refunded amount and status.
async fn payment_state(db: &PgPool, payment: Uuid) -> (i64, String) {
    sqlx::query_as("SELECT refunded_amount, status FROM payments WHERE id = $1")
        .bind(payment)
        .fetch_one(db)
        .await
        .expect("payment")
}

/// Open disputes and payment-dispute cases on a booking.
async fn flags(db: &PgPool, booking: Uuid) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM disputes d JOIN payments p ON p.id = d.payment_id WHERE p.booking_id = $1),
                (SELECT count(*) FROM cases WHERE booking_id = $1 AND kind = 'payment_dispute')",
    )
    .bind(booking)
    .fetch_one(db)
    .await
    .expect("flags")
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_cancel_from_the_console_is_recorded_and_flagged_once(pool: PgPool) {
    let (booking, payment) = paid_booking(&pool, CANCEL_PAYMENT, 0).await;
    let app = prod_app(pool.clone(), &portone_stand_in(30_000).await);
    let body = cancel_event("Transaction.PartialCancelled", CANCEL_PAYMENT, "tx_console_1");
    let headers = signed_headers(KEY, "msg_cancel_1", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body.clone()).await, StatusCode::OK);
    assert_eq!(
        payment_state(&pool, payment).await,
        (30_000, "partially_refunded".into()),
        "refunded amount follows PortOne"
    );
    assert_eq!(flags(&pool, booking).await, (1, 1), "one dispute and one staff case");
    let replay = signed_headers(KEY, "msg_cancel_2", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &replay, body).await, StatusCode::OK);
    assert_eq!(
        payment_state(&pool, payment).await,
        (30_000, "partially_refunded".into()),
        "the replay changes nothing"
    );
    assert_eq!(flags(&pool, booking).await, (1, 1), "the replay opens nothing new");
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_full_cancel_from_the_console_refunds_the_payment_and_flags_it_once(pool: PgPool) {
    let (booking, payment) = paid_booking(&pool, CANCEL_PAYMENT, 0).await;
    let app = prod_app(pool.clone(), &portone_stand_in(TOTAL).await);
    let body = cancel_event("Transaction.Cancelled", CANCEL_PAYMENT, "tx_console_full");
    let headers = signed_headers(KEY, "msg_full_1", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body.clone()).await, StatusCode::OK);
    assert_eq!(payment_state(&pool, payment).await, (TOTAL, "refunded".into()), "full refund follows PortOne");
    assert_eq!(flags(&pool, booking).await, (1, 1), "one dispute and one staff case");
    let replay = signed_headers(KEY, "msg_full_2", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &replay, body).await, StatusCode::OK);
    assert_eq!(payment_state(&pool, payment).await, (TOTAL, "refunded".into()), "the replay changes nothing");
    assert_eq!(flags(&pool, booking).await, (1, 1), "the replay opens nothing new");
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_console_partial_cancel_opens_its_own_dispute(pool: PgPool) {
    let (booking, payment) = paid_booking(&pool, CANCEL_PAYMENT, 0).await;
    let first = prod_app(pool.clone(), &portone_stand_in(30_000).await);
    // PortOne may send the original transaction id on every cancel, so the second event repeats it.
    let body = cancel_event("Transaction.PartialCancelled", CANCEL_PAYMENT, "tx_original");
    let headers = signed_headers(KEY, "msg_second_1", &body);
    assert_eq!(post(&first, "/v1/webhooks/portone", &headers, body.clone()).await, StatusCode::OK);
    assert_eq!(flags(&pool, booking).await, (1, 1));
    let second = prod_app(pool.clone(), &portone_stand_in(40_000).await);
    let headers = signed_headers(KEY, "msg_second_2", &body);
    assert_eq!(post(&second, "/v1/webhooks/portone", &headers, body).await, StatusCode::OK);
    assert_eq!(payment_state(&pool, payment).await, (40_000, "partially_refunded".into()));
    assert_eq!(flags(&pool, booking).await, (2, 2), "the second console refund opens its own dispute and case");
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_we_made_is_recorded_without_a_flag(pool: PgPool) {
    let (booking, payment) = paid_booking(&pool, CANCEL_PAYMENT, 30_000).await;
    let app = prod_app(pool.clone(), &portone_stand_in(30_000).await);
    let body = cancel_event("Transaction.PartialCancelled", CANCEL_PAYMENT, "tx_ours_1");
    let headers = signed_headers(KEY, "msg_ours_1", &body);
    assert_eq!(post(&app, "/v1/webhooks/portone", &headers, body).await, StatusCode::OK);
    assert_eq!(payment_state(&pool, payment).await, (30_000, "partially_refunded".into()));
    assert_eq!(flags(&pool, booking).await, (0, 0), "a refund the app already recorded is not a dispute");
}
