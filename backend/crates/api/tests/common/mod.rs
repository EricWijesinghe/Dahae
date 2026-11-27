// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

#![allow(dead_code)]
//! Shared helpers for the DB-backed flow tests: the router with dev-mode fakes, a JSON call helper,
//! and setup steps (sign-in, verified pro, customer with card and address, paid booking).

use api::{
    auth::{self, Role},
    build_state,
    config::{Config, PortOne},
    crypto,
    providers::Providers,
    routes, seed, totp,
};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use chrono::Duration;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceExt;
use uuid::Uuid;

pub const SECRET: &[u8] = b"flow-test-secret-flow-test-secret-0000";
/// Photo URL that passes `is_own_upload` for the test config's public base URL.
pub const PHOTO: &str = "http://127.0.0.1:8080/files/0123456789abcdef0123456789abcdef.jpg";
/// District of every test address; the test pros cover it.
pub const DISTRICT: &str = "11680";
/// Public base URL of the test config; signed file URLs are built on it.
pub const BASE_URL: &str = "http://127.0.0.1:8080";

/// Dev-mode configuration: fake SMS, payments and push. Keep in step with `Config` fields.
pub fn test_config() -> Config {
    Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".parse().expect("bind addr"),
        jwt_secret: SECRET.to_vec(),
        jwt_previous_secret: None,
        otp_pepper: None,
        data_key: [7u8; 32],
        host_jobs_dir: std::path::PathBuf::from("/nonexistent-host-jobs"),
        instance_backup: None,
        upload_dir: std::env::temp_dir().join("api-flow-tests"),
        public_base_url: BASE_URL.into(),
        dev_mode: true,
        staff_mfa_required: true,
        allowed_origins: Vec::new(),
        seed_admin: None,
        setup_code: None,
        setup_code_at: None,
        kakao_app_id: None,
        kakao_admin_key: None,
        google_client_ids: Vec::new(),
        apple_client_ids: Vec::new(),
        portone: PortOne::default(),
        solapi: None,
        fcm: None,
        trusted_proxy: false,
        rate_auth_per_min: 1_000,
        rate_upload_per_min: 1_000,
        rate_general_per_min: 10_000,
        upload_daily_quota: 1_000,
        max_open_quote_requests: 100,
        review_report_threshold: 3,
        max_threads_per_day: 100,
        reconcile_interval_secs: 60,
        payment_diff_interval_secs: 3600,
        anthropic_base_url: "http://127.0.0.1:9".into(),
        android_cert_sha256: Vec::new(),
        apple_team_id: None,
        apple_signin: None,
        ai_chat_retention_days: 180,
        upload_orphan_days: 30,
        db: Default::default(),
        metrics_token: None,
        noshow_grace_minutes: 30,
        edition: "dahae".into(),
        editions_offered: Vec::new(),
    }
}

/// The API router over `db`, configured like `DEV_MODE=1`.
pub fn app(db: PgPool) -> Router {
    let state = build_state(db, test_config()).expect("http client");
    routes::router(state)
}

/// The API router with PortOne channel keys set for `channels`: a payment method is offered only when its key is here.
pub fn app_with_channels(db: PgPool, channels: &[&str]) -> Router {
    let mut cfg = test_config();
    cfg.portone.channel_keys = channels.iter().map(|c| (c.to_string(), format!("ch-{c}"))).collect();
    routes::router(build_state(db, cfg).expect("http client"))
}

/// The API router on the real PortOne gateway (DEV_MODE off, store and API secret set) with channel keys for
/// `channels`. Payment methods are gated on their keys here; the dev fake offers every enabled method instead.
pub fn app_real_gateway_with_channels(db: PgPool, channels: &[&str]) -> Router {
    let mut cfg = test_config();
    cfg.dev_mode = false;
    cfg.portone = PortOne {
        api_secret: Some(PORTONE_SECRET.into()),
        store_id: Some("store-test".into()),
        channel_keys: channels.iter().map(|c| (c.to_string(), format!("ch-{c}"))).collect(),
        ..PortOne::default()
    };
    routes::router(build_state(db, cfg).expect("http client"))
}

