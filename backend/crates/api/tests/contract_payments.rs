// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The payment contract: one suite run against the PortOne and Toss Payments shims (production mode, mock gateways)
//! and against their in-memory fakes. Confirm then the core gate (a wrong amount or currency never settles), lookups,
//! partial then full refunds with a duplicate retry, a stale balance and an over-refund refused, signed and forged
//! webhooks, and saved cards (issue, charge once, delete). The shims must answer exactly what `Providers` answers.
//! The shim runs need DATABASE_URL (sqlx::test); the fake runs need nothing.

mod common;

use api::{
    adapters::{
        self,
        fake::{FakeGateway, Flavour},
        payments::{Charge, Confirm, CustomerRef, HintKind, PayStatus, PaymentGateway, RefundReq, WebhookError},
    },
    config::PortOne,
    crypto,
    providers::{ProviderError, Providers, TOSS, TOSS_BILLING, portone::BALANCE_MOVED},
    routes::checkouts::{CheckoutRow, amount_matches},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use common::{PORTONE_SECRET, TossEnv, prod_app_gateways, prod_app_toss, toss};
use domain::money::Money;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::PgPool;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const KRW: &str = "KRW";
const PORTONE_WEBHOOK_KEY: &[u8] = b"contract-portone-webhook-key-0001";
const FAKE_SECRET: &str = "fake-webhook-secret";
const FAKE_DATA_KEY: [u8; 32] = [42; 32];

/// What the suite needs from a backend besides the gateway: the customer's side and signed deliveries.
trait World {
    /// The gateway checkouts are pinned to.
    fn gw(&self) -> Arc<dyn PaymentGateway>;
    /// The gateway saved-card charges are looked up on (Toss: the billing MID's code).
    fn billing(&self) -> Arc<dyn PaymentGateway>;
    /// The customer pays `amount` in the window; returns what the return URL carries.
    async fn customer_pays(&self, checkout: &str, amount: i64) -> String;
    /// The card window issued a card for `who`; returns its result (authKey or billing key).
    fn card_window(&self, who: &CustomerRef) -> String;
    /// A payment delivery about `tx`, signed, or with a forged signature.
    fn delivery(&self, tx: &str, forge: bool) -> (HeaderMap, Vec<u8>);
    fn data_key(&self) -> [u8; 32];
}

fn row(id: &str, amount: i64, currency: &str) -> CheckoutRow {
    CheckoutRow {
        id: Uuid::parse_str(id).expect("checkout id"),
        user_id: Uuid::nil(),
        kind: "booking".into(),
        booking_id: None,
        draft: None,
        amount,
        points: 0,
        status: "pending".into(),
        provider_payment_id: None,
        result_booking_id: None,
        expires_at: Utc::now(),
        gateway: String::new(),
        currency: currency.into(),
    }
}

fn refund(tx: &str, amount: i64, expected: i64) -> RefundReq {
    RefundReq {
        provider_tx: tx.into(),
        amount: Money(amount),
        expected_cancellable: Some(Money(expected)),
    }
}

async fn suite<W: World>(w: &W) {
    let gw = w.gw();
    assert!(gw.methods().contains(&"card"));
    let ck = Uuid::now_v7().to_string();
    let tx = w.customer_pays(&ck, 50_000).await;
    let confirm = Confirm {
        checkout_id: &ck,
        provider_tx: &tx,
        expected: Money(50_000),
        currency: KRW,
    };
    let info = gw.confirm(confirm).await.expect("confirm");
    assert_eq!((info.pay_status(), info.total, info.currency.as_str()), (PayStatus::Paid, 50_000, KRW));
    assert_eq!(info.cancellable(), Some(Money(50_000)));
    // The core gate, on what this gateway answered.
    assert!(amount_matches(&row(&ck, 50_000, KRW), KRW, &info));
    assert!(!amount_matches(&row(&ck, 49_000, KRW), KRW, &info), "a wrong amount never settles");
    assert!(!amount_matches(&row(&ck, 50_000, "USD"), KRW, &info), "a wrong currency never settles");

    let read = gw.fetch(&ck, Money(50_000)).await.expect("fetch");
    assert_eq!((read.status.as_str(), read.total), ("PAID", 50_000));
    assert!(matches!(gw.fetch("no-such-payment", Money(1)).await, Err(ProviderError::NotFound)));

    refunds(gw.as_ref(), &ck).await;
    webhooks(w, gw.as_ref(), &ck);
    saved_cards(w, gw.as_ref()).await;
}

async fn refunds(gw: &dyn PaymentGateway, ck: &str) {
    let left = |info: api::providers::PaymentInfo| (info.pay_status(), info.cancellable());
    gw.refund(refund(ck, 20_000, 50_000)).await.expect("partial refund");
    let after = left(gw.fetch(ck, Money(50_000)).await.expect("fetch"));
    assert_eq!(after, (PayStatus::PartiallyRefunded, Some(Money(30_000))));
    gw.refund(refund(ck, 20_000, 50_000)).await.expect("a retry of a refund that landed is done");
    assert_eq!(left(gw.fetch(ck, Money(50_000)).await.expect("fetch")), after, "never refunded twice");
    let stale = gw.refund(refund(ck, 10_000, 45_000)).await.err();
    assert!(matches!(&stale, Some(ProviderError::Declined(c)) if c == BALANCE_MOVED), "{stale:?}");
    let over = gw.refund(refund(ck, 40_000, 30_000)).await.err();
    assert!(matches!(over, Some(ProviderError::Declined(_))), "{over:?}");
    assert_eq!(left(gw.fetch(ck, Money(50_000)).await.expect("fetch")), after, "refusals change nothing");
    gw.refund(refund(ck, 30_000, 30_000)).await.expect("full refund");
    assert_eq!(left(gw.fetch(ck, Money(50_000)).await.expect("fetch")), (PayStatus::Refunded, Some(Money(0))));
}

fn webhooks<W: World>(w: &W, gw: &dyn PaymentGateway, ck: &str) {
    let (h, body) = w.delivery(ck, false);
    let hints = gw.verify_webhook(&h, &body, Utc::now()).expect("signed delivery");
    assert_eq!(hints.len(), 1);
    assert_eq!((hints[0].provider_tx.as_str(), hints[0].kind), (ck, HintKind::Payment));
    assert!(hints[0].event_id.is_some(), "a signed delivery enters the ledger");
    let again = gw.verify_webhook(&h, &body, Utc::now()).expect("replay verifies");
    assert_eq!(again[0].event_id, hints[0].event_id, "a replay has the same ledger key");
    let (h, body) = w.delivery(ck, true);
    assert_eq!(gw.verify_webhook(&h, &body, Utc::now()), Err(WebhookError::BadSignature));
}

async fn saved_cards<W: World>(w: &W, gw: &dyn PaymentGateway) {
    let cards = gw.saved_cards().expect("saves cards");
    let customer = Uuid::now_v7();
    let who = CustomerRef {
        id: customer.to_string(),
        issue: Uuid::now_v7().to_string(),
    };
    let auth = w.card_window(&who);
    let key = cards.issue(&who, &auth).await.expect("issue");
    let (sealed, _) = crypto::seal_billing_key(&w.data_key(), &key);
    let pid = format!("charge-{}", Uuid::now_v7().simple());
    let charge = || Charge {
        sealed_key: &sealed,
        customer_key: Some(&who.issue),
        amount: Money(30_000),
        currency: KRW,
        payment_id: &pid,
        order_name: "contract",
        customer,
    };
    assert_eq!(cards.charge(charge()).await.expect("charge"), pid);
    let paid = w.billing().fetch(&pid, Money(30_000)).await.expect("charged payment");
    assert_eq!((paid.pay_status(), paid.total), (PayStatus::Paid, 30_000));
    assert!(cards.charge(charge()).await.is_err(), "one payment id never charges twice");
    cards.delete(&key).await.expect("delete");
}

// ---- fakes ----

struct FakeWorld(Arc<FakeGateway>);

impl FakeWorld {
    fn new(flavour: Flavour) -> Self {
        Self(Arc::new(FakeGateway::new(flavour, true, FAKE_DATA_KEY, FAKE_SECRET).expect("dev fake")))
    }
}

impl World for FakeWorld {
    fn gw(&self) -> Arc<dyn PaymentGateway> {
        self.0.clone()
    }
    fn billing(&self) -> Arc<dyn PaymentGateway> {
        self.0.clone()
    }
    async fn customer_pays(&self, checkout: &str, amount: i64) -> String {
        self.0.customer_pays(checkout, amount, KRW)
    }
    fn card_window(&self, who: &CustomerRef) -> String {
        self.0.card_window(who)
    }
    fn delivery(&self, tx: &str, forge: bool) -> (HeaderMap, Vec<u8>) {
        self.0.delivery(tx, HintKind::Payment, forge)
    }
    fn data_key(&self) -> [u8; 32] {
        FAKE_DATA_KEY
    }
}

#[tokio::test]
async fn the_portone_fake_keeps_the_contract() {
    suite(&FakeWorld::new(Flavour::PortOne)).await;
}

#[tokio::test]
async fn the_toss_fake_keeps_the_contract() {
    suite(&FakeWorld::new(Flavour::Toss)).await;
}

#[tokio::test]
async fn the_toss_fake_captures_only_the_authorised_amount() {
    let w = FakeWorld::new(Flavour::Toss);
    let ck = Uuid::now_v7().to_string();
    let tx = w.customer_pays(&ck, 50_000).await;
    let wrong = Confirm {
        checkout_id: &ck,
        provider_tx: &tx,
        expected: Money(5_000),
        currency: KRW,
    };
    assert!(matches!(w.0.confirm(wrong).await, Err(ProviderError::Declined(_))));
    assert!(matches!(w.0.fetch(&ck, Money(5_000)).await, Err(ProviderError::NotFound)), "nothing captured");
}

// ---- PortOne shim against a PortOne mock ----

#[derive(Default)]
struct PortOneMock {
    /// payment id -> (total, cancelled)
    payments: HashMap<String, (i64, i64)>,
    /// billing key -> (customer id, issue id)
    keys: HashMap<String, (String, String)>,
}

type PoShared = Arc<Mutex<PortOneMock>>;
type Reply = (StatusCode, Json<Value>);

fn po_authorized(h: &HeaderMap) -> Option<Reply> {
    let ok = h.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(format!("PortOne {PORTONE_SECRET}").as_str());
    (!ok).then(|| (StatusCode::UNAUTHORIZED, Json(json!({ "type": "UnauthorizedError" }))))
}

fn po_err(status: StatusCode, kind: &str) -> Reply {
    (status, Json(json!({ "type": kind })))
}

async fn po_payment(State(g): State<PoShared>, Path(id): Path<String>, h: HeaderMap) -> Reply {
    if let Some(r) = po_authorized(&h) {
        return r;
    }
    let Some(&(total, cancelled)) = g.lock().expect("mock").payments.get(&id) else {
        return po_err(StatusCode::NOT_FOUND, "PaymentNotFoundError");
    };
    let status = match cancelled {
        0 => "PAID",
        c if c == total => "CANCELLED",
        _ => "PARTIAL_CANCELLED",
    };
    let body = json!({ "status": status, "amount": { "total": total, "cancelled": cancelled }, "currency": KRW, "method": { "type": "PaymentMethodCard" } });
    (StatusCode::OK, Json(body))
}

async fn po_cancel(State(g): State<PoShared>, Path(id): Path<String>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    if let Some(r) = po_authorized(&h) {
        return r;
    }
    let mut g = g.lock().expect("mock");
    let Some(entry) = g.payments.get_mut(&id) else {
        return po_err(StatusCode::NOT_FOUND, "PaymentNotFoundError");
    };
    let left = entry.0 - entry.1;
    if b["currentCancellableAmount"].as_i64().is_some_and(|c| c != left) {
        return po_err(StatusCode::CONFLICT, "CancellableAmountConsistencyBrokenError");
    }
    let amount = b["amount"].as_i64().unwrap_or(left);
    if amount > left {
        return po_err(StatusCode::CONFLICT, "CancelAmountExceedsCancellableAmountError");
    }
    entry.1 += amount;
    (
        StatusCode::OK,
        Json(json!({ "cancellation": { "status": "SUCCEEDED", "totalAmount": amount } })),
    )
}

async fn po_charge(State(g): State<PoShared>, Path(id): Path<String>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    if let Some(r) = po_authorized(&h) {
        return r;
    }
    let mut g = g.lock().expect("mock");
    if !g.keys.contains_key(b["billingKey"].as_str().unwrap_or_default()) {
        return po_err(StatusCode::BAD_REQUEST, "BillingKeyNotFoundError");
    }
    if g.payments.contains_key(&id) {
        return po_err(StatusCode::CONFLICT, "AlreadyPaidError");
    }
    g.payments.insert(id, (b["amount"]["total"].as_i64().unwrap_or(0), 0));
    (
        StatusCode::OK,
        Json(json!({ "payment": { "pgTxId": "pg-tx", "paidAt": "2026-11-28T00:00:00Z" } })),
    )
}

async fn po_key(State(g): State<PoShared>, Path(key): Path<String>, h: HeaderMap) -> Reply {
    if let Some(r) = po_authorized(&h) {
        return r;
    }
    match g.lock().expect("mock").keys.get(&key) {
        Some((customer, issue)) => (
            StatusCode::OK,
            Json(json!({ "status": "ISSUED", "customer": { "id": customer }, "issueId": issue, "channels": [] })),
        ),
        None => po_err(StatusCode::NOT_FOUND, "BillingKeyNotFoundError"),
    }
}

async fn po_delete_key(State(g): State<PoShared>, Path(key): Path<String>, h: HeaderMap) -> Reply {
    if let Some(r) = po_authorized(&h) {
        return r;
    }
    match g.lock().expect("mock").keys.remove(&key) {
        Some(_) => (StatusCode::OK, Json(json!({}))),
        None => po_err(StatusCode::NOT_FOUND, "BillingKeyNotFoundError"),
    }
}

async fn start_portone() -> (String, PoShared) {
    let g = PoShared::default();
    let app = Router::new()
        .route("/payments/{id}", get(po_payment))
        .route("/payments/{id}/cancel", post(po_cancel))
        .route("/payments/{id}/billing-key", post(po_charge))
        .route("/billing-keys/{key}", get(po_key).delete(po_delete_key))
        .with_state(g.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind portone mock");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("mock serves") });
    (format!("http://{addr}"), g)
}

struct PortOneWorld {
    env: TossEnv,
    mock: PoShared,
}

impl PortOneWorld {
    async fn new(pool: PgPool) -> Self {
        let (base, mock) = start_portone().await;
        let portone = PortOne {
            api_secret: Some(PORTONE_SECRET.into()),
            webhook_secret: Some(format!("whsec_{}", STANDARD.encode(PORTONE_WEBHOOK_KEY))),
            store_id: Some("store-test".into()),
            channel_keys: [("card".to_string(), "channel-key-card".to_string())].into(),
            cash_receipts: false,
        };
        Self {
            env: prod_app_gateways(pool, &base, &base, portone).await,
            mock,
        }
    }

    fn providers(&self) -> &Providers {
        &self.env.state.providers
    }
}

impl World for PortOneWorld {
    fn gw(&self) -> Arc<dyn PaymentGateway> {
        adapters::payment_gateway(self.providers(), "portone").expect("portone shim")
    }
    fn billing(&self) -> Arc<dyn PaymentGateway> {
        self.gw()
    }
    async fn customer_pays(&self, checkout: &str, amount: i64) -> String {
        self.mock.lock().expect("mock").payments.insert(checkout.into(), (amount, 0));
        checkout.into()
    }
    fn card_window(&self, who: &CustomerRef) -> String {
        let key = format!("bk-{}", who.issue);
        self.mock.lock().expect("mock").keys.insert(key.clone(), (who.id.clone(), who.issue.clone()));
        key
    }
    fn delivery(&self, tx: &str, forge: bool) -> (HeaderMap, Vec<u8>) {
        let body = json!({ "type": "Transaction.Paid", "data": { "paymentId": tx } }).to_string().into_bytes();
        let (id, ts) = (format!("msg-{tx}"), Utc::now().timestamp().to_string());
        let key: &[u8] = if forge { b"forged" } else { PORTONE_WEBHOOK_KEY };
        let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("key");
        mac.update(format!("{id}.{ts}.").as_bytes());
        mac.update(&body);
        let sig = format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()));
        let mut h = HeaderMap::new();
        for (name, value) in [("webhook-id", id), ("webhook-timestamp", ts), ("webhook-signature", sig)] {
            h.insert(name, value.parse().expect("header"));
        }
        (h, body)
    }
    fn data_key(&self) -> [u8; 32] {
        self.env.state.config.data_key
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_portone_shim_keeps_the_contract(pool: PgPool) {
    suite(&PortOneWorld::new(pool).await).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_portone_shim_answers_what_providers_answers(pool: PgPool) {
    let w = PortOneWorld::new(pool).await;
    let ck = Uuid::now_v7().to_string();
    w.customer_pays(&ck, 12_000).await;
    let via_shim = w.gw().fetch(&ck, Money(12_000)).await.expect("shim");
    let direct = w.providers().payment("portone", &ck, 12_000).await.expect("providers");
    assert_eq!(
        (via_shim.status, via_shim.total, via_shim.cancelled, via_shim.currency, via_shim.method),
        (direct.status, direct.total, direct.cancelled, direct.currency, direct.method)
    );
    let stale_shim = w.gw().refund(refund(&ck, 1_000, 9_000)).await.err().map(|e| e.to_string());
    let stale_direct = w.providers().refund_checked("portone", &ck, 1_000, 9_000).await.err().map(|e| e.to_string());
    assert_eq!(stale_shim, stale_direct);
    // The SDK parameters POST /checkouts hands the app for PortOne.
    let start = w.gw().start(checkout_req(&ck, "card")).await.expect("start");
    let params = json!({ "store_id": "store-test", "channel_key": "channel-key-card", "cash_receipts": false });
    assert_eq!(start, adapters::payments::CheckoutStart::Sdk { params });
    let unserved = w.gw().start(checkout_req(&ck, "phone")).await.err();
    assert!(matches!(unserved, Some(ProviderError::NotConfigured(_))), "a method without a channel key");
}

#[sqlx::test(migrations = "../../migrations")]
async fn portone_webhooks_without_a_secret_are_refused_in_production(pool: PgPool) {
    let (base, _mock) = start_portone().await;
    let env = prod_app_gateways(pool, &base, &base, PortOne::default()).await;
    let gw = adapters::payment_gateway(&env.state.providers, "portone").expect("shim");
    let body = br#"{"type":"Transaction.Paid","data":{"paymentId":"p"}}"#;
    assert_eq!(gw.verify_webhook(&HeaderMap::new(), body, Utc::now()), Err(WebhookError::NotConfigured));
}

fn checkout_req(id: &str, method: &str) -> adapters::payments::CheckoutReq {
    adapters::payments::CheckoutReq {
        checkout_id: Uuid::parse_str(id).expect("id"),
        amount: Money(1_000),
        currency: KRW.into(),
        method: method.into(),
    }
}

// ---- Toss Payments shim against the Toss mock ----

struct TossWorld {
    env: TossEnv,
}

impl TossWorld {
    async fn new(pool: PgPool) -> Self {
        let (toss_api, _) = toss::start().await;
        // PortOne is never called by these tests.
        Self {
            env: prod_app_toss(pool, &toss_api, "http://127.0.0.1:9", true).await,
        }
    }

    fn providers(&self) -> &Providers {
        &self.env.state.providers
    }
}

impl World for TossWorld {
    fn gw(&self) -> Arc<dyn PaymentGateway> {
        adapters::payment_gateway(self.providers(), TOSS).expect("toss shim")
    }
    fn billing(&self) -> Arc<dyn PaymentGateway> {
        adapters::payment_gateway(self.providers(), TOSS_BILLING).expect("toss billing shim")
    }
    /// The window only authorises: the mock creates the payment on confirm.
    async fn customer_pays(&self, checkout: &str, _amount: i64) -> String {
        format!("pk_{checkout}")
    }
    fn card_window(&self, who: &CustomerRef) -> String {
        format!("auth-{}", who.issue)
    }
    fn delivery(&self, tx: &str, forge: bool) -> (HeaderMap, Vec<u8>) {
        let body = json!({ "eventType": "PAYMENT_STATUS_CHANGED", "data": { "orderId": tx, "status": "DONE" } })
            .to_string()
            .into_bytes();
        let time = Utc::now().to_rfc3339();
        let key = if forge { "forged" } else { toss::SECURITY_KEY };
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("key");
        mac.update(&body);
        mac.update(b":");
        mac.update(time.as_bytes());
        let sig = format!("v1:{}", STANDARD.encode(mac.finalize().into_bytes()));
        let mut h = HeaderMap::new();
        h.insert(api::providers::toss_webhook::SIGNATURE_HEADER, sig.parse().expect("header"));
        h.insert(api::providers::toss_webhook::TRANSMISSION_TIME_HEADER, time.parse().expect("header"));
        (h, body)
    }
    fn data_key(&self) -> [u8; 32] {
        self.env.state.config.data_key
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_toss_shim_keeps_the_contract(pool: PgPool) {
    suite(&TossWorld::new(pool).await).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_toss_shim_answers_what_providers_answers(pool: PgPool) {
    let w = TossWorld::new(pool).await;
    let ck = Uuid::now_v7().to_string();
    let confirm = Confirm {
        checkout_id: &ck,
        provider_tx: &format!("pk_{ck}"),
        expected: Money(8_000),
        currency: KRW,
    };
    w.gw().confirm(confirm).await.expect("confirm");
    let via_shim = w.gw().fetch(&ck, Money(8_000)).await.expect("shim");
    let direct = w.providers().payment(TOSS, &ck, 8_000).await.expect("providers");
    assert_eq!(
        (via_shim.status, via_shim.total, via_shim.cancelled, via_shim.currency, via_shim.method),
        (direct.status, direct.total, direct.cancelled, direct.currency, direct.method)
    );
    let stale_shim = w.gw().refund(refund(&ck, 1_000, 9_000)).await.err().map(|e| e.to_string());
    let stale_direct = w.providers().refund_checked(TOSS, &ck, 1_000, 9_000).await.err().map(|e| e.to_string());
    assert_eq!(stale_shim, stale_direct);
    let start = w.gw().start(checkout_req(&ck, "kakaopay")).await.expect("start");
    let params = json!({ "store_id": null, "channel_key": toss::WINDOW_CK, "cash_receipts": false });
    assert_eq!(start, adapters::payments::CheckoutStart::Sdk { params });
}

#[sqlx::test(migrations = "../../migrations")]
async fn unsigned_toss_events_are_hints_outside_the_ledger(pool: PgPool) {
    let w = TossWorld::new(pool).await;
    let body = br#"{"eventType":"PAYMENT_STATUS_CHANGED","data":{"orderId":"ord-1","status":"PARTIAL_CANCELED"}}"#;
    let hints = w.gw().verify_webhook(&HeaderMap::new(), body, Utc::now()).expect("hint");
    assert_eq!(hints[0].kind, HintKind::Cancelled { partial: true });
    assert_eq!(hints[0].event_id, None);
    let other = br#"{"eventType":"SELLER_CHANGED","data":{}}"#;
    assert!(w.gw().verify_webhook(&HeaderMap::new(), other, Utc::now()).expect("ignored").is_empty());
}

// ---- the registry's answers for codes outside the two shims ----

fn dev_providers(dev_mode: bool) -> Providers {
    let mut cfg = common::test_config();
    cfg.dev_mode = dev_mode;
    let db = PgPool::connect_lazy("postgres://unused@127.0.0.1:1/unused").expect("lazy pool");
    Providers::new(cfg, db).expect("http client")
}

#[tokio::test]
async fn unknown_codes_fail_closed() {
    let p = dev_providers(true);
    for code in ["stripe", "", "solapi", "manual"] {
        assert!(
            matches!(adapters::payment_gateway(&p, code).err(), Some(ProviderError::NotConfigured(_))),
            "{code}"
        );
    }
}

#[tokio::test]
async fn the_legacy_pg_code_is_served_as_before() {
    let prod = dev_providers(false);
    let gw = adapters::payment_gateway(&prod, "pg").expect("pg");
    let refused = gw.refund(refund("tx", 1, 1)).await.err().map(|e| e.to_string());
    let before = prod.refund_checked("pg", "tx", 1, 1).await.err().map(|e| e.to_string());
    assert_eq!(refused, before);
    assert!(matches!(refused.as_deref(), Some(m) if m.contains("not configured")));
    let dev = dev_providers(true);
    let gw = adapters::payment_gateway(&dev, "pg").expect("pg");
    assert!(gw.refund(refund("tx", 1, 1)).await.is_ok(), "the dev fake refunds");
}

#[tokio::test]
async fn dev_mode_shims_answer_with_todays_dev_fake() {
    let p = dev_providers(true);
    for code in ["portone", TOSS] {
        let gw = adapters::payment_gateway(&p, code).expect("shim");
        let va = gw.fetch("dev:va", Money(33_000)).await.expect("dev payment");
        assert!(matches!(va.pay_status(), PayStatus::PendingOffline { .. }), "{code}");
        assert_eq!(va.total, 33_000);
    }
    let toss = adapters::payment_gateway(&p, TOSS).expect("shim");
    let declined = Confirm {
        checkout_id: "ck",
        provider_tx: "dev:failed",
        expected: Money(1),
        currency: KRW,
    };
    assert!(matches!(toss.confirm(declined).await, Err(ProviderError::Declined(_))));
}
