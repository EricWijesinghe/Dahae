// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The contracts of the six ports besides payments (`contract_payments.rs`): one suite per trait, run against each
//! KR shim and against its in-memory fake. The shims must also answer exactly what today's code answers when called
//! directly (`Providers`, `crate::address`, `providers::portone_cash_receipt`), in dev mode, in production without
//! keys, and against mock providers. None of these tests need a database.

mod common;

use api::{
    adapters::{
        self,
        address::{AddressLookup, AddressQuery},
        fake_ports::{FAKE_TOKEN, FakeAddress, FakeIdentity, FakeInvoices, FakeMessenger, FakePayouts, FakeSocial},
        identity::{IdPurpose, IdentityVerifier, Subject},
        invoice::{DocKind, InvoiceSink, TaxDoc},
        juso::Juso,
        login::SocialVerifier,
        messaging::{ChannelKind, Messenger, OutMsg},
        payouts::{KycStatus, PayeeDraft, PayoutRail, Transfer, TransferStatus},
    },
    address,
    config::PortOne,
    integrations::{Integrations, PayoutMode, PayoutSettings},
    providers::{ProviderError, Providers, portone_cash_receipt as pcr, sms, social},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use domain::money::Money;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

fn providers(dev_mode: bool, portone: PortOne) -> Providers {
    let mut cfg = common::test_config();
    cfg.dev_mode = dev_mode;
    cfg.portone = portone;
    // Nothing here reaches the database: dev mode records no health, and the production calls tested fail or answer
    // before a health record.
    let db = PgPool::connect_lazy("postgres://unused@127.0.0.1:1/unused").expect("lazy pool");
    Providers::new(cfg, db).expect("http client")
}

fn dev() -> Providers {
    providers(true, PortOne::default())
}

fn prod() -> Providers {
    providers(false, PortOne::default())
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    base
}

fn err<T>(r: Result<T, ProviderError>) -> Option<String> {
    r.err().map(|e| e.to_string())
}

// ---- messaging ----

fn msg(channel: ChannelKind, template_key: &str) -> OutMsg {
    let vars: BTreeMap<String, String> = [("brand", "Dahae"), ("title", "Booked"), ("body", "See you Monday")]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .into();
    OutMsg {
        to_e164: "+821012345678".into(),
        channel,
        template_key: template_key.into(),
        provider_template_id: None,
        vars,
        fallback_text: "Booked\nSee you Monday".into(),
    }
}

async fn messaging_suite(m: &dyn Messenger) {
    let offered: Vec<ChannelKind> = m.channels().iter().map(|c| c.kind).collect();
    assert!(offered.contains(&ChannelKind::Sms));
    m.send(msg(ChannelKind::Sms, "")).await.expect("sms");
    if offered.contains(&ChannelKind::AlimTalk) {
        m.send(msg(ChannelKind::AlimTalk, sms::TPL_CONFIRMED)).await.expect("alimtalk");
    }
    let missing = [ChannelKind::WhatsApp, ChannelKind::Line, ChannelKind::Viber, ChannelKind::Email]
        .into_iter()
        .find(|k| !offered.contains(k))
        .expect("a channel it does not offer");
    assert!(matches!(m.send(msg(missing, "x")).await, Err(ProviderError::NotConfigured(_))));
    m.balance().await.expect("balance");
}

#[tokio::test]
async fn the_solapi_shim_and_the_fake_messenger_keep_the_contract() {
    let p = dev();
    messaging_suite(adapters::messenger(&p, "solapi").expect("shim").as_ref()).await;
    let fake = FakeMessenger::new(true).expect("dev");
    messaging_suite(&fake).await;
    assert_eq!(fake.sent().len(), 2);
    assert_eq!(fake.sent()[1].template_key, sms::TPL_CONFIRMED);
    assert!(matches!(fake.send(msg(ChannelKind::AlimTalk, "")).await, Err(ProviderError::Declined(m)) if m == "template_required"));
    let mut national = msg(ChannelKind::Sms, "");
    national.to_e164 = "01012345678".into();
    assert!(matches!(fake.send(national).await, Err(ProviderError::Declined(m)) if m == "invalid_number"));
}

#[tokio::test]
async fn the_solapi_shim_answers_what_providers_answers_without_keys() {
    let p = prod();
    let shim = adapters::messenger(&p, "solapi").expect("shim");
    assert_eq!(
        err(shim.send(msg(ChannelKind::Sms, "")).await),
        err(p.send_sms("01012345678", "Booked\nSee you Monday").await)
    );
    assert!(err(shim.send(msg(ChannelKind::Sms, "")).await).is_some_and(|e| e.contains("not configured")));
    // No Solapi card: customer messages are skipped quietly, as today.
    let direct = p
        .send_customer_message("01012345678", sms::TPL_CONFIRMED, "Dahae", "Booked", "See you Monday")
        .await;
    assert!(direct.is_ok());
    assert!(shim.send(msg(ChannelKind::AlimTalk, sms::TPL_CONFIRMED)).await.is_ok());
}

// ---- payouts ----

fn payee() -> PayeeDraft {
    let fields = json!({
        "bank": "국민은행", "account": "110-123-456789", "holder": "홍길동", "business_type": "individual",
        "name": "홍길동", "email": "pro@example.com", "phone": "01012345678"
    });
    PayeeDraft {
        ref_id: "mk0123456789abr1".into(),
        fields: fields.as_object().cloned().expect("object"),
    }
}

fn transfer(payee: &adapters::payouts::PayeeRef, idem: &str) -> Transfer {
    Transfer {
        idem: idem.into(),
        ref_id: "po0123456789ab".into(),
        payee: payee.clone(),
        amount: Money(30_000),
        memo: "정산".into(),
    }
}

/// Register, KYC, send twice with one idempotency key (one transfer), then read the transfer back.
async fn payout_suite(r: &dyn PayoutRail, sent: TransferStatus) {
    let payee = r.register_payee(payee()).await.expect("register");
    assert_eq!(r.kyc_status(&payee).await.expect("kyc"), KycStatus::Approved);
    let id = r.send(transfer(&payee, "idem-1")).await.expect("send");
    assert_eq!(r.send(transfer(&payee, "idem-1")).await.expect("retry"), id, "a retry is the same transfer");
    assert_eq!(r.status(&id).await.expect("status"), sent);
}

#[tokio::test]
async fn the_payout_shims_and_the_fake_rail_keep_the_contract() {
    let p = dev();
    let toss = adapters::payout_rail(&p, "toss").expect("toss shim");
    assert!(!toss.payee_schema().is_empty());
    payout_suite(toss.as_ref(), TransferStatus::Paid).await;
    payout_suite(adapters::payout_rail(&p, "manual").expect("manual").as_ref(), TransferStatus::Pending).await;
    let fake = FakePayouts::new(true).expect("dev");
    payout_suite(&fake, TransferStatus::Paid).await;
    assert_eq!(fake.balance().await.expect("balance"), Money(10_000_000 - 30_000));
    assert!(matches!(fake.status("nope").await, Err(ProviderError::NotFound)));
}

#[tokio::test]
async fn the_toss_payout_shim_answers_what_providers_answers() {
    // Dev mode: the same dev fake answers.
    let p = dev();
    let shim = adapters::payout_rail(&p, "toss").expect("shim");
    let draft = adapters::toss_payouts::seller_draft(&payee()).expect("draft");
    let seller = shim.register_payee(payee()).await.expect("register");
    assert_eq!(seller.0, p.register_seller(&draft).await.expect("direct").0);
    let t = api::providers::payout::Transfer {
        amount: 30_000,
        bank_code: "",
        account: String::new(),
        holder: String::new(),
        memo: "정산".into(),
    };
    assert_eq!(
        shim.send(transfer(&seller, "idem-1")).await.expect("shim"),
        p.send_transfer("idem-1", "po0123456789ab", &seller.0, &t).await.expect("direct")
    );
    // Production without payout keys: the same refusal.
    let p = prod();
    let shim = adapters::payout_rail(&p, "toss").expect("shim");
    assert_eq!(err(shim.register_payee(payee()).await), err(p.register_seller(&draft).await));
    assert_eq!(err(shim.status("t-1").await), err(p.transfer_status("t-1").await));
    assert!(matches!(shim.balance().await, Err(ProviderError::NotConfigured(_))));
}

#[tokio::test]
async fn the_toss_payout_shim_reads_transfers_from_toss() {
    let app = Router::new().route(
        "/v2/payouts/{id}",
        get(|Path(id): Path<String>| async move {
            let status = if id == "t-done" { "COMPLETED" } else { "REJECTED" };
            Json(json!({ "id": id, "status": status, "error": { "message": "계좌 오류" } }))
        }),
    );
    let base = serve(app).await;
    let p = prod().with_toss_api(&base);
    let mut integ = (*p.integ()).clone();
    integ.payout = Some(PayoutSettings {
        mode: PayoutMode::Toss,
        secret_key: Some("test_sk_payout".into()),
        security_key: Some("ab".repeat(32)),
    });
    p.set_integrations(integ);
    let shim = adapters::payout_rail(&p, "toss").expect("shim");
    for id in ["t-done", "t-bad"] {
        assert_eq!(shim.status(id).await.expect("shim"), p.transfer_status(id).await.expect("direct"), "{id}");
    }
    assert_eq!(shim.status("t-done").await.expect("paid"), TransferStatus::Paid);
    assert_eq!(shim.status("t-bad").await.expect("failed"), TransferStatus::Failed("계좌 오류".into()));
}

// ---- sign-in ----

async fn login_suite(v: &dyn SocialVerifier, prefix: &str) {
    let user = v.verify(&format!("{prefix}user-1"), None).await.expect("verify");
    assert_eq!(user.provider_user_id, "user-1");
    assert!(matches!(v.verify(prefix, None).await, Err(ProviderError::Declined(_))), "an empty id");
    v.revoke(&user).await.expect("revoke");
}

#[tokio::test]
async fn the_social_shims_and_the_fake_keep_the_contract() {
    let p = dev();
    for code in ["kakao", "naver", "google", "apple"] {
        login_suite(adapters::social_verifier(&p, code).expect("shim").as_ref(), "dev:").await;
    }
    let fake = FakeSocial::new(true).expect("dev");
    login_suite(&fake, FAKE_TOKEN).await;
    assert_eq!(fake.revoked(), ["user-1"]);
}

#[tokio::test]
async fn the_social_shims_answer_what_the_sign_in_code_answers_without_keys() {
    let p = prod();
    for code in ["kakao", "naver", "google", "apple"] {
        let shim = adapters::social_verifier(&p, code).expect("shim");
        let via_shim = shim.verify("token:state", None).await.err();
        let direct = social::verify(&p, code, "token:state", None).await.err();
        assert!(direct.is_some() && via_shim.is_some(), "{code}");
        let user = api::adapters::login::SocialUser {
            provider_user_id: "u".into(),
            email: None,
            audience: None,
            refresh_token: None,
        };
        let revoke = shim.revoke(&user).await.err();
        let direct = social::unlink_one(&p, code, "u", None, None).await.err();
        assert_eq!(revoke.is_some(), direct.is_some(), "{code}");
    }
    let kakao = adapters::social_verifier(&p, "kakao").expect("shim");
    assert!(matches!(kakao.verify("t", None).await, Err(ProviderError::NotConfigured(_))), "no Kakao app id");
    let naver = adapters::social_verifier(&p, "naver").expect("shim");
    assert!(matches!(naver.verify("no-state", None).await, Err(ProviderError::Declined(m)) if m == "invalid_token"));
}

// ---- address ----

const JUSO_KEY: &str = "devU01TX0FVVEgyMDI2";

fn juso_reply(keyword: &str) -> Value {
    let juso: Vec<Value> = if keyword.contains("테헤란로") {
        vec![
            json!({ "roadAddr": "서울특별시 강남구 테헤란로 152", "jibunAddr": "서울특별시 강남구 역삼동 737", "zipNo": "06236", "bdNm": "강남파이낸스센터", "admCd": "1168010100" }),
        ]
    } else {
        Vec::new()
    };
    json!({ "results": { "common": { "errorCode": "0", "totalCount": juso.len().to_string() }, "juso": juso } })
}

async fn juso_mock() -> String {
    let app = Router::new().fallback(|axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>| async move {
        if q.get("confmKey").map(String::as_str) != Some(JUSO_KEY) {
            return Json(json!({ "results": { "common": { "errorCode": "E0001", "errorMessage": "승인되지 않은 KEY" } } }));
        }
        Json(juso_reply(q.get("keyword").map_or("", String::as_str)))
    });
    serve(app).await
}

fn query(keyword: &str, page: u32) -> AddressQuery {
    AddressQuery {
        keyword: keyword.into(),
        page,
        postcode_first: false,
    }
}

async fn address_suite(a: &dyn AddressLookup, keyword: &str) {
    let hits = a.search(query(keyword, 1)).await.expect("search");
    assert!(!hits.is_empty(), "{keyword}");
    assert!(hits.iter().all(|h| h.area_code.len() == 5 && !h.postcode.is_empty() && !h.label.is_empty()));
    assert!(matches!(a.search(query("a", 1)).await, Err(ProviderError::Declined(m)) if m == "invalid_query"));
    assert!(matches!(a.search(query(keyword, 0)).await, Err(ProviderError::Declined(m)) if m == "invalid_page"));
    assert_eq!(a.reverse(adapters::address::LatLng { lat: 37.5, lng: 127.0 }).await.expect("reverse"), None);
}

fn with_juso_key(p: &Providers) {
    let mut integ: Integrations = (*p.integ()).clone();
    integ.juso_confm_key = Some(JUSO_KEY.into());
    p.set_integrations(integ);
}

#[tokio::test]
async fn the_juso_shim_and_the_fake_keep_the_contract() {
    let base = juso_mock().await;
    let p = prod();
    with_juso_key(&p);
    address_suite(&Juso::at(&p, &base), "테헤란로 152").await;
    address_suite(&FakeAddress::new(true).expect("dev"), "fake street").await;
}

#[tokio::test]
async fn the_juso_shim_answers_what_the_address_search_answers() {
    let base = juso_mock().await;
    let p = prod();
    assert!(matches!(
        Juso::at(&p, &base).search(query("테헤란로", 1)).await,
        Err(ProviderError::NotConfigured(_))
    ));
    with_juso_key(&p);
    let shim = Juso::at(&p, &base).search(query("  테헤란로   152 ", 1)).await.expect("shim");
    let direct = address::search_at(p.http(), &base, JUSO_KEY, "테헤란로 152", 1).await.expect("direct");
    let mapped: Vec<(String, String, String)> = direct.into_iter().map(|h| (h.road_address, h.zip_code, h.district_code)).collect();
    let got: Vec<(String, String, String)> = shim.into_iter().map(|h| (h.label, h.postcode, h.area_code)).collect();
    assert_eq!(got, mapped);
    assert_eq!(got[0].2, "11680");
    let sql = Juso::at(&p, &base).search(query("강남' OR 1=1", 1)).await;
    assert_eq!(err(sql), Some("declined: invalid_query".into()));
    assert!(adapters::address_lookup(&p, "juso").is_ok(), "registered");
}

// ---- tax documents ----

#[derive(Clone, Default)]
struct Receipts {
    /// Issued receipts by payment id: issue number.
    issued: Arc<Mutex<HashMap<String, String>>>,
    bodies: Arc<Mutex<Vec<Value>>>,
}

async fn issue_receipt(State(r): State<Receipts>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    let id = body["paymentId"].as_str().unwrap_or_default().to_string();
    r.bodies.lock().expect("lock").push(body);
    let mut issued = r.issued.lock().expect("lock");
    if issued.contains_key(&id) {
        return (StatusCode::CONFLICT, Json(json!({ "type": "CASH_RECEIPT_ALREADY_ISSUED" })));
    }
    let number = format!("CR-{}", issued.len() + 1);
    issued.insert(id, number.clone());
    (StatusCode::OK, Json(json!({ "cashReceipt": { "issueNumber": number } })))
}

async fn read_receipt(State(r): State<Receipts>, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match r.issued.lock().expect("lock").get(&id) {
        Some(n) => (StatusCode::OK, Json(json!({ "status": "ISSUED", "issueNumber": n }))),
        None => (StatusCode::NOT_FOUND, Json(json!({}))),
    }
}

async fn cancel_receipt(State(r): State<Receipts>, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    let known = r.issued.lock().expect("lock").contains_key(&id);
    let code = if known { StatusCode::OK } else { StatusCode::NOT_FOUND };
    (code, Json(json!({ "cancelledAt": "2026-11-28T00:00:00Z" })))
}

fn receipt_portone(base_on: bool) -> PortOne {
    PortOne {
        api_secret: Some(common::PORTONE_SECRET.into()),
        store_id: Some("store-test".into()),
        channel_keys: [("cash_receipt".to_string(), "channel-key-receipt".to_string())].into(),
        cash_receipts: base_on,
        ..PortOne::default()
    }
}

fn doc(id: &str) -> TaxDoc {
    TaxDoc {
        kind: DocKind::CashReceipt,
        id: id.into(),
        total: Money(55_000),
        tax: Money(5_000),
        currency: "KRW".into(),
        buyer: Some("01012345678".into()),
        business: false,
        seq_no: 1,
        prev_hash: None,
    }
}

async fn invoice_suite(i: &dyn InvoiceSink) {
    assert!(i.kinds().contains(&DocKind::CashReceipt));
    let first = i.issue(doc("pay-1")).await.expect("issue");
    assert!(first.number.is_some());
    assert_eq!(
        i.issue(doc("pay-1")).await.expect("again"),
        first,
        "issuing again reads the first document back"
    );
    i.cancel(&first.id, "refunded").await.expect("cancel");
    assert!(matches!(i.cancel("pay-unknown", "refunded").await, Err(ProviderError::NotFound)));
}

#[tokio::test]
async fn the_cash_receipt_shim_and_the_fake_keep_the_contract() {
    let receipts = Receipts::default();
    let app = Router::new()
        .route("/cash-receipts", post(issue_receipt))
        .route("/payments/{id}/cash-receipt", get(read_receipt))
        .route("/payments/{id}/cash-receipt/cancel", post(cancel_receipt))
        .with_state(receipts.clone());
    let base = serve(app).await;
    let p = providers(false, receipt_portone(true)).with_portone_api(&base);
    let shim = adapters::invoice_sink(&p, "portone_cash_receipt").expect("shim");
    invoice_suite(shim.as_ref()).await;
    invoice_suite(&FakeInvoices::new(true).expect("dev")).await;
    // The request is the one the cash-receipt job builds today.
    let today = pcr::issue_body(Some("store-test".into()), "pay-1", "channel-key-receipt", false, "01012345678", 55_000);
    assert_eq!(receipts.bodies.lock().expect("lock")[0], serde_json::to_value(today).expect("json"));
    let mut corporate = doc("pay-2");
    (corporate.business, corporate.buyer) = (true, Some("1234567890".into()));
    shim.issue(corporate).await.expect("corporate");
    let sent = receipts.bodies.lock().expect("lock")[2].clone();
    assert_eq!(
        (sent["type"].as_str(), sent["customer"]["identityNumberType"].as_str()),
        (Some("CORPORATE"), Some("BUSINESS"))
    );
}

#[tokio::test]
async fn the_cash_receipt_shim_refuses_what_the_job_refuses() {
    let off = providers(false, receipt_portone(false));
    let shim = adapters::invoice_sink(&off, "portone_cash_receipt").expect("shim");
    assert!(matches!(shim.issue(doc("pay-1")).await, Err(ProviderError::NotConfigured(_))), "switched off");
    let mut no_secret = receipt_portone(true);
    no_secret.api_secret = None;
    let shim = adapters::invoice_sink(&providers(false, no_secret), "portone_cash_receipt").expect("shim");
    assert!(matches!(shim.issue(doc("pay-1")).await, Err(ProviderError::NotConfigured(_))), "no secret");
    let on = providers(false, receipt_portone(true));
    let shim = adapters::invoice_sink(&on, "portone_cash_receipt").expect("shim");
    let mut invoice = doc("pay-1");
    invoice.kind = DocKind::TaxInvoice;
    assert!(matches!(shim.issue(invoice).await, Err(ProviderError::NotConfigured(_))), "cash receipts only");
}

// ---- identity ----

async fn identity_suite(v: &dyn IdentityVerifier) {
    let subject = Subject { phone_e164: None, name: None };
    let session = v.start(subject, IdPurpose::Signup).await.expect("start");
    let info = v.result(&session.id).await.expect("result");
    assert_eq!((info.status.as_str(), info.id.as_str()), ("VERIFIED", session.id.as_str()));
    assert!(info.ci.is_some());
    assert!(matches!(v.result("unknown").await, Err(ProviderError::NotFound)));
}

#[tokio::test]
async fn the_identity_shim_and_the_fake_keep_the_contract() {
    let p = dev();
    identity_suite(adapters::identity_verifier(&p, "portone_identity").expect("shim").as_ref()).await;
    identity_suite(&FakeIdentity::new(true).expect("dev")).await;
}

#[tokio::test]
async fn the_identity_shim_starts_sessions_as_the_route_does() {
    let subject = || Subject { phone_e164: None, name: None };
    let shim = adapters::identity_verifier(&prod(), "portone_identity").expect("shim");
    assert!(
        matches!(shim.start(subject(), IdPurpose::Signup).await, Err(ProviderError::NotConfigured(_))),
        "no identity channel"
    );
    let portone = PortOne {
        store_id: Some("store-test".into()),
        channel_keys: [("identity".to_string(), "channel-key-identity".to_string())].into(),
        ..PortOne::default()
    };
    let shim = adapters::identity_verifier(&providers(false, portone), "portone_identity").expect("shim");
    let session = shim.start(subject(), IdPurpose::Signup).await.expect("start");
    assert!(session.id.starts_with("idv-"));
    assert_eq!(
        session.sdk_params,
        Some(json!({ "store_id": "store-test", "channel_key": "channel-key-identity" }))
    );
    let in_dev = adapters::identity_verifier(&dev(), "portone_identity").expect("shim");
    let session = in_dev.start(subject(), IdPurpose::Payout).await.expect("dev start");
    assert!(session.id.starts_with("dev:"));
    let direct = dev().identity(&session.id).await.expect("dev identity").ci;
    assert_eq!(in_dev.result(&session.id).await.expect("shim").ci, direct);
}

// ---- the registry ----

#[tokio::test]
async fn each_port_accessor_refuses_codes_of_another_capability() {
    let p = dev();
    assert!(adapters::messenger(&p, "juso").is_err());
    assert!(adapters::payout_rail(&p, "portone").is_err());
    assert!(adapters::social_verifier(&p, "solapi").is_err());
    assert!(adapters::address_lookup(&p, "stripe").is_err());
    assert!(adapters::invoice_sink(&p, "portone").is_err());
    assert!(adapters::identity_verifier(&p, "toss").is_err());
    assert!(adapters::payment_gateway(&p, "portone_cash_receipt").is_err());
}
