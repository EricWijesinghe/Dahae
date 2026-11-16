// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! 현금영수증 against a PortOne stand-in that replies with a recorded issue response, and a database
//! fault on the local write. Needs DATABASE_URL (sqlx::test). The fixture is shaped from the PortOne
//! SDK typings; it is replaced by a sandbox recording once the sandbox check is done.

mod common;

use api::{build_state, config::PortOne, crypto, providers::Providers, routes::cash_receipts as cr};
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{StatusCode, Uri},
};
use common::{seed_catalog, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

const ISSUED_REPLY: &str = include_str!("fixtures/portone_cash_receipt_issued.json");

/// Every request the stand-in saw: (path, JSON body).
#[derive(Default)]
struct Gateway {
    seen: Mutex<Vec<(String, Value)>>,
    /// When set, cancels are refused with a 400 instead of succeeding.
    refuse_cancels: AtomicBool,
}

async fn record(State(g): State<Arc<Gateway>>, uri: Uri, body: Bytes) -> (StatusCode, Json<Value>) {
    let json = serde_json::from_slice(&body).unwrap_or(Value::Null);
    g.seen.lock().expect("gateway lock").push((uri.path().to_string(), json));
    if uri.path().ends_with("/cash-receipt/cancel") && g.refuse_cancels.load(Ordering::SeqCst) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "already cancelled" })));
    }
    let reply = if uri.path() == "/cash-receipts" {
        serde_json::from_str(ISSUED_REPLY).expect("fixture")
    } else {
        json!({})
    };
    (StatusCode::OK, Json(reply))
}

/// Starts the stand-in on a loopback port. Returns its base URL and the recorder.
async fn gateway() -> (String, Arc<Gateway>) {
    let g = Arc::new(Gateway::default());
    let app = Router::new().fallback(record).with_state(g.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    (format!("http://{addr}"), g)
}

/// App state with cash receipts switched on or off, pointed at the stand-in.
async fn state(db: &PgPool, base: &str, cash_receipts: bool) -> api::state::AppState {
    let mut cfg = test_config();
    cfg.portone = PortOne {
        api_secret: Some("test-portone-secret".into()),
        channel_keys: HashMap::from([("card".to_string(), "ch-card".to_string())]),
        cash_receipts,
        ..PortOne::default()
    };
    let mut s = build_state(db.clone(), cfg.clone()).expect("state");
    s.providers = Arc::new(Providers::new(cfg, db.clone()).expect("providers").with_portone_api(base));
    s
}

/// A paid card checkout with its booking and payment (provider transaction `tx`, status `status`).
async fn paid_checkout(db: &PgPool, tx: &str, status: &str) -> (Uuid, Uuid) {
    seed_catalog(db).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000001', 'cash') RETURNING id")
        .fetch_one(db)
        .await
        .expect("user");
    let booking: Uuid = sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, created_at)
         VALUES ('BKC-1', $1, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, 3, 110000, 110000, 0, now()) RETURNING id",
    )
    .bind(user)
    .fetch_one(db)
    .await
    .expect("booking");
    let checkout: Uuid = sqlx::query_scalar(
        "INSERT INTO checkouts (user_id, kind, booking_id, method, amount, status, expires_at)
         VALUES ($1, 'booking', $2, 'card', 110000, 'paid', now() + interval '1 day') RETURNING id",
    )
    .bind(user)
    .bind(booking)
    .fetch_one(db)
    .await
    .expect("checkout");
    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (booking_id, amount, kind, status, refunded_amount, provider, provider_tx_id, idempotency_key)
         VALUES ($1, 110000, 'full', $2, CASE WHEN $2 = 'refunded' THEN 110000 ELSE 0 END, 'portone', $3, $3) RETURNING id",
    )
    .bind(booking)
    .bind(status)
    .bind(tx)
    .fetch_one(db)
    .await
    .expect("payment");
    (checkout, payment)
}

/// A receipt request the customer made at checkout, not yet issued.
async fn pending_receipt(db: &PgPool, checkout: Uuid) -> Uuid {
    let enc = crypto::encrypt(&[7u8; 32], crypto::aad::RECEIPT_IDENTITY, "01012345678");
    sqlx::query_scalar(
        "INSERT INTO cash_receipts (checkout_id, purpose, identity_type, identity_enc, identity_last4, state)
         VALUES ($1, 'PERSONAL', 'PHONE', $2, '5678', 'pending') RETURNING id",
    )
    .bind(checkout)
    .bind(enc)
    .fetch_one(db)
    .await
    .expect("receipt")
}

/// A receipt already issued for `payment`.
async fn issued_receipt(db: &PgPool, checkout: Uuid, payment: Uuid) -> Uuid {
    let id = pending_receipt(db, checkout).await;
    sqlx::query("UPDATE cash_receipts SET state = 'issued', payment_id = $2, issue_number = '1234567890' WHERE id = $1")
        .bind(id)
        .bind(payment)
        .execute(db)
        .await
        .expect("issued");
    id
}

async fn state_of(db: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM cash_receipts WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
        .expect("state")
}