/// The API router with staff second factors off, for tests about sessions, lockouts and resets rather than MFA
/// (MFA itself is covered by auth_mfa and staff_mfa).
pub fn app_without_staff_mfa(db: PgPool) -> Router {
    let mut cfg = test_config();
    cfg.staff_mfa_required = false;
    routes::router(build_state(db, cfg).expect("http client"))
}

/// The API router configured like production (`DEV_MODE` off): PortOne calls go to `portone_api` (a mock).
/// Sign-in still needs the dev router (OTP codes are only echoed in dev mode); tokens work on both.
pub fn prod_app(db: PgPool, portone_api: &str) -> Router {
    let mut cfg = test_config();
    cfg.dev_mode = false;
    cfg.portone = PortOne {
        api_secret: Some(PORTONE_SECRET.into()),
        ..PortOne::default()
    };
    let mut state = build_state(db.clone(), cfg.clone()).expect("http client");
    state.providers = Arc::new(Providers::new(cfg, db).expect("http client").with_portone_api(portone_api));
    routes::router(state)
}

/// API secret `prod_app` sends to the PortOne mock.
pub const PORTONE_SECRET: &str = "test-portone-secret";

pub mod toss;

/// A production-mode app and the state behind it, so a test can change the saved Toss card and reload.
pub struct TossEnv {
    pub app: Router,
    pub state: api::state::AppState,
}

/// Production mode with PortOne (secret, store, a `transfer` channel only) at `portone_api` and the Toss card
/// (both MIDs and the 보안 키) at `toss_api`, switched on when `active`. The card is stored like a console save,
/// so `setup::reload` keeps it.
pub async fn prod_app_toss(db: PgPool, toss_api: &str, portone_api: &str, active: bool) -> TossEnv {
    let portone = PortOne {
        api_secret: Some(PORTONE_SECRET.into()),
        store_id: Some("store-test".into()),
        // PortOne serves transfer only, so card, kakaopay and saved cards go to Toss while it is on.
        channel_keys: [("transfer".to_string(), "channel-key-transfer".to_string())].into(),
        cash_receipts: true,
        ..PortOne::default()
    };
    let env = prod_app_gateways(db, toss_api, portone_api, portone).await;
    set_toss(&env, toss::card(active)).await;
    env
}

/// Production mode with the given env PortOne settings and no Toss card saved yet (see `set_toss`).
pub async fn prod_app_gateways(db: PgPool, toss_api: &str, portone_api: &str, portone: PortOne) -> TossEnv {
    let mut cfg = test_config();
    cfg.dev_mode = false;
    cfg.portone = portone;
    let mut state = build_state(db.clone(), cfg.clone()).expect("http client");
    let providers = Providers::new(cfg, db)
        .expect("http client")
        .with_portone_api(portone_api)
        .with_toss_api(toss_api);
    state.providers = Arc::new(providers);
    TossEnv {
        app: routes::router(state.clone()),
        state,
    }
}

/// Saves the Toss card values (encrypted, as the console stores them) and reloads the live keys.
pub async fn set_toss(env: &TossEnv, values: serde_json::Map<String, Value>) {
    let enc = crypto::encrypt(&env.state.config.data_key, crypto::aad::INTEGRATION, &Value::Object(values).to_string());
    sqlx::query("INSERT INTO integration_settings (section, secret) VALUES ('tosspayments', $1) ON CONFLICT (section) DO UPDATE SET secret = EXCLUDED.secret")
        .bind(enc)
        .execute(&env.state.db)
        .await
        .expect("toss card");
    routes::setup::reload(&env.state).await.expect("reload");
}

/// A live staff row plus a token for it. Returns (token, staff id).
pub async fn staff_token(db: &PgPool, role: Role) -> (String, Uuid) {
    let id = Uuid::now_v7();
    let db_role = if role == Role::StaffAdmin { "admin" } else { "consultant" };
    // Stepped up, so sensitive actions (export, deletion, payouts) are open to the test token.
    sqlx::query("INSERT INTO staff (id, email, name, password_hash, role, stepup_at) VALUES ($1, $2, 'tester', 'x', $3, now())")
        .bind(id)
        .bind(format!("{id}@test.local"))
        .bind(db_role)
        .execute(db)
        .await
        .expect("staff row");
    (auth::issue_access(SECRET, id, role).expect("token"), id)
}

