//! Payment path end to end against a PortOne mock with DEV_MODE off: the owner saves the PortOne keys
//! (with a webhook secret), a checkout is confirmed, a signed Transaction.Paid arrives, the customer cancels
//! late (a partial refund sent to PortOne), and a signed Transaction.PartialCancelled arrives without moving
//! money again. The Toss payout step is not covered: its API base is fixed in providers/payout.rs (no mock seam).
//! Needs DATABASE_URL (sqlx::test). The mock is a local axum server, so no extra dependency.

mod common;

use api::auth::Role;
use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, Request, StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use common::{call, customer, prod_app, seed_catalog, staff_token};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;
use uuid::Uuid;

const E2E_SECRET: &str = "portone-e2e-secret";
const STORE: &str = "store-e2e-1";
/// Test-only webhook key (32 bytes). The saved secret is `whsec_` + its base64.
const WEBHOOK_KEY: &[u8] = b"e2e-webhook-key-0123456789abcdef";

/// What the mock PortOne saw: payment id -> (total, cancelled), and every cancel body in order.
#[derive(Default)]
struct Gateway {
    payments: HashMap<String, (i64, i64)>,
    cancels: Vec<Value>,
}

type Shared = Arc<Mutex<Gateway>>;
type Reply = (StatusCode, Json<Value>);

fn authorized(h: &HeaderMap) -> bool {
    h.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(format!("PortOne {E2E_SECRET}").as_str())
}

fn refused(status: StatusCode, kind: &str) -> Reply {
    (status, Json(json!({ "type": kind })))
}

async fn payment(State(g): State<Shared>, Path(id): Path<String>, h: HeaderMap) -> Reply {
    if !authorized(&h) {
        return refused(StatusCode::UNAUTHORIZED, "UnauthorizedError");
    }
    let g = g.lock().expect("gateway");
    let Some(&(total, cancelled)) = g.payments.get(&id) else {
        return refused(StatusCode::NOT_FOUND, "PaymentNotFoundError");
    };
    let status = match cancelled {
        0 => "PAID",
        c if c == total => "CANCELLED",
        _ => "PARTIAL_CANCELLED",
    };
    (
        StatusCode::OK,
        Json(json!({ "status": status, "amount": { "total": total, "cancelled": cancelled }, "currency": "KRW", "method": {} })),
    )
}

async fn cancel(State(g): State<Shared>, Path(id): Path<String>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    if !authorized(&h) {
        return refused(StatusCode::UNAUTHORIZED, "UnauthorizedError");
    }
    let mut g = g.lock().expect("gateway");
    g.cancels.push(b.clone());
    let Some(entry) = g.payments.get_mut(&id) else {
        return refused(StatusCode::NOT_FOUND, "PaymentNotFoundError");
    };
    let left = entry.0 - entry.1;
    let amount = b["amount"].as_i64().unwrap_or(left);
    if amount > left || b["currentCancellableAmount"].as_i64().is_some_and(|c| c != left) {
        return refused(StatusCode::CONFLICT, "CancelAmountExceedsCancellableAmountError");
    }
    entry.1 += amount;
    (
        StatusCode::OK,
        Json(json!({ "cancellation": { "status": "SUCCEEDED", "id": "cancel-1", "totalAmount": amount } })),
    )
}

async fn start_gateway() -> (String, Shared) {
    let g = Shared::default();
    let app = Router::new()
        .route("/payments/{id}", get(payment))
        .route("/payments/{id}/cancel", post(cancel))
        .with_state(g.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("mock serves") });
    (format!("http://{addr}"), g)
}

/// POSTs a PortOne webhook signed the Standard Webhooks way (`{id}.{timestamp}.{body}`, HMAC-SHA256).
async fn signed_webhook(app: &Router, kind: &str, payment_id: &str) -> StatusCode {
    let msg_id = format!("msg_{}", Uuid::now_v7());
    let ts = chrono::Utc::now().timestamp().to_string();
    let body = json!({
        "type": kind,
        "timestamp": "2026-11-16T00:00:00Z",
        "data": { "storeId": STORE, "paymentId": payment_id, "transactionId": "tx-e2e-1" }
    })
    .to_string();
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(WEBHOOK_KEY).expect("key");
    mac.update(format!("{msg_id}.{ts}.").as_bytes());
    mac.update(body.as_bytes());
    let signature = format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()));
    let mut req = Request::builder()
        .method("POST")
        .uri("/v1/webhooks/portone")
        .header("content-type", "application/json")
        .header("webhook-id", msg_id)
        .header("webhook-timestamp", ts)
        .header("webhook-signature", signature)
        .body(Body::from(body))
        .expect("request");
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    app.clone().oneshot(req).await.expect("router is infallible").status()
}