#[sqlx::test(migrations = "../../migrations")]
async fn settled_checkout_records_the_provider_issue_number(pool: PgPool) {
    let (base, gw) = gateway().await;
    let s = state(&pool, &base, true).await;
    let (checkout, _) = paid_checkout(&pool, "pay_1", "paid").await;
    let receipt = pending_receipt(&pool, checkout).await;

    cr::on_settled(&s, checkout, "pay_1").await;

    let (st, number): (String, Option<String>) = sqlx::query_as("SELECT state, issue_number FROM cash_receipts WHERE id = $1")
        .bind(receipt)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!((st.as_str(), number.as_deref()), ("issued", Some("1234567890")));
    let seen = gw.seen.lock().expect("gateway lock");
    assert_eq!(seen[0].0, "/cash-receipts");
    assert_eq!(seen[0].1["paymentId"], "pay_1");
    assert_eq!(seen[0].1["channelKey"], "ch-card");
    assert_eq!(seen[0].1["type"], "PERSONAL");
    assert_eq!(seen[0].1["amount"]["total"], 110_000);
    assert_eq!(seen[0].1["customer"]["identityNumber"], "01012345678");
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_whose_local_write_fails_is_not_sent_again(pool: PgPool) {
    let (base, gw) = gateway().await;
    let s = state(&pool, &base, true).await;
    let (checkout, payment) = paid_checkout(&pool, "pay_2", "refunded").await;
    let receipt = issued_receipt(&pool, checkout, payment).await;
    sqlx::query("CREATE FUNCTION fail_cancel_write() RETURNS trigger AS $$ BEGIN RAISE EXCEPTION 'simulated write failure'; END $$ LANGUAGE plpgsql")
        .execute(&pool)
        .await
        .expect("fn");
    sqlx::query(
        "CREATE TRIGGER fail_cancel_write BEFORE UPDATE ON cash_receipts FOR EACH ROW WHEN (NEW.state = 'cancelled') EXECUTE FUNCTION fail_cancel_write()",
    )
    .execute(&pool)
    .await
    .expect("trigger");

    assert!(
        cr::on_full_refund(&s, payment, "pay_2").await.is_err(),
        "a failed local write is returned, not dropped"
    );
    assert_eq!(state_of(&pool, receipt).await, "issued");

    sqlx::query("DROP TRIGGER fail_cancel_write ON cash_receipts")
        .execute(&pool)
        .await
        .expect("drop");
    assert_eq!(cr::retry_cancels(&s).await.expect("pass"), 0, "a provider success is not cancelled again");
    assert_eq!(state_of(&pool, receipt).await, "issued");
    let paths: Vec<String> = gw.seen.lock().expect("gateway lock").iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(paths, vec!["/payments/pay_2/cash-receipt/cancel"], "exactly one provider cancel");
    let status = cr::ops_status(&pool, true).await.expect("status");
    assert_eq!(status["needs_check"], 1, "the row is counted for a manual check");
}

#[sqlx::test(migrations = "../../migrations")]
async fn refused_cancel_is_retried_only_after_its_backoff(pool: PgPool) {
    let (base, gw) = gateway().await;
    let s = state(&pool, &base, true).await;
    let (checkout, payment) = paid_checkout(&pool, "pay_5", "refunded").await;
    let receipt = issued_receipt(&pool, checkout, payment).await;
    gw.refuse_cancels.store(true, Ordering::SeqCst);

    cr::on_full_refund(&s, payment, "pay_5").await.expect("refusal is recorded, not raised");
    assert_eq!(state_of(&pool, receipt).await, "issued");
    assert_eq!(cr::retry_cancels(&s).await.expect("pass"), 0, "the backoff has not passed yet");

    gw.refuse_cancels.store(false, Ordering::SeqCst);
    sqlx::query("UPDATE cash_receipts SET next_cancel_at = now() - interval '1 minute' WHERE id = $1")
        .bind(receipt)
        .execute(&pool)
        .await
        .expect("backdate");
    assert_eq!(cr::retry_cancels(&s).await.expect("pass"), 1);
    assert_eq!(state_of(&pool, receipt).await, "cancelled");
    assert_eq!(gw.seen.lock().expect("gateway lock").len(), 2, "one refused cancel, then one success");
}

#[sqlx::test(migrations = "../../migrations")]
async fn retry_does_nothing_while_cash_receipts_are_off(pool: PgPool) {
    let (base, gw) = gateway().await;
    let s = state(&pool, &base, false).await;
    let (checkout, payment) = paid_checkout(&pool, "pay_3", "refunded").await;
    let receipt = issued_receipt(&pool, checkout, payment).await;

    assert_eq!(cr::retry_cancels(&s).await.expect("pass"), 0);
    assert_eq!(state_of(&pool, receipt).await, "issued");
    assert!(gw.seen.lock().expect("gateway lock").is_empty(), "no provider call while the switch is off");
}

#[sqlx::test(migrations = "../../migrations")]
async fn ops_status_shows_the_switch_and_last_result(pool: PgPool) {
    let (checkout, payment) = paid_checkout(&pool, "pay_4", "paid").await;
    issued_receipt(&pool, checkout, payment).await;

    let on = cr::ops_status(&pool, true).await.expect("status");
    assert_eq!(on["enabled"], true);
    assert_eq!(on["last_result"]["state"], "issued");
    assert_eq!(on["needs_check"], 0);
    let off = cr::ops_status(&pool, false).await.expect("status");
    assert_eq!(off["enabled"], false);
    assert!(!on.to_string().contains("failure_reason"), "provider text never reaches the status page");
}
