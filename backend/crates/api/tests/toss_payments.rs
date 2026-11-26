// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Toss Payments next to PortOne, DEV_MODE off, against a fake Toss API (tests/common/toss.rs) and a small PortOne
//! mock: the checkout intent, the public toss-confirm capture, virtual accounts and their deposit callback, the
//! reconciler, switching gateways with payments in flight, saved cards on the billing MID, refunds, webhooks, the
//! console card, cash receipts and the daily diff. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, providers::ProviderError, routes::checkout_recon};
use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{Request, StatusCode},
    routing::{get, post},
};
use common::{
    Customer, TossEnv, call, call_owner, customer, enrol_owner, prod_app_toss, seed_catalog, set_toss, staff_token,
    toss::{self, BILLING_SK, Shared, WINDOW_CK, WINDOW_SK},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;
use uuid::Uuid;

/// The PortOne side: payment id -> (total, cancelled), every cancel and every saved-card charge.
#[derive(Default)]
struct PortOneMock {
    payments: HashMap<String, (i64, i64)>,
    cancels: Vec<String>,
    charges: Vec<String>,
    /// Every cancel and charge request: call, payment id and body, for the KR golden.
    bodies: Vec<Value>,
    /// The next charge answers 500: `Some(true)` after charging (answer lost), `Some(false)` without charging.
    fail_next_charge: Option<bool>,
}

type PortOneShared = Arc<Mutex<PortOneMock>>;

async fn po_payment(State(g): State<PortOneShared>, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    let g = g.lock().expect("portone");
    match g.payments.get(&id) {
        Some(&(total, cancelled)) => (
            StatusCode::OK,
            Json(json!({ "status": if cancelled == 0 { "PAID" } else { "CANCELLED" },
                         "amount": { "total": total, "cancelled": cancelled }, "currency": "KRW", "method": {} })),
        ),
        None => (StatusCode::NOT_FOUND, Json(json!({ "type": "PaymentNotFoundError" }))),
    }
}

async fn po_cancel(State(g): State<PortOneShared>, Path(id): Path<String>, Json(b): Json<Value>) -> Json<Value> {
    let mut g = g.lock().expect("portone");
    g.cancels.push(id.clone());
    g.bodies.push(json!({ "call": "cancel", "id": id, "body": b }));
    if let Some(p) = g.payments.get_mut(&id) {
        p.1 += b["amount"].as_i64().unwrap_or(0);
    }
    Json(json!({ "cancellation": { "status": "SUCCEEDED" } }))
}

async fn po_charge(State(g): State<PortOneShared>, Path(id): Path<String>, Json(b): Json<Value>) -> (StatusCode, Json<Value>) {
    let mut g = g.lock().expect("portone");
    g.charges.push(id.clone());
    g.bodies.push(json!({ "call": "charge", "id": id, "body": b }));
    let fail = g.fail_next_charge.take();
    if fail != Some(false) {
        let total = b["amount"]["total"].as_i64().unwrap_or(0);
        g.payments.insert(id, (total, 0));
    }
    if fail.is_some() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({})));
    }
    (StatusCode::OK, Json(json!({ "payment": { "paidAt": "2026-11-18T00:00:00Z" } })))
}

async fn po_list() -> Json<Value> {
    Json(json!({ "items": [], "page": { "totalCount": 0 } }))
}