/// Late-cancel policy: 0% at three or more days out, 50% inside that (the fee is then kept).
async fn late_cancel_policy(db: &PgPool) {
    let policy = json!({ "tiers": [{ "min_days": 3, "percent": 0 }, { "min_days": -100, "percent": 50 }], "min_fee": 0, "reschedule_min_days": 2 });
    sqlx::query("INSERT INTO settings (key, value) VALUES ('cancel_policy', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(sqlx::types::Json(policy))
        .execute(db)
        .await
        .expect("policy");
}

#[sqlx::test(migrations = "../../migrations")]
async fn keys_checkout_signed_webhook_partial_refund_and_repeat_webhook(pool: PgPool) {
    seed_catalog(&pool).await;
    late_cancel_policy(&pool).await;
    let (base, gateway) = start_gateway().await;
    let dev = common::app(pool.clone());
    let c = customer(&dev, &pool, "010-7100-0100").await;
    let prod = prod_app(pool.clone(), &base);
    let (owner, _) = staff_token(&pool, Role::StaffAdmin).await;

    // 1. The owner saves the PortOne keys and webhook secret; the live providers pick them up.
    let webhook_secret = format!("whsec_{}", STANDARD.encode(WEBHOOK_KEY));
    let keys = json!({
        "store_id": STORE,
        "api_secret": E2E_SECRET,
        "channel_keys": { "card": "channel-key-e2e-card" },
        "webhook_secret": webhook_secret,
    });
    let (st, saved) = call(&prod, "PUT", "/v1/admin/setup/portone", Some(owner.as_str()), Some(keys)).await;
    assert_eq!(st, StatusCode::OK, "{saved}");

    // 2. Checkout for a visit two days out, paid at PortOne, then confirmed with the PortOne payment id.
    let visit = api::util::today_kst() + chrono::Duration::days(2);
    let body = json!({ "kind": "booking", "method": "card",
        "booking": { "service_code": "studio", "address_id": c.address, "visit_date": visit, "start_hour": 9, "visit_notes": {} } });
    let (st, intent) = call(&prod, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{intent}");
    let checkout: Uuid = intent["id"].as_str().expect("checkout id").parse().expect("uuid");
    let amount: i64 = sqlx::query_scalar("SELECT amount FROM checkouts WHERE id = $1")
        .bind(checkout)
        .fetch_one(&pool)
        .await
        .expect("amount");
    gateway.lock().expect("gateway").payments.insert(checkout.to_string(), (amount, 0));
    let (st, done) = call(
        &prod,
        "POST",
        &format!("/v1/checkouts/{checkout}/confirm"),
        Some(c.token.as_str()),
        Some(json!({ "payment_id": checkout.to_string() })),
    )
    .await;
    assert_eq!((st, done["status"].as_str()), (StatusCode::OK, Some("paid")), "{done}");
    let booking: Uuid = done["booking_id"].as_str().expect("booking id").parse().expect("uuid");

    // 3. Signed Transaction.Paid for the settled checkout: acknowledged, nothing changes.
    assert_eq!(signed_webhook(&prod, "Transaction.Paid", &checkout.to_string()).await, StatusCode::OK);

    // 4. Late cancel: PortOne receives a partial refund of the amount after the fee.
    let tok = Some(c.token.as_str());
    let (_, preview) = call(&prod, "GET", &format!("/v1/bookings/{booking}/cancel-preview"), tok, None).await;
    let fee = preview["fee"].as_i64().expect("fee");
    assert!(fee > 0, "a two-day-out cancel keeps a fee: {preview}");
    let (st, cancelled) = call(
        &prod,
        "POST",
        &format!("/v1/bookings/{booking}/cancel"),
        tok,
        Some(json!({ "accepted_fee": fee })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{cancelled}");
    let refund = amount - fee;
    let sent = gateway.lock().expect("gateway").cancels.clone();
    assert_eq!(sent.len(), 1, "one provider cancel: {sent:?}");
    assert_eq!(sent[0]["amount"].as_i64(), Some(refund), "the refund is the amount after the fee");
    let refunded: i64 = sqlx::query_scalar("SELECT refunded_amount FROM payments WHERE booking_id = $1")
        .bind(booking)
        .fetch_one(&pool)
        .await
        .expect("refunded");
    assert_eq!(refunded, refund, "our ledger records the same partial refund");

    // 5. PortOne reports that partial cancel back, signed. The refund is not booked twice.
    assert_eq!(
        signed_webhook(&prod, "Transaction.PartialCancelled", &checkout.to_string()).await,
        StatusCode::OK
    );
    let again: i64 = sqlx::query_scalar("SELECT refunded_amount FROM payments WHERE booking_id = $1")
        .bind(booking)
        .fetch_one(&pool)
        .await
        .expect("refunded");
    assert_eq!(again, refund, "the PartialCancelled webhook moves no money");
    assert_eq!(gateway.lock().expect("gateway").cancels.len(), 1, "no second provider cancel");
}