/// Makes a staff row an owner with a confirmed second factor whose TOTP secret is `seed` (test seeds are fake).
pub async fn enrol_owner(db: &PgPool, id: Uuid, seed: &[u8]) {
    let enc = crypto::encrypt(&test_config().data_key, crypto::aad::STAFF_TOTP, &totp::encode_secret(seed));
    sqlx::query("UPDATE staff SET totp_secret = $2, totp_enabled_at = now(), recovery_confirmed_at = now(), is_owner = true WHERE id = $1")
        .bind(id)
        .bind(enc)
        .execute(db)
        .await
        .expect("owner enrolment");
}

/// Catalogue, quote formula, slot capacity and starter content (what first boot inserts).
pub async fn seed_catalog(db: &PgPool) {
    seed::run(db, &test_config()).await.expect("seed");
}

/// One request through the router. Returns the status and the parsed JSON body (Null when empty).
pub async fn call(app: &Router, method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    call_keyed(app, method, uri, token, body, None).await
}

/// `call` with an `Idempotency-Key` header.
pub async fn call_keyed(app: &Router, method: &str, uri: &str, token: Option<&str>, body: Option<Value>, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(k) = key {
        req = req.header("idempotency-key", k);
    }
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let payload = match body {
        Some(v) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let mut request = req.body(payload).expect("valid request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let resp = app.clone().oneshot(request).await.expect("router is infallible");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("response body");
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// One request with an optional `X-Owner-Totp` header, for owner-gated routes.
pub async fn call_owner(app: &Router, method: &str, uri: &str, token: &str, totp: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(code) = totp {
        req = req.header("x-owner-totp", code);
    }
    let payload = if body.is_null() {
        Body::empty()
    } else {
        req = req.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    };
    let mut request = req.body(payload).expect("valid request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let resp = app.clone().oneshot(request).await.expect("router is infallible");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("response body");
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// The required customer consents, granted for the current policy version, as the app sends them at signup.
pub fn required_consents() -> Value {
    let grant = |kind: &str| json!({ "kind": kind, "granted": true, "version": api::privacy::CONSENT_VERSION });
    json!([grant("terms"), grant("privacy"), grant("overseas_transfer")])
}

/// Signs in through the OTP flow (dev mode echoes the code). Returns (access token, subject id).
/// A customer sign-up grants the required consents, as the app does.
pub async fn login(app: &Router, phone: &str, audience: &str) -> (String, String) {
    let (st, sent) = call(app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": phone }))).await;
    assert_eq!(st, StatusCode::OK, "otp request: {sent}");
    let code = sent["dev_code"].as_str().expect("dev_code in dev mode").to_string();
    let consents = if audience == "customer" { required_consents() } else { json!([]) };
    let (st, tokens) = call(
        app,
        "POST",
        "/v1/auth/verify",
        None,
        Some(json!({ "phone": phone, "code": code, "audience": audience, "consents": consents })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "verify: {tokens}");
    (
        tokens["access_token"].as_str().expect("access token").to_string(),
        tokens["subject"].as_str().expect("subject").to_string(),
    )
}

/// Pro signed in and made verified, taking studio jobs in the test district on every weekday.
/// Returns (access token, pro id).
pub async fn verified_pro(app: &Router, db: &PgPool, phone: &str) -> (String, Uuid) {
    let (token, subject) = login(app, phone, "partner").await;
    let id: Uuid = subject.parse().expect("pro id");
    sqlx::query("UPDATE pros SET status = 'verified' WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .expect("verify pro");
    sqlx::query(
        "UPDATE partner_prefs SET services = ARRAY['studio'], districts = ARRAY[$2::text],
           weekdays = ARRAY[0,1,2,3,4,5,6]::smallint[], day_parts = ARRAY['morning','afternoon'] WHERE pro_id = $1",
    )
    .bind(id)
    .bind(DISTRICT)
    .execute(db)
    .await
    .expect("pro prefs");
    (token, id)
}

/// Records `PHOTO` as uploaded by `owner`, as the upload route would. Completion and review refuse
/// photos the caller did not upload, so every test that attaches `PHOTO` registers it first.
pub async fn register_photo(db: &PgPool, owner: Uuid) {
    sqlx::query("INSERT INTO uploads (owner_role, owner_id, file, bytes) VALUES ('test', $1, '0123456789abcdef0123456789abcdef.jpg', 1)")
        .bind(owner)
        .execute(db)
        .await
        .expect("register photo");
}

pub struct Customer {
    pub token: String,
    pub user: Uuid,
    pub address: Uuid,
    pub card: Uuid,
}

/// Customer signed up by OTP, with an address in the test district and a saved card (dev billing key).
pub async fn customer(app: &Router, db: &PgPool, phone: &str) -> Customer {
    let (token, subject) = login(app, phone, "customer").await;
    let user: Uuid = subject.parse().expect("user id");
    let (st, addr) = call(
        app,
        "POST",
        "/v1/me/addresses",
        Some(token.as_str()),
        Some(json!({
            "kind": "home",
            "label": "집",
            "road_address": "서울특별시 강남구 테헤란로 1",
            "detail": "",
            "district_code": DISTRICT,
            "size_pyeong": 8
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "address: {addr}");
    let (sealed, digest) = api::crypto::seal_billing_key(&test_config().data_key, &format!("bk_test_{user}"));
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO payment_methods (user_id, provider, billing_key, billing_key_digest, display_name) VALUES ($1, 'pg', $2, $3, 'test card') RETURNING id",
    )
    .bind(user)
    .bind(sealed)
    .bind(digest)
    .fetch_one(db)
    .await
    .expect("payment method");
    Customer {
        token,
        user,
        address: addr["id"].as_str().expect("address id").parse().expect("uuid"),
        card,
    }
}

/// Books a studio clean `days` from now, paid with the customer's card. Returns (booking id, response body).
pub async fn book(app: &Router, c: &Customer, days: i64, coupon: Option<Uuid>) -> (Uuid, Value) {
    let visit = api::util::local_today() + Duration::days(days);
    let (st, b) = call(
        app,
        "POST",
        "/v1/bookings",
        Some(c.token.as_str()),
        Some(json!({
            "service_code": "studio",
            "address_id": c.address,
            "visit_date": visit,
            "start_hour": 9,
            "coupon_id": coupon,
            "visit_notes": {},
            "payment_method_id": c.card
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "booking: {b}");
    let id = b["id"].as_str().expect("booking id").parse().expect("uuid");
    (id, b)
}

/// Compares `value` with `tests/golden/kr/<name>.json`, or rewrites it when `UPDATE_GOLDENS=1` (PLAN section 8.1 step 1).
pub fn golden(name: &str, value: &Value) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/kr")
        .join(format!("{name}.json"));
    let actual = serde_json::to_string_pretty(value).expect("golden JSON") + "\n";
    if std::env::var("UPDATE_GOLDENS").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}; run with UPDATE_GOLDENS=1 to create it", path.display()));
    assert!(
        expected == actual,
        "{} differs from today's output (run with UPDATE_GOLDENS=1 only for a reviewed change):\n{actual}",
        path.display()
    );
}

/// `value` with every UUID (hyphenated, or 32 hex digits on their own) replaced by `<uuid-N>`, numbered in order of
/// first appearance, so ids made per run compare equal in a golden.
pub fn redact_uuids(value: &Value) -> Value {
    let text = value.to_string();
    let hex = |b: Option<&u8>| b.is_some_and(u8::is_ascii_hexdigit);
    let mut seen: Vec<Uuid> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let bytes = text.as_bytes();
        let found = [36, 32].into_iter().find_map(|len| {
            let s = text.get(i..i + len)?;
            let alone = !hex(i.checked_sub(1).and_then(|p| bytes.get(p))) && !hex(bytes.get(i + len));
            let shaped = if len == 36 {
                bytes[i + 8] == b'-'
            } else {
                s.bytes().all(|b| b.is_ascii_hexdigit())
            };
            (alone && shaped).then(|| Uuid::parse_str(s).ok().map(|id| (id, len))).flatten()
        });
        match found {
            Some((id, len)) => {
                let n = seen.iter().position(|s| *s == id).unwrap_or_else(|| {
                    seen.push(id);
                    seen.len() - 1
                });
                out.push_str(&format!("<uuid-{n}>"));
                i += len;
            }
            None => {
                let ch = text[i..].chars().next().expect("char");
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    serde_json::from_str(&out).expect("redacted JSON")
}
