// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! A fake Toss Payments API on 127.0.0.1:0 for the gateway tests: confirm, lookup by orderId, cancel (with the
//! `refundableAmount` and balance rules), billing key issue / charge / delete, and the transaction list. Every call
//! is recorded with its Authorization header, so a test can tell which MID's secret was used. A test can queue
//! one-shot replies per call kind (`script`) to force an error or a lost answer.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

pub const WINDOW_SK: &str = "test_sk_window0001";
pub const WINDOW_CK: &str = "test_ck_window0001";
pub const BILLING_SK: &str = "test_sk_billing0001";
pub const BILLING_CK: &str = "test_ck_billing0001";
pub const SECURITY_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// One request the fake saw.
#[derive(Clone, Debug)]
pub struct Call {
    /// `confirm`, `order`, `cancel`, `issue`, `charge`, `delete` or `transactions`.
    pub kind: &'static str,
    pub path: String,
    /// The secret key the Basic header carried.
    pub secret: String,
    pub body: Value,
}

#[derive(Default)]
pub struct Fake {
    /// orderId -> Payment object (paymentKey, status, totalAmount, balanceAmount, currency, ...).
    pub orders: HashMap<String, Value>,
    /// orderId -> fields merged into the Payment a confirm creates (e.g. a virtual account).
    pub confirm_as: HashMap<String, Value>,
    /// billingKey -> customerKey.
    pub billing_keys: HashMap<String, String>,
    /// What `GET /v1/transactions` returns on the window MID, and on the billing MID.
    pub transactions: Vec<Value>,
    pub billing_transactions: Vec<Value>,
    /// The next charge goes through but its answer is a 500, like a reply lost on the way back.
    pub lose_next_charge_answer: bool,
    /// One-shot replies by call kind, used before the default behaviour.
    pub script: HashMap<&'static str, VecDeque<(u16, Value)>>,
    pub calls: Vec<Call>,
}

pub type Shared = Arc<Mutex<Fake>>;
type Reply = (StatusCode, Json<Value>);

impl Fake {
    pub fn calls_of(&self, kind: &str) -> Vec<Call> {
        self.calls.iter().filter(|c| c.kind == kind).cloned().collect()
    }

    /// Queues one reply for the next call of `kind`.
    pub fn push(&mut self, kind: &'static str, status: u16, body: Value) {
        self.script.entry(kind).or_default().push_back((status, body));
    }

    /// A DONE payment of `amount` for `order`, as if confirmed or charged.
    pub fn paid(&mut self, order: &str, amount: i64) {
        self.orders.insert(order.to_string(), payment(order, &format!("pk_{order}"), amount));
    }
}

pub fn payment(order: &str, key: &str, amount: i64) -> Value {
    json!({ "orderId": order, "paymentKey": key, "status": "DONE", "totalAmount": amount, "balanceAmount": amount,
            "currency": "KRW", "method": "카드", "card": { "amount": amount } })
}

fn reply(status: u16, body: Value) -> Reply {
    (StatusCode::from_u16(status).expect("status"), Json(body))
}

fn err(status: u16, code: &str) -> Reply {
    reply(status, json!({ "code": code, "message": code }))
}

fn secret_of(h: &HeaderMap) -> String {
    let raw = h.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or_default();
    let decoded = raw.strip_prefix("Basic ").and_then(|b| STANDARD.decode(b).ok()).unwrap_or_default();
    String::from_utf8(decoded).unwrap_or_default().trim_end_matches(':').to_string()
}

/// Records the call; returns a scripted reply, or a 401 for an unknown secret.
fn enter(f: &mut Fake, kind: &'static str, path: String, h: &HeaderMap, body: Value) -> Option<Reply> {
    let secret = secret_of(h);
    f.calls.push(Call {
        kind,
        path,
        secret: secret.clone(),
        body,
    });
    if let Some((st, b)) = f.script.get_mut(kind).and_then(VecDeque::pop_front) {
        return Some(reply(st, b));
    }
    (secret != WINDOW_SK && secret != BILLING_SK).then(|| err(401, "UNAUTHORIZED_KEY"))
}

async fn confirm(State(g): State<Shared>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    let mut f = g.lock().expect("fake");
    if let Some(r) = enter(&mut f, "confirm", "/v1/payments/confirm".into(), &h, b.clone()) {
        return r;
    }
    let order = b["orderId"].as_str().unwrap_or_default().to_string();
    if f.orders.contains_key(&order) {
        return err(400, "ALREADY_PROCESSED_PAYMENT");
    }
    let mut p = payment(&order, b["paymentKey"].as_str().unwrap_or_default(), b["amount"].as_i64().unwrap_or(0));
    if let Some(extra) = f.confirm_as.get(&order).and_then(Value::as_object) {
        for (k, v) in extra {
            p[k] = v.clone();
        }
    }
    f.orders.insert(order, p.clone());
    reply(200, p)
}