async fn start_portone() -> (String, PortOneShared) {
    let g = PortOneShared::default();
    let app = Router::new()
        .route("/payments", get(po_list))
        .route("/payments/{id}", get(po_payment))
        .route("/payments/{id}/cancel", post(po_cancel))
        .route("/payments/{id}/billing-key", post(po_charge))
        .with_state(g.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind portone mock");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("mock serves") });
    (format!("http://{addr}"), g)
}

/// Every request body both gateways saw, with per-run ids redacted (PLAN section 8.1 step 1 golden).
fn gateway_bodies(w: &World) -> Value {
    let toss: Vec<Value> = w
        .toss
        .lock()
        .expect("fake")
        .calls
        .iter()
        .map(|c| json!({ "kind": c.kind, "path": c.path, "secret": c.secret, "body": c.body }))
        .collect();
    let portone = w.portone.lock().expect("portone").bodies.clone();
    common::redact_uuids(&json!({ "toss": toss, "portone": portone }))
}

struct World {
    env: TossEnv,
    toss: Shared,
    portone: PortOneShared,
    c: Customer,
}

async fn world(pool: &PgPool, phone: &str, active: bool) -> World {
    seed_catalog(pool).await;
    let (toss_base, toss) = toss::start().await;
    let (portone_base, portone) = start_portone().await;
    let c = customer(&common::app(pool.clone()), pool, phone).await;
    let env = prod_app_toss(pool.clone(), &toss_base, &portone_base, active).await;
    World { env, toss, portone, c }
}

/// A booking checkout for a studio clean five days out. Returns (checkout id, amount to charge, intent).
async fn checkout(w: &World, method: &str) -> (Uuid, i64, Value) {
    let visit = api::util::today_kst() + chrono::Duration::days(5);
    let body = json!({ "kind": "booking", "method": method,
        "booking": { "service_code": "studio", "address_id": w.c.address, "visit_date": visit, "start_hour": 9, "visit_notes": {} } });
    let (st, intent) = call(&w.env.app, "POST", "/v1/checkouts", Some(w.c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{intent}");
    let id = intent["id"].as_str().expect("id").parse().expect("uuid");
    (id, intent["amount"].as_i64().expect("amount"), intent)
}

async fn toss_confirm(w: &World, id: Uuid, key: &str, amount: i64) -> (StatusCode, Value) {
    let body = json!({ "payment_key": key, "amount": amount });
    call(&w.env.app, "POST", &format!("/v1/checkouts/{id}/toss-confirm"), None, Some(body)).await
}

async fn checkout_state(pool: &PgPool, id: Uuid) -> (String, String, Option<Uuid>) {
    sqlx::query_as("SELECT status, gateway, result_booking_id FROM checkouts WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("checkout")
}

async fn payment_provider(pool: &PgPool, booking: Uuid) -> (String, String, i64) {
    sqlx::query_as("SELECT provider, provider_tx_id, refunded_amount FROM payments WHERE booking_id = $1")
        .bind(booking)
        .fetch_one(pool)
        .await
        .expect("payment row")
}

/// Posts a Toss webhook body, optionally with a signature header.
async fn webhook(app: &Router, body: &Value, signature: Option<&str>) -> StatusCode {
    let mut req = Request::builder()
        .method("POST")
        .uri("/v1/webhooks/tosspayments")
        .header("content-type", "application/json");
    if let Some(sig) = signature {
        req = req
            .header("tosspayments-webhook-signature", sig)
            .header("tosspayments-webhook-transmission-time", "1800000000");
    }
    let mut req = req.body(Body::from(body.to_string())).expect("request");
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    app.clone().oneshot(req).await.expect("router").status()
}

/// Cancels a booking as the customer, accepting whatever fee the preview shows.
async fn cancel_booking(w: &World, booking: Uuid) {
    let tok = Some(w.c.token.as_str());
    let (_, preview) = call(&w.env.app, "GET", &format!("/v1/bookings/{booking}/cancel-preview"), tok, None).await;
    let body = json!({ "accepted_fee": preview["fee"].as_i64().unwrap_or(0) });
    let (st, out) = call(&w.env.app, "POST", &format!("/v1/bookings/{booking}/cancel"), tok, Some(body)).await;
    assert_eq!(st, StatusCode::OK, "cancel: {out}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_intent_names_the_gateway_and_its_client_key(pool: PgPool) {
    let w = world(&pool, "010-7600-0001", true).await;
    let (id, _, intent) = checkout(&w, "kakaopay").await;
    assert_eq!(intent["gateway"], "tosspayments");
    assert_eq!(intent["store_id"], Value::Null);
    assert_eq!(intent["channel_key"], WINDOW_CK);
    assert_eq!(intent["cash_receipts"], false, "Toss issues receipts in its own window");
    assert_eq!(checkout_state(&pool, id).await.1, "tosspayments");

    // A method with a PortOne channel key goes to PortOne, Toss on or off.
    let (id, _, intent) = checkout(&w, "transfer").await;
    assert_eq!(
        (intent["gateway"].as_str(), intent["channel_key"].as_str()),
        (Some("portone"), Some("channel-key-transfer"))
    );
    assert_eq!(intent["cash_receipts"], true);
    assert_eq!(checkout_state(&pool, id).await.1, "portone");
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_confirm_refuses_a_checkout_in_another_currency_before_toss(pool: PgPool) {
    let w = world(&pool, "010-7600-0031", true).await;
    let (id, amount, _) = checkout(&w, "card").await;
    sqlx::query("UPDATE checkouts SET currency = 'USD' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("currency");
    let (st, out) = toss_confirm(&w, id, "pk_currency", amount).await;
    assert_eq!((st, out["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("amount_mismatch")), "{out}");
    assert!(w.toss.lock().expect("fake").calls.is_empty(), "the payment is never captured");
    assert_eq!(checkout_state(&pool, id).await.0, "pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_confirm_captures_with_the_stored_amount_once(pool: PgPool) {
    let w = world(&pool, "010-7600-0002", true).await;
    let (id, amount, _) = checkout(&w, "card").await;

    let (st, wrong) = toss_confirm(&w, id, "pk_1", amount + 1000).await;
    assert_eq!(
        (st, wrong["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("amount_mismatch")),
        "{wrong}"
    );
    assert!(w.toss.lock().expect("fake").calls.is_empty(), "a tampered amount never reaches Toss");

    let (st, done) = toss_confirm(&w, id, "pk_1", amount).await;
    assert_eq!((st, done.clone()), (StatusCode::OK, json!({ "status": "paid" })), "only the status goes back");
    let confirms = w.toss.lock().expect("fake").calls_of("confirm");
    assert_eq!(confirms.len(), 1);
    assert_eq!(confirms[0].secret, WINDOW_SK, "the window MID confirms");
    assert_eq!(confirms[0].body, json!({ "paymentKey": "pk_1", "orderId": id.to_string(), "amount": amount }));
    let (status, _, booking) = checkout_state(&pool, id).await;
    assert_eq!(status, "paid");
    let booking = booking.expect("booking created");
    assert_eq!(payment_provider(&pool, booking).await.0, "tosspayments");

    let (st, again) = toss_confirm(&w, id, "pk_1", amount).await;
    assert_eq!((st, again["status"].as_str()), (StatusCode::OK, Some("paid")), "a reload re-posts harmlessly");
    let (st, app) = call(
        &w.env.app,
        "POST",
        &format!("/v1/checkouts/{id}/confirm"),
        Some(w.c.token.as_str()),
        Some(json!({ "payment_id": id.to_string() })),
    )
    .await;
    assert_eq!((st, app["booking_id"].as_str()), (StatusCode::OK, Some(booking.to_string().as_str())), "{app}");
    assert_eq!(w.toss.lock().expect("fake").calls_of("confirm").len(), 1, "never captured twice");
    common::golden("toss_confirm_bodies", &gateway_bodies(&w));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unclear_confirm_is_looked_up_before_it_fails(pool: PgPool) {
    let w = world(&pool, "010-7600-0003", true).await;
    // Already processed (a double submit): the lookup shows it paid.
    let (id, amount, _) = checkout(&w, "card").await;
    w.toss.lock().expect("fake").paid(&id.to_string(), amount);
    let (st, out) = toss_confirm(&w, id, "pk_dup", amount).await;
    assert_eq!((st, out["status"].as_str()), (StatusCode::OK, Some("paid")), "{out}");

    // A 500 whose payment went through anyway.
    let (id, amount, _) = checkout(&w, "card").await;
    {
        let mut f = w.toss.lock().expect("fake");
        f.push("confirm", 500, json!({ "code": "FAILED_INTERNAL_SYSTEM_PROCESSING" }));
        f.paid(&id.to_string(), amount);
    }
    let (st, out) = toss_confirm(&w, id, "pk_500", amount).await;
    assert_eq!((st, out["status"].as_str()), (StatusCode::OK, Some("paid")), "{out}");

    // "Already done" answers to a re-POST (the first confirm captured, its fulfil died): looked up, never a decline.
    for (status, code) in [
        (400, "ALREADY_COMPLETED_PAYMENT"),
        (400, "DUPLICATED_REQUEST"),
        (403, "FORBIDDEN_CONSECUTIVE_REQUEST"),
    ] {
        let (id, amount, _) = checkout(&w, "card").await;
        {
            let mut f = w.toss.lock().expect("fake");
            f.push("confirm", status, json!({ "code": code }));
            f.paid(&id.to_string(), amount);
        }
        let (st, out) = toss_confirm(&w, id, "pk_again", amount).await;
        assert_eq!((st, out["status"].as_str()), (StatusCode::OK, Some("paid")), "{code}: {out}");
    }
    // Already processed but the lookup still shows READY: unclear, not a decline the page would report.
    let (id, amount, _) = checkout(&w, "card").await;
    {
        let mut f = w.toss.lock().expect("fake");
        f.push("confirm", 400, json!({ "code": "ALREADY_PROCESSED_PAYMENT" }));
        f.paid(&id.to_string(), amount);
        f.orders.get_mut(&id.to_string()).expect("order")["status"] = json!("READY");
    }
    let (st, out) = toss_confirm(&w, id, "pk_ready", amount).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{out}");
    assert_eq!(checkout_state(&pool, id).await.0, "pending");

    // A decline: nothing captured, the checkout stays open for another try.
    let (id, amount, _) = checkout(&w, "card").await;
    w.toss.lock().expect("fake").push("confirm", 403, json!({ "code": "REJECT_CARD_COMPANY" }));
    let (st, out) = toss_confirm(&w, id, "pk_no", amount).await;
    assert_eq!(st, StatusCode::PAYMENT_REQUIRED, "{out}");
    assert_eq!(checkout_state(&pool, id).await.0, "pending");
}

/// Windows: the customer may tap "done" before the success page captured. That is "not yet", never a decline.
#[sqlx::test(migrations = "../../migrations")]
async fn an_early_app_confirm_is_pending_not_declined(pool: PgPool) {
    let w = world(&pool, "010-7600-0016", true).await;
    let (id, amount, _) = checkout(&w, "card").await;
    let app_confirm = || async {
        let body = json!({ "payment_id": id.to_string() });
        call(&w.env.app, "POST", &format!("/v1/checkouts/{id}/confirm"), Some(w.c.token.as_str()), Some(body)).await
    };
    let (st, out) = app_confirm().await;
    assert_eq!(
        (st, out["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("payment_pending")),
        "unknown order: {out}"
    );
    {
        let mut f = w.toss.lock().expect("fake");
        f.paid(&id.to_string(), amount);
        f.orders.get_mut(&id.to_string()).expect("order")["status"] = json!("IN_PROGRESS");
    }
    let (st, out) = app_confirm().await;
    assert_eq!(
        (st, out["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("payment_pending")),
        "authenticated: {out}"
    );
    assert_eq!(checkout_state(&pool, id).await.0, "pending");
    w.toss.lock().expect("fake").orders.get_mut(&id.to_string()).expect("order")["status"] = json!("DONE");
    let (st, out) = app_confirm().await;
    assert_eq!((st, out["status"].as_str()), (StatusCode::OK, Some("paid")), "{out}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_confirm_refuses_a_portone_checkout_and_bad_keys(pool: PgPool) {
    let w = world(&pool, "010-7600-0004", false).await;
    let (id, amount, _) = checkout(&w, "transfer").await;
    let (st, out) = toss_confirm(&w, id, "pk_x", amount).await;
    assert_eq!((st, out["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("payment_not_for_checkout")));
    let (st, out) = toss_confirm(&w, id, "pk x", amount).await;
    assert_eq!((st, out["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_input")));
    let (st, _) = toss_confirm(&w, Uuid::now_v7(), "pk_x", amount).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert!(w.toss.lock().expect("fake").calls.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn virtual_account_waits_for_the_deposit_callback(pool: PgPool) {
    let w = world(&pool, "010-7600-0005", true).await;
    let (id, amount, _) = checkout(&w, "virtual_account").await;
    w.toss.lock().expect("fake").confirm_as.insert(
        id.to_string(),
        json!({ "status": "WAITING_FOR_DEPOSIT", "method": "가상계좌",
                "virtualAccount": { "bankCode": "88", "accountNumber": "X12345678", "dueDate": "2026-11-20T00:00:00+09:00" } }),
    );
    let (st, out) = toss_confirm(&w, id, "pk_va", amount).await;
    assert_eq!((st, out["status"].as_str()), (StatusCode::OK, Some("awaiting_deposit")), "{out}");
    let va: Value = sqlx::query_scalar("SELECT virtual_account FROM checkouts WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("va");
    assert_eq!(va["bank"], "SHINHAN");
    assert_eq!(va["accountNumber"], "X12345678");
    assert_eq!(va["remitteeName"], "다해", "Toss names no holder: the brand is shown");

    // Deposit: Toss flips the payment to DONE and sends DEPOSIT_CALLBACK (no eventType, top-level fields).
    w.toss.lock().expect("fake").orders.get_mut(&id.to_string()).expect("order")["status"] = json!("DONE");
    let callback = json!({ "createdAt": "2026-11-18T10:00:00.000000", "secret": "s", "status": "DONE", "transactionKey": "t1", "orderId": id.to_string() });
    assert_eq!(webhook(&w.env.app, &callback, None).await, StatusCode::OK);
    assert_eq!(checkout_state(&pool, id).await.0, "paid", "the re-read DONE settles it");

    // 입금 오류: Toss moves DONE back to WAITING_FOR_DEPOSIT and calls again. The booking stays, staff hear of it once.
    w.toss.lock().expect("fake").orders.get_mut(&id.to_string()).expect("order")["status"] = json!("WAITING_FOR_DEPOSIT");
    let reversed =
        json!({ "createdAt": "2026-11-18T11:00:00.000000", "secret": "s", "status": "WAITING_FOR_DEPOSIT", "transactionKey": "t2", "orderId": id.to_string() });
    assert_eq!(webhook(&w.env.app, &reversed, None).await, StatusCode::OK);
    assert_eq!(webhook(&w.env.app, &reversed, None).await, StatusCode::OK, "a repeat is harmless");
    let booking = checkout_state(&pool, id).await.2.expect("booking");
    let cases: i64 = sqlx::query_scalar("SELECT count(*) FROM cases WHERE booking_id = $1 AND kind = 'payment_dispute'")
        .bind(booking)
        .fetch_one(&pool)
        .await
        .expect("cases");
    assert_eq!(cases, 1, "one case per order");
    let alert: Option<String> = sqlx::query_scalar("SELECT last_error FROM ops_health WHERE integration = 'payment_stuck'")
        .fetch_optional(&pool)
        .await
        .expect("health");
    assert_eq!(alert.as_deref(), Some("deposit_reversed"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_reconciler_settles_a_done_order_we_never_confirmed(pool: PgPool) {
    let w = world(&pool, "010-7600-0006", true).await;
    let (id, amount, _) = checkout(&w, "card").await;
    w.toss.lock().expect("fake").paid(&id.to_string(), amount);
    sqlx::query("UPDATE checkouts SET created_at = now() - interval '5 minutes' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("age");
    checkout_recon::reconcile_pass(&w.env.state).await.expect("pass");
    assert_eq!(checkout_state(&pool, id).await.0, "paid");
    let orders = w.toss.lock().expect("fake").calls_of("order");
    assert!(orders.iter().all(|c| c.secret == WINDOW_SK), "looked up on the window MID");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_config_change_never_moves_a_row(pool: PgPool) {
    let w = world(&pool, "010-7600-0007", false).await;
    // Paid on PortOne, then the owner turns Toss on: the refund still goes to PortOne.
    let (po, amount, _) = checkout(&w, "transfer").await;
    w.portone.lock().expect("portone").payments.insert(po.to_string(), (amount, 0));
    let tok = Some(w.c.token.as_str());
    let (st, done) = call(
        &w.env.app,
        "POST",
        &format!("/v1/checkouts/{po}/confirm"),
        tok,
        Some(json!({ "payment_id": po.to_string() })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{done}");
    let po_booking: Uuid = done["booking_id"].as_str().expect("booking").parse().expect("uuid");
    set_toss(&w.env, toss::card(true)).await;
    cancel_booking(&w, po_booking).await;
    assert_eq!(w.portone.lock().expect("portone").cancels, [po.to_string()]);
    assert!(w.toss.lock().expect("fake").calls_of("cancel").is_empty());

    // A PortOne saved card keeps charging PortOne while Toss serves one-off methods.
    let (booking, _) = common::book(&w.env.app, &w.c, 6, None).await;
    assert_eq!(w.portone.lock().expect("portone").charges.len(), 1);
    assert_eq!(payment_provider(&pool, booking).await.0, "portone");

    // Paid on Toss, then the owner turns 사용 off: the refund still goes to Toss, guarded by the balance.
    let (tc, amount, _) = checkout(&w, "card").await;
    assert_eq!(toss_confirm(&w, tc, "pk_sw", amount).await.0, StatusCode::OK);
    let tc_booking = checkout_state(&pool, tc).await.2.expect("booking");
    set_toss(&w.env, toss::card(false)).await;
    cancel_booking(&w, tc_booking).await;
    let cancels = w.toss.lock().expect("fake").calls_of("cancel");
    assert_eq!(cancels.len(), 1);
    assert_eq!(cancels[0].secret, WINDOW_SK);
    assert_eq!(cancels[0].path, "/v1/payments/pk_sw/cancel", "the paymentKey comes from the lookup");
    assert_eq!(cancels[0].body["refundableAmount"].as_i64(), Some(amount));
    assert_eq!(w.portone.lock().expect("portone").cancels.len(), 1, "PortOne saw only its own refund");
    common::golden("gateway_switch_bodies", &gateway_bodies(&w));
}

/// Registers a card under Toss. Returns the payment method id and the customerKey (the ticket).
async fn register_toss_card(w: &World) -> (Uuid, String) {
    let tok = Some(w.c.token.as_str());
    let (st, setup) = call(&w.env.app, "GET", "/v1/me/payment-methods/setup?gateways=portone,tosspayments", tok, None).await;
    assert_eq!(st, StatusCode::OK, "{setup}");
    assert_eq!(setup["gateway"], "tosspayments");
    assert_eq!(setup["store_id"], Value::Null);
    assert_eq!(setup["channels"], json!({ "card": toss::BILLING_CK }));
    let issue = setup["issue_id"].as_str().expect("issue").to_string();
    assert_eq!(Uuid::parse_str(&issue).expect("uuid").get_version_num(), 4, "unguessable customerKey");
    let body = json!({ "provider": "card", "auth_key": "auth_1", "display_name": "토스 카드" });
    let (st, row) = call(&w.env.app, "POST", "/v1/me/payment-methods", tok, Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{row}");
    (row["id"].as_str().expect("id").parse().expect("uuid"), issue)
}

#[sqlx::test(migrations = "../../migrations")]
async fn saved_cards_under_toss_use_the_billing_mid(pool: PgPool) {
    let w = world(&pool, "010-7600-0008", true).await;
    let tok = Some(w.c.token.as_str());
    let (st, methods) = call(&w.env.app, "GET", "/v1/payments/methods?gateways=portone,tosspayments", None, None).await;
    assert_eq!(st, StatusCode::OK);
    let flow = |code: &str| methods.as_array().expect("list").iter().find(|m| m["code"] == code).map(|m| m["flow"].clone());
    assert_eq!(flow("card"), Some(json!("billing")));
    assert_eq!(flow("kakaopay"), Some(json!("checkout")), "Toss saves cards only");
    let easy = json!({ "provider": "kakaopay", "auth_key": "a", "display_name": "x" });
    assert_eq!(
        call(&w.env.app, "POST", "/v1/me/payment-methods", tok, Some(easy)).await.0,
        StatusCode::BAD_REQUEST
    );

    let (card, customer_key) = register_toss_card(&w).await;
    let issued = w.toss.lock().expect("fake").calls_of("issue");
    assert_eq!(issued[0].secret, BILLING_SK);
    assert_eq!(issued[0].body, json!({ "authKey": "auth_1", "customerKey": customer_key }));
    let (gateway, stored_customer, sealed): (String, Option<String>, String) =
        sqlx::query_as("SELECT gateway, customer_key, billing_key FROM payment_methods WHERE id = $1")
            .bind(card)
            .fetch_one(&pool)
            .await
            .expect("row");
    assert_eq!((gateway.as_str(), stored_customer.as_deref()), ("tosspayments", Some(customer_key.as_str())));
    assert!(!sealed.contains("bk_auth_1"), "the billing key is sealed");

    // A booking with the card: charged on the billing MID under our pid, recorded as tosspayments_billing.
    let paying = Customer { card, ..clone_customer(&w.c) };
    let (booking, _) = common::book(&w.env.app, &paying, 5, None).await;
    let charges = w.toss.lock().expect("fake").calls_of("charge");
    assert_eq!(charges.len(), 1);
    assert_eq!((charges[0].secret.as_str(), charges[0].path.as_str()), (BILLING_SK, "/v1/billing/bk_auth_1"));
    assert_eq!(charges[0].body["customerKey"].as_str(), Some(customer_key.as_str()));
    let (provider, pid, _) = payment_provider(&pool, booking).await;
    assert_eq!(provider, "tosspayments_billing");
    assert_eq!(charges[0].body["orderId"].as_str(), Some(pid.as_str()));

    // Its refund also goes to the billing MID.
    cancel_booking(&w, booking).await;
    let cancels = w.toss.lock().expect("fake").calls_of("cancel");
    assert_eq!(cancels.len(), 1);
    assert_eq!(cancels[0].secret, BILLING_SK);

    // Deleting the card fails closed when Toss refuses, then deletes it at Toss.
    w.toss
        .lock()
        .expect("fake")
        .push("delete", 500, json!({ "code": "FAILED_INTERNAL_SYSTEM_PROCESSING" }));
    let uri = format!("/v1/me/payment-methods/{card}");
    assert_eq!(call(&w.env.app, "DELETE", &uri, tok, None).await.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(call(&w.env.app, "DELETE", &uri, tok, None).await.0, StatusCode::OK);
    let deletes = w.toss.lock().expect("fake").calls_of("delete");
    assert_eq!(deletes.len(), 2);
    assert_eq!((deletes[1].secret.as_str(), deletes[1].path.as_str()), (BILLING_SK, "/v1/billing/bk_auth_1"));
    common::golden("toss_billing_bodies", &gateway_bodies(&w));
}

fn clone_customer(c: &Customer) -> Customer {
    Customer {
        token: c.token.clone(),
        user: c.user,
        address: c.address,
        card: c.card,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lost_charge_answer_is_looked_up_never_charged_twice(pool: PgPool) {
    let w = world(&pool, "010-7600-0009", true).await;
    let (card, _) = register_toss_card(&w).await;
    let paying = Customer { card, ..clone_customer(&w.c) };
    // The charge lands at Toss but its answer is a 500: the lookup finds it DONE and the booking goes through.
    w.toss.lock().expect("fake").lose_next_charge_answer = true;
    let (booking, _) = common::book(&w.env.app, &paying, 5, None).await;
    assert_eq!(payment_provider(&pool, booking).await.0, "tosspayments_billing");
    let lookups = w.toss.lock().expect("fake").calls_of("order");
    assert_eq!(lookups.len(), 1, "looked up once");
    assert_eq!(lookups[0].secret, BILLING_SK, "on the billing MID");
    assert_eq!(w.toss.lock().expect("fake").calls_of("charge").len(), 1, "never charged twice");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refund_whose_answer_was_lost_is_recorded_and_a_moved_balance_is_refused(pool: PgPool) {
    let w = world(&pool, "010-7600-0010", true).await;
    let p = &w.env.state.providers;
    {
        let mut f = w.toss.lock().expect("fake");
        f.paid("order-landed", 10_000);
        f.orders.get_mut("order-landed").expect("order")["balanceAmount"] = json!(7_000);
        f.paid("order-moved", 10_000);
        f.orders.get_mut("order-moved").expect("order")["balanceAmount"] = json!(5_000);
    }
    p.refund_checked("tosspayments", "order-landed", 3_000, 10_000)
        .await
        .expect("our earlier cancel landed");
    let moved = p.refund_checked("tosspayments", "order-moved", 3_000, 10_000).await;
    assert!(
        matches!(moved, Err(ProviderError::Declined(ref c)) if c == api::providers::portone::BALANCE_MOVED),
        "{moved:?}"
    );
    assert!(w.toss.lock().expect("fake").calls_of("cancel").is_empty(), "neither sent a second cancel");
}

#[sqlx::test(migrations = "../../migrations")]
async fn webhooks_settle_and_flag_console_cancels_once(pool: PgPool) {
    let w = world(&pool, "010-7600-0011", true).await;
    // PAYMENT_STATUS_CHANGED DONE for a checkout whose success page never posted.
    let (id, amount, _) = checkout(&w, "card").await;
    w.toss.lock().expect("fake").paid(&id.to_string(), amount);
    let done = json!({ "eventType": "PAYMENT_STATUS_CHANGED", "createdAt": "x", "data": { "orderId": id.to_string(), "status": "DONE" } });
    assert_eq!(webhook(&w.env.app, &done, None).await, StatusCode::OK);
    let booking = checkout_state(&pool, id).await.2.expect("settled by the webhook");

    // Cancelled in the Toss console: one dispute and one case, a replay changes nothing.
    {
        let mut f = w.toss.lock().expect("fake");
        let o = f.orders.get_mut(&id.to_string()).expect("order");
        o["balanceAmount"] = json!(0);
        o["status"] = json!("CANCELED");
    }
    let cancelled = json!({ "eventType": "PAYMENT_STATUS_CHANGED", "data": { "orderId": id.to_string(), "status": "CANCELED" } });
    assert_eq!(webhook(&w.env.app, &cancelled, None).await, StatusCode::OK);
    assert_eq!(webhook(&w.env.app, &cancelled, None).await, StatusCode::OK);
    let disputes: i64 = sqlx::query_scalar("SELECT count(*) FROM disputes").fetch_one(&pool).await.expect("disputes");
    let cases: Vec<String> = sqlx::query_scalar("SELECT summary FROM cases WHERE kind = 'payment_dispute'")
        .fetch_all(&pool)
        .await
        .expect("cases");
    assert_eq!(disputes, 1);
    assert_eq!(cases.len(), 1);
    assert!(cases[0].contains("Toss Payments"), "{}", cases[0]);
    assert_eq!(payment_provider(&pool, booking).await.2, amount, "recorded as refunded in full");

    // A bad signature is refused; an unknown order is acknowledged.
    assert_eq!(webhook(&w.env.app, &done, Some("v1:bogus")).await, StatusCode::UNAUTHORIZED);
    let unknown = json!({ "eventType": "PAYMENT_STATUS_CHANGED", "data": { "orderId": "nobody-1", "status": "DONE" } });
    assert_eq!(webhook(&w.env.app, &unknown, None).await, StatusCode::OK);
    let ignored = json!({ "eventType": "BILLING_DELETED", "data": { "billingKey": "k" } });
    assert_eq!(webhook(&w.env.app, &ignored, None).await, StatusCode::OK);
    let ledger: i64 = sqlx::query_scalar("SELECT count(*) FROM webhook_events")
        .fetch_one(&pool)
        .await
        .expect("ledger");
    assert_eq!(
        ledger, 0,
        "unsigned hints never enter the ledger, so strangers cannot fill it or pre-record a body"
    );
}

fn owner_code(seed: &[u8]) -> String {
    format!(
        "{:06}",
        api::totp::code_at(seed, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_console_card_saves_with_the_owner_code_and_tests_both_mids(pool: PgPool) {
    let w = world(&pool, "010-7600-0012", false).await;
    sqlx::query("DELETE FROM integration_settings WHERE section = 'tosspayments'")
        .execute(&pool)
        .await
        .expect("unsaved");
    api::routes::setup::reload(&w.env.state).await.expect("reload");
    let (owner, owner_id) = staff_token(&pool, Role::StaffAdmin).await;
    let seed = b"12345678901234567890";
    enrol_owner(&pool, owner_id, seed).await;
    let app = &w.env.app;
    let put = |values: Value, code: Option<String>| {
        let owner = owner.clone();
        async move { call_owner(app, "PUT", "/v1/admin/setup/tosspayments", &owner, code.as_deref(), values).await }
    };

    let mut card = Value::Object(toss::card(false));
    let bad = |field: &str, v: &str| {
        let mut c = card.clone();
        c[field] = json!(v);
        c
    };
    for (values, code) in [
        (bad("client_key", "test_gck_widget"), "invalid_client_key"),
        (bad("secret_key", "test_gsk_widget"), "invalid_secret_key"),
        (bad("billing_secret_key", "live_sk_other"), "key_mode_mismatch"),
        (bad("security_key", "not-hex"), "invalid_security_key"),
    ] {
        let (st, out) = put(values, Some(owner_code(seed))).await;
        assert_eq!((st, out["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some(code)), "{out}");
    }
    let mut half = card.clone();
    half["billing_secret_key"] = Value::Null;
    let (st, out) = put(half, Some(owner_code(seed))).await;
    assert_eq!(
        (st, out["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("missing_field")),
        "both billing keys or neither"
    );

    let (st, _) = put(card.clone(), None).await;
    assert_ne!(st, StatusCode::OK, "the first save needs the owner code");
    let (st, saved) = put(card.clone(), Some(owner_code(seed))).await;
    assert_eq!(st, StatusCode::OK, "{saved}");
    assert_eq!((saved["mode"].as_str(), saved["locked"].as_bool()), (Some("test"), Some(true)));
    let shown = |name: &str| saved["fields"].as_array().expect("fields").iter().find(|f| f["name"] == name).expect("field")["preview"].clone();
    assert_eq!(shown("client_key"), json!(WINDOW_CK), "client keys are public");
    assert_eq!(shown("secret_key"), json!("••••"), "secrets are masked");
    card["active"] = json!(true);
    let (st, out) = put(card, None).await;
    assert_eq!(
        (st, out["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("card_locked")),
        "switching needs an unlock"
    );

    let (st, test) = call(app, "POST", "/v1/admin/setup/tosspayments/test", Some(owner.as_str()), None).await;
    assert_eq!((st, test["code"].as_str()), (StatusCode::OK, Some("connected")), "{test}");
    let probes = w.toss.lock().expect("fake").calls_of("order");
    assert_eq!(
        probes.iter().map(|c| c.secret.as_str()).collect::<Vec<_>>(),
        [WINDOW_SK, BILLING_SK],
        "both MIDs"
    );
    w.toss.lock().expect("fake").push("order", 404, json!({ "code": "NOT_FOUND_PAYMENT" }));
    w.toss.lock().expect("fake").push("order", 401, json!({ "code": "UNAUTHORIZED_KEY" }));
    let (_, test) = call(app, "POST", "/v1/admin/setup/tosspayments/test", Some(owner.as_str()), None).await;
    assert_eq!(test["code"], "billing_invalid_key");
    // Toss also answers a bad secret with 400 INVALID_API_KEY: a key to fix, not an outage.
    w.toss.lock().expect("fake").push("order", 400, json!({ "code": "INVALID_API_KEY" }));
    let (_, test) = call(app, "POST", "/v1/admin/setup/tosspayments/test", Some(owner.as_str()), None).await;
    assert_eq!(test["code"], "invalid_key");

    let (st, overview) = call(app, "GET", "/v1/admin/setup", Some(owner.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(overview["toss_payments_webhook_url"], "http://127.0.0.1:8080/v1/webhooks/tosspayments");
    assert_eq!(
        overview["payment_gateways"],
        json!([{ "code": "portone", "mode": null }]),
        "사용 off: Toss not listed"
    );
    assert_eq!(overview["payment_gateway"], "portone");
    let toss_card = overview["integrations"]
        .as_array()
        .expect("cards")
        .iter()
        .find(|c| c["code"] == "tosspayments")
        .cloned();
    assert_eq!(toss_card.expect("card")["mode"], "test");
    assert_eq!(overview["payment_gateway_mode"], Value::Null, "PortOne is live");
    set_toss(&w.env, toss::card(true)).await;
    let (_, overview) = call(app, "GET", "/v1/admin/setup", Some(owner.as_str()), None).await;
    assert_eq!(
        overview["payment_gateways"],
        json!([{ "code": "portone", "mode": null }, { "code": "tosspayments", "mode": "test" }]),
        "both take payments, PortOne first; Toss on test keys: the console warns"
    );
    assert_eq!(overview["payment_gateway"], "portone", "older consoles see the first gateway");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_toss_key_of_another_mid_is_refused_while_its_payments_remain(pool: PgPool) {
    let w = world(&pool, "010-7600-0040", true).await;
    let (id, amount, _) = checkout(&w, "card").await;
    assert_eq!(toss_confirm(&w, id, "pk_1", amount).await.0, StatusCode::OK);
    let (owner, owner_id) = staff_token(&pool, Role::StaffAdmin).await;
    enrol_owner(&pool, owner_id, b"12345678901234567890").await;
    sqlx::query("UPDATE integration_settings SET unlocked_until = now() + interval '5 minutes', unlocked_by = $1 WHERE section = 'tosspayments'")
        .bind(owner_id)
        .execute(&pool)
        .await
        .expect("unlocked");
    let put = |secret: &str| {
        let mut card = toss::card(true);
        card.insert("secret_key".into(), json!(secret));
        call_owner(&w.env.app, "PUT", "/v1/admin/setup/tosspayments", &owner, None, Value::Object(card))
    };

    let (st, out) = put("test_sk_othermid0001").await;
    assert_eq!(
        (st, out["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("gateway_account_changed")),
        "refunds of that payment would go to another MID: {out}"
    );
    // A new key of the same MID reads its payments (the fake accepts this key on every order).
    let (st, out) = put(BILLING_SK).await;
    assert_eq!(st, StatusCode::OK, "{out}");
    let probes = w.toss.lock().expect("fake").calls_of("order");
    assert_eq!(
        probes.last().map(|c| c.path.clone()),
        Some(format!("/v1/payments/orders/{id}")),
        "the payment itself is read"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cash_receipts_and_tax_invoices_follow_the_gateway(pool: PgPool) {
    let w = world(&pool, "010-7600-0013", true).await;
    // PortOne serves transfer in this world, so the Toss leg uses the other cash-eligible method.
    let (id, amount, _) = checkout(&w, "virtual_account").await;
    let body = json!({ "purpose": "PERSONAL", "identity": "01076000013" });
    let (st, out) = call(
        &w.env.app,
        "PUT",
        &format!("/v1/checkouts/{id}/cash-receipt"),
        Some(w.c.token.as_str()),
        Some(body),
    )
    .await;
    assert_eq!(
        (st, out["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("cash_receipt_method")),
        "{out}"
    );

    let request: Uuid = sqlx::query_scalar(
        "INSERT INTO tax_invoice_requests (checkout_id, business_number, company_name, email, state)
         VALUES ($1, '2208162517', 'TOSS-CO', 'tax@example.test', 'pending') RETURNING id",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("request");
    assert_eq!(toss_confirm(&w, id, "pk_tax", amount).await.0, StatusCode::OK);
    let linked: (String, Option<Uuid>) = sqlx::query_as("SELECT state, payment_id FROM tax_invoice_requests WHERE id = $1")
        .bind(request)
        .fetch_one(&pool)
        .await
        .expect("linked");
    assert_eq!(linked.0, "requested");
    assert!(linked.1.is_some(), "linked to the Toss payment row");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_daily_diff_runs_per_gateway(pool: PgPool) {
    let w = world(&pool, "010-7600-0014", true).await;
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 17).expect("day");
    let noon = day.and_hms_opt(3, 0, 0).expect("time").and_utc();
    // Two Toss checkouts paid that day: one Toss lists, one it does not. PortOne lists nothing.
    for (order, amount) in [("ck-listed", 9000), ("ck-lost", 4000)] {
        sqlx::query(
            "INSERT INTO checkouts (user_id, kind, method, amount, status, provider_payment_id, expires_at, created_at, gateway)
             VALUES ($1, 'booking', 'card', $2, 'paid', $3, now() + interval '1 day', $4, 'tosspayments')",
        )
        .bind(w.c.user)
        .bind(amount)
        .bind(order)
        .bind(noon)
        .execute(&pool)
        .await
        .expect("checkout");
    }
    w.toss.lock().expect("fake").transactions = vec![
        json!({ "orderId": "ck-listed", "status": "DONE", "amount": 9000, "transactionAt": "2026-11-17T12:00:00+09:00", "transactionKey": "t1" }),
        json!({ "orderId": "ck-ghost", "status": "DONE", "amount": 1000, "transactionAt": "2026-11-17T13:00:00+09:00", "transactionKey": "t2" }),
    ];
    let run_at = chrono::NaiveDate::from_ymd_opt(2026, 11, 18)
        .expect("day")
        .and_hms_opt(0, 0, 0)
        .expect("t")
        .and_utc();
    let found = checkout_recon::payment_diff_at(&w.env.state, run_at).await.expect("diff");
    let codes: Vec<String> = sqlx::query_scalar("SELECT code FROM ops_failures WHERE integration = 'payment_mismatch' ORDER BY code")
        .fetch_all(&pool)
        .await
        .expect("codes");
    assert_eq!(codes, ["missing_at_provider", "missing_locally"], "PortOne does not flag the Toss checkouts");
    assert_eq!(found, 2);
    let listed = w.toss.lock().expect("fake").calls_of("transactions");
    assert_eq!(
        listed.iter().map(|c| c.secret.as_str()).collect::<Vec<_>>(),
        [WINDOW_SK, BILLING_SK],
        "one pull per MID"
    );
    assert!(
        listed[0].path.contains("startDate=2026-11-16T23%3A00%3A00"),
        "KST wall time: {}",
        listed[0].path
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_subscription_visit_charges_its_toss_card_and_retries_the_same_order(pool: PgPool) {
    let w = world(&pool, "010-7600-0015", true).await;
    let (card, customer_key) = register_toss_card(&w).await;
    let tomorrow = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive() + chrono::Duration::days(1);
    let weekday = chrono::Datelike::weekday(&tomorrow).num_days_from_monday();
    let body = json!({ "service_code": "studio", "address_id": w.c.address, "payment_method_id": card,
                       "cadence_weeks": 1, "weekday": weekday, "start_hour": 9 });
    let (st, sub) = call(&w.env.app, "POST", "/v1/subscriptions", Some(w.c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{sub}");

    // The first answer is lost before Toss charged: the visit waits with its order id stored.
    w.toss
        .lock()
        .expect("fake")
        .push("charge", 500, json!({ "code": "FAILED_INTERNAL_SYSTEM_PROCESSING" }));
    let first = api::routes::subscription_engine::pass(&w.env.state).await.expect("pass");
    assert_eq!(first.booked, 0, "{first:?}");
    // The next pass looks the order up on the billing MID, finds nothing, and sends the same order again.
    let second = api::routes::subscription_engine::pass(&w.env.state).await.expect("pass");
    assert_eq!(second.booked, 1, "{second:?}");
    let (charges, orders) = {
        let f = w.toss.lock().expect("fake");
        (f.calls_of("charge"), f.calls_of("order"))
    };
    assert_eq!(charges.len(), 2);
    assert_eq!(
        charges[0].body["orderId"], charges[1].body["orderId"],
        "the same order id: Toss cannot charge twice"
    );
    assert_eq!(charges[1].body["customerKey"].as_str(), Some(customer_key.as_str()));
    assert!(orders.iter().all(|c| c.secret == BILLING_SK));
    let provider: String = sqlx::query_scalar("SELECT p.provider FROM payments p JOIN bookings b ON b.id = p.booking_id WHERE b.subscription_id IS NOT NULL")
        .fetch_one(&pool)
        .await
        .expect("payment");
    assert_eq!(provider, "tosspayments_billing");
}

/// Starts a weekly subscription on the customer's PortOne card whose first visit is tomorrow. Returns the visit id.
async fn portone_subscription(w: &World) -> Uuid {
    let tomorrow = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive() + chrono::Duration::days(1);
    let weekday = chrono::Datelike::weekday(&tomorrow).num_days_from_monday();
    let body = json!({ "service_code": "studio", "address_id": w.c.address, "payment_method_id": w.c.card,
                       "cadence_weeks": 1, "weekday": weekday, "start_hour": 9 });
    let (st, sub) = call(&w.env.app, "POST", "/v1/subscriptions", Some(w.c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{sub}");
    sqlx::query_scalar(
        "SELECT v.id FROM subscription_visits v JOIN subscriptions s ON s.id = v.subscription_id WHERE s.user_id = $1 ORDER BY v.visit_date LIMIT 1",
    )
    .bind(w.c.user)
    .fetch_one(&w.env.state.db)
    .await
    .expect("visit")
}

async fn move_subscription_to(w: &World, card: Uuid) {
    sqlx::query("UPDATE subscriptions SET payment_method_id = $2 WHERE user_id = $1")
        .bind(w.c.user)
        .bind(card)
        .execute(&w.env.state.db)
        .await
        .expect("card change");
}

/// A visit's charge id stays with the gateway it was sent to, even when the subscription moves to a Toss card.
#[sqlx::test(migrations = "../../migrations")]
async fn an_open_subscription_charge_stays_on_its_gateway_after_a_card_change(pool: PgPool) {
    let w = world(&pool, "010-7600-0017", true).await;
    let (toss_card, _) = register_toss_card(&w).await;
    portone_subscription(&w).await;
    let pass = || api::routes::subscription_engine::pass(&w.env.state);

    // PortOne charged, its answer was lost; then the card changes to Toss. The next pass asks PortOne and books.
    w.portone.lock().expect("portone").fail_next_charge = Some(true);
    assert_eq!(pass().await.expect("pass").booked, 0);
    move_subscription_to(&w, toss_card).await;
    assert_eq!(pass().await.expect("pass").booked, 1);
    assert!(w.toss.lock().expect("fake").calls_of("charge").is_empty(), "never charged again at Toss");
    let provider: String = sqlx::query_scalar("SELECT p.provider FROM payments p JOIN bookings b ON b.id = p.booking_id WHERE b.subscription_id IS NOT NULL")
        .fetch_one(&pool)
        .await
        .expect("payment");
    assert_eq!(provider, "portone");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_charge_unknown_to_its_old_gateway_is_retried_on_the_new_card_under_a_new_id(pool: PgPool) {
    let w = world(&pool, "010-7600-0018", true).await;
    let (toss_card, _) = register_toss_card(&w).await;
    let visit = portone_subscription(&w).await;
    let pass = || api::routes::subscription_engine::pass(&w.env.state);

    // PortOne never saw the charge. After the card change the stored id is dropped, not sent to Toss.
    w.portone.lock().expect("portone").fail_next_charge = Some(false);
    assert_eq!(pass().await.expect("pass").booked, 0);
    let sent = w.portone.lock().expect("portone").charges.clone();
    move_subscription_to(&w, toss_card).await;
    assert_eq!(pass().await.expect("pass").booked, 0);
    assert!(w.toss.lock().expect("fake").calls_of("charge").is_empty());
    assert_eq!(pass().await.expect("pass").booked, 1);
    let charges = w.toss.lock().expect("fake").calls_of("charge");
    assert_eq!(charges.len(), 1);
    assert_ne!(charges[0].body["orderId"].as_str(), sent.first().map(String::as_str), "a fresh id");
    assert!(charges[0].body["orderId"].as_str().expect("id").starts_with(&format!("subvisit-{visit}-")));
}

/// A queued Toss saved-card refund for a charge Toss never saw stops once Toss would long have answered.
#[sqlx::test(migrations = "../../migrations")]
async fn a_refund_of_a_charge_toss_never_made_stops_retrying(pool: PgPool) {
    let w = world(&pool, "010-7600-0019", true).await;
    let id = sqlx::query_scalar(
        "INSERT INTO refund_queue (provider, tx_id, amount, cancellable, attempts, next_at)
         VALUES ('tosspayments_billing', 'full-never', 5000, 5000, 2, now()) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("row");
    let row = |attempts| api::util::QueuedRefund {
        id,
        provider: "tosspayments_billing".into(),
        tx_id: "full-never".into(),
        amount: 5000,
        cancellable: 5000,
        attempts,
        checkout_id: None,
    };
    // Earlier tries still retry: the charge may land late.
    assert_eq!(api::util::retry_refund(&w.env.state, &row(1)).await, api::util::Refund::Pending);
    assert_eq!(api::util::retry_refund(&w.env.state, &row(2)).await, api::util::Refund::Done);
    assert!(w.toss.lock().expect("fake").calls_of("cancel").is_empty());
    let alert: Option<String> = sqlx::query_scalar("SELECT last_error FROM ops_health WHERE integration = 'payment_stuck'")
        .fetch_optional(&pool)
        .await
        .expect("health");
    assert_eq!(alert, None, "no stuck-refund alert for a charge that never happened");
}

/// One gateway leg failing keeps the other legs' mismatches and leaves the day to be diffed again.
#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_diff_leg_keeps_the_other_legs(pool: PgPool) {
    let w = world(&pool, "010-7600-0020", true).await;
    {
        let mut f = w.toss.lock().expect("fake");
        f.push("transactions", 500, json!({ "code": "FAILED_INTERNAL_SYSTEM_PROCESSING" }));
        f.billing_transactions =
            vec![json!({ "orderId": "full-ghost", "status": "DONE", "amount": 1000, "transactionAt": "2026-11-17T13:00:00+09:00", "transactionKey": "t9" })];
    }
    let run_at = chrono::NaiveDate::from_ymd_opt(2026, 11, 18)
        .expect("day")
        .and_hms_opt(0, 0, 0)
        .expect("t")
        .and_utc();
    let found = checkout_recon::payment_diff_at(&w.env.state, run_at)
        .await
        .expect("the pass survives a failing leg");
    assert_eq!(found, 1);
    let codes: Vec<String> = sqlx::query_scalar("SELECT code FROM ops_failures WHERE integration = 'payment_mismatch' ORDER BY code")
        .fetch_all(&pool)
        .await
        .expect("codes");
    assert_eq!(codes, ["diff_leg_failed", "missing_locally"]);
    let state: Value = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'payment_diff'")
        .fetch_one(&pool)
        .await
        .expect("state");
    assert_eq!(state["done_day"], Value::Null, "the day is tried again");
}