async fn order(State(g): State<Shared>, Path(id): Path<String>, h: HeaderMap) -> Reply {
    let mut f = g.lock().expect("fake");
    if let Some(r) = enter(&mut f, "order", format!("/v1/payments/orders/{id}"), &h, Value::Null) {
        return r;
    }
    match f.orders.get(&id) {
        Some(p) => reply(200, p.clone()),
        None => err(404, "NOT_FOUND_PAYMENT"),
    }
}

async fn cancel(State(g): State<Shared>, Path(key): Path<String>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    let mut f = g.lock().expect("fake");
    if let Some(r) = enter(&mut f, "cancel", format!("/v1/payments/{key}/cancel"), &h, b.clone()) {
        return r;
    }
    let Some(p) = f.orders.values_mut().find(|p| p["paymentKey"].as_str() == Some(key.as_str())) else {
        return err(404, "NOT_FOUND_PAYMENT");
    };
    let left = p["balanceAmount"].as_i64().unwrap_or(0);
    let amount = b["cancelAmount"].as_i64().unwrap_or(left);
    if b["refundableAmount"].as_i64().is_some_and(|r| r != left) {
        return err(400, "NOT_MATCHES_REFUNDABLE_AMOUNT");
    }
    if left == 0 {
        return err(400, "ALREADY_CANCELED_PAYMENT");
    }
    if amount > left {
        return err(403, "NOT_CANCELABLE_AMOUNT");
    }
    p["balanceAmount"] = json!(left - amount);
    p["status"] = json!(if left == amount { "CANCELED" } else { "PARTIAL_CANCELED" });
    reply(200, p.clone())
}

async fn issue(State(g): State<Shared>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    let mut f = g.lock().expect("fake");
    if let Some(r) = enter(&mut f, "issue", "/v1/billing/authorizations/issue".into(), &h, b.clone()) {
        return r;
    }
    let customer = b["customerKey"].as_str().unwrap_or_default().to_string();
    let key = format!("bk_{}", b["authKey"].as_str().unwrap_or_default());
    f.billing_keys.insert(key.clone(), customer.clone());
    reply(200, json!({ "mId": "tbilling", "customerKey": customer, "billingKey": key, "method": "카드" }))
}

async fn charge(State(g): State<Shared>, Path(key): Path<String>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    let mut f = g.lock().expect("fake");
    if let Some(r) = enter(&mut f, "charge", format!("/v1/billing/{key}"), &h, b.clone()) {
        return r;
    }
    if f.billing_keys.get(&key).map(String::as_str) != b["customerKey"].as_str() {
        return err(400, "NOT_MATCHES_CUSTOMER_KEY");
    }
    let order = b["orderId"].as_str().unwrap_or_default().to_string();
    if f.orders.contains_key(&order) {
        return err(400, "DUPLICATED_ORDER_ID");
    }
    let p = payment(&order, &format!("pk_{order}"), b["amount"].as_i64().unwrap_or(0));
    f.orders.insert(order, p.clone());
    if std::mem::take(&mut f.lose_next_charge_answer) {
        return err(500, "FAILED_INTERNAL_SYSTEM_PROCESSING");
    }
    reply(200, p)
}

async fn delete(State(g): State<Shared>, Path(key): Path<String>, h: HeaderMap) -> Reply {
    let mut f = g.lock().expect("fake");
    if let Some(r) = enter(&mut f, "delete", format!("/v1/billing/{key}"), &h, Value::Null) {
        return r;
    }
    f.billing_keys.remove(&key);
    reply(200, json!({}))
}

async fn transactions(State(g): State<Shared>, h: HeaderMap, axum::extract::RawQuery(q): axum::extract::RawQuery) -> Reply {
    let mut f = g.lock().expect("fake");
    if let Some(r) = enter(&mut f, "transactions", format!("/v1/transactions?{}", q.unwrap_or_default()), &h, Value::Null) {
        return r;
    }
    let billing = secret_of(&h) == BILLING_SK;
    reply(200, json!(if billing { &f.billing_transactions } else { &f.transactions }))
}

/// Starts the fake. Returns its base URL (for `Providers::with_toss_api`) and its shared state.
pub async fn start() -> (String, Shared) {
    let g = Shared::default();
    let app = Router::new()
        .route("/v1/payments/confirm", post(confirm))
        .route("/v1/payments/orders/{id}", get(order))
        .route("/v1/payments/{key}/cancel", post(cancel))
        .route("/v1/billing/authorizations/issue", post(issue))
        .route("/v1/billing/{key}", post(charge).delete(delete))
        .route("/v1/transactions", get(transactions))
        .with_state(g.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind toss fake");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("fake serves") });
    (format!("http://{addr}"), g)
}

/// The Toss card values: both MIDs, the 보안 키, and the switch.
pub fn card(active: bool) -> serde_json::Map<String, Value> {
    json!({
        "client_key": WINDOW_CK, "secret_key": WINDOW_SK,
        "billing_client_key": BILLING_CK, "billing_secret_key": BILLING_SK,
        "security_key": SECURITY_KEY, "active": active,
    })
    .as_object()
    .cloned()
    .expect("object")
}
