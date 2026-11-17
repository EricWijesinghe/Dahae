// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Setup wizard flows: owner account from the one-time code, business profile, integration keys.

mod common;

use api::routes::setup::ensure_setup_code;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use common::call;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;

/// Owner sign-in: password, then the admin's first-time TOTP enrolment (admins must have a second factor).
async fn owner_token(app: &axum::Router, db: &PgPool) -> String {
    owner_session(app, db).await.0
}

/// Owner signed in with a confirmed TOTP. Returns the access token and the TOTP secret.
async fn owner_session(app: &axum::Router, db: &PgPool) -> (String, Vec<u8>) {
    let (st, b) = call(
        app,
        "POST",
        "/v1/auth/staff/login",
        None,
        Some(json!({"email": "owner@test.local", "password": "owner-password-123"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["mfa_enrol_required"], json!(true), "{b}");
    let mfa = b["mfa_token"].as_str().expect("mfa token").to_string();
    let (st, e) = call(app, "POST", "/v1/auth/staff/mfa/enrol", None, Some(json!({ "mfa_token": mfa }))).await;
    assert_eq!(st, StatusCode::OK, "{e}");
    let secret = api::totp::decode_secret(e["secret"].as_str().expect("secret")).expect("base32");
    let code = format!(
        "{:06}",
        api::totp::code_at(&secret, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    );
    let (st, c) = call(
        app,
        "POST",
        "/v1/auth/staff/mfa/enrol/confirm",
        None,
        Some(json!({ "mfa_token": mfa, "code": code })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    // Saving or unlocking a card needs a confirmed second factor, i.e. the recovery codes were confirmed too.
    sqlx::query("UPDATE staff SET recovery_confirmed_at = now() WHERE email = 'owner@test.local'")
        .execute(db)
        .await
        .expect("confirm recovery");
    (c["access_token"].as_str().expect("token").to_string(), secret)
}

/// The owner's current code, with the last step cleared: a step is single-use, and an earlier request may have spent it.
async fn owner_code(db: &PgPool, secret: &[u8]) -> String {
    sqlx::query("UPDATE staff SET totp_last_step = NULL").execute(db).await.expect("reset step");
    format!(
        "{:06}",
        api::totp::code_at(secret, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    )
}

/// Owner unlock of a saved card. A TOTP step is single-use, so the last step is cleared before each unlock.
async fn unlock(app: &axum::Router, db: &PgPool, token: &str, secret: &[u8], section: &str) {
    sqlx::query("UPDATE staff SET totp_last_step = NULL").execute(db).await.expect("reset step");
    let code = format!(
        "{:06}",
        api::totp::code_at(secret, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    );
    let (st, b) = common::call_owner(app, "POST", &format!("/v1/admin/setup/{section}/unlock"), token, Some(&code), Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{b}");
}

async fn create_owner(app: &axum::Router, code: &str) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/v1/setup/admin",
        None,
        Some(json!({"code": code, "email": "Owner@Test.local", "name": "사장님", "password": "owner-password-123"})),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_owner_needs_the_setup_code_and_only_one_can_be_created(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let app = common::app(pool.clone());
    let (_, s) = call(&app, "GET", "/v1/setup", None, None).await;
    assert_eq!(s["needs_admin"], true);

    let (st, _) = create_owner(&app, "WRONG2345678").await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, b) = create_owner(&app, "abcd-2345-efgh").await;
    assert_eq!(st, StatusCode::OK, "code is case and dash insensitive: {b}");
    let (st, _) = create_owner(&app, "abcd-2345-efgh").await;
    assert_eq!(st, StatusCode::CONFLICT, "second owner refused");

    let (_, s) = call(&app, "GET", "/v1/setup", None, None).await;
    assert_eq!(s["needs_admin"], false);
    owner_token(&app, &pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn installer_setup_code_expires_after_two_hours(pool: PgPool) {
    let now = chrono::Utc::now().timestamp();
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), Some(now - 7201)).await.expect("code");
    let app = common::app(pool.clone());
    let (st, b) = create_owner(&app, "ABCD2345EFGH").await;
    assert_eq!(st, StatusCode::FORBIDDEN, "expired code refused even when correct: {b}");
    let (_, s) = call(&app, "GET", "/v1/setup", None, None).await;
    assert_eq!(s["needs_admin"], true, "expired code does not create the owner");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_saves_business_and_keys_secrets_come_back_masked(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let app = common::app(pool.clone());
    create_owner(&app, "ABCD2345EFGH").await;
    let (token, secret) = owner_session(&app, &pool).await;
    let t = Some(token.as_str());

    let (st, b) = call(
        &app,
        "PUT",
        "/v1/admin/setup/business",
        t,
        Some(json!({"name": "다해", "primary_color": "#3b5bfd", "company": {"hosting_provider": "예시호스팅(주)"}})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, brand) = call(&app, "GET", "/v1/brand", None, None).await;
    assert_eq!(brand["name"], "다해");
    assert_eq!(brand["primary_color"], "#3B5BFD");
    assert_eq!(brand["company"]["hosting_provider"], "예시호스팅(주)", "seller details are public");

    let code = owner_code(&pool, &secret).await;
    let (st, b) = common::call_owner(&app, "PUT", "/v1/admin/setup/portone", &token, Some(&code), json!({"store_id": "store-1"})).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "secret is required: {b}");
    let keys = json!({"store_id": "store-1", "api_secret": "secret-value-9876", "channel_keys": {"card": "channel-key-card"}});
    let (st, card) = common::call_owner(&app, "PUT", "/v1/admin/setup/portone", &token, Some(&code), keys).await;
    assert_eq!(st, StatusCode::OK, "{card}");
    assert_eq!(card["source"], "console");
    let api_secret = card["fields"].as_array().unwrap().iter().find(|f| f["name"] == "api_secret").unwrap();
    assert_eq!(api_secret["preview"], "••••", "a secret shows no part of its value");
    let raw: String = sqlx::query_scalar("SELECT secret FROM integration_settings WHERE section = 'portone'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!raw.contains("secret-value-9876"), "stored encrypted");

    // Blank secret keeps the saved one; the checkout now hands the app the console's store id.
    unlock(&app, &pool, &token, &secret, "portone").await;
    let (st, _) = call(
        &app,
        "PUT",
        "/v1/admin/setup/portone",
        t,
        Some(json!({"store_id": "store-2", "api_secret": ""})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, all) = call(&app, "GET", "/v1/admin/setup", t, None).await;
    let portone = all["integrations"].as_array().unwrap().iter().find(|c| c["code"] == "portone").unwrap().clone();
    assert_eq!(portone["fields"][0]["preview"], "store-2", "the store id is an identifier and shows in full");
    assert_eq!(portone["fields"][1]["set"], true);

    unlock(&app, &pool, &token, &secret, "portone").await;
    let (st, _) = call(&app, "DELETE", "/v1/admin/setup/portone", t, None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, all) = call(&app, "GET", "/v1/admin/setup", t, None).await;
    let portone = all["integrations"].as_array().unwrap().iter().find(|c| c["code"] == "portone").unwrap().clone();
    assert_eq!(portone["source"], "none");
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_key_prefix_sets_the_wizard_mode_and_the_key_stays_masked(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let app = common::app(pool.clone());
    create_owner(&app, "ABCD2345EFGH").await;
    let (token, secret) = owner_session(&app, &pool).await;
    let t = Some(token.as_str());

    // Manual payouts leave Toss off, so no owner code is asked for here.
    let (st, test) = call(
        &app,
        "PUT",
        "/v1/admin/setup/payout",
        t,
        Some(json!({"mode": "manual", "secret_key": "test_sk_abcdef1234"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{test}");
    assert_eq!(test["mode"], "test");
    assert!(!test.to_string().contains("test_sk_abcdef1234"), "the key itself is never returned");
    let preview = test["fields"].as_array().unwrap().iter().find(|f| f["name"] == "secret_key").unwrap()["preview"].clone();
    assert_eq!(preview, "••••");

    unlock(&app, &pool, &token, &secret, "payout").await;
    let (st, live) = call(&app, "PUT", "/v1/admin/setup/payout", t, Some(json!({"secret_key": "live_sk_zz9876"}))).await;
    assert_eq!(st, StatusCode::OK, "{live}");
    assert_eq!(live["mode"], "live");

    unlock(&app, &pool, &token, &secret, "payout").await;
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/payout", t, Some(json!({"secret_key": "sk_plain_no_mode"}))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["mode"], Value::Null, "an unknown prefix has no mode");

    let (_, all) = call(&app, "GET", "/v1/admin/setup", t, None).await;
    let portone = all["integrations"].as_array().unwrap().iter().find(|c| c["code"] == "portone").unwrap();
    assert_eq!(portone["mode"], Value::Null, "PortOne ids carry no test or live mode");
}

/// 64 hex characters, the shape of a Toss 보안 키. Only ever a test value.
const SECURITY_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TOSS_BODY: &str = r#"{"eventType":"payout.changed","entityBody":{"id":"p1"}}"#;

/// `tosspayments-webhook-signature` as Toss sends it: HMAC-SHA256 over `{body}:{transmission-time}`.
fn toss_signature(key: &str, time: &str, body: &str) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()).expect("key");
    mac.update(body.as_bytes());
    mac.update(b":");
    mac.update(time.as_bytes());
    format!("v1:{}", STANDARD.encode(mac.finalize().into_bytes()))
}

/// POSTs a raw Toss webhook through the router and returns only the status.
async fn toss_hook(app: &axum::Router, time: &str, signature: &str, body: &str) -> StatusCode {
    let mut req = Request::builder()
        .method("POST")
        .uri("/v1/webhooks/toss")
        .header("content-type", "application/json")
        .header("tosspayments-webhook-transmission-time", time)
        .header("tosspayments-webhook-signature", signature)
        .body(Body::from(body.to_string()))
        .expect("request");
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    app.clone().oneshot(req).await.expect("router is infallible").status()
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_webhook_checks_the_signature_once_a_security_key_is_saved(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let app = common::app(pool.clone());
    let time = "1800000000";
    assert_eq!(
        toss_hook(&app, time, "", TOSS_BODY).await,
        StatusCode::OK,
        "dev mode without a key accepts unsigned smoke events"
    );

    create_owner(&app, "ABCD2345EFGH").await;
    let t = owner_token(&app, &pool).await;
    let (st, b) = call(
        &app,
        "PUT",
        "/v1/admin/setup/payout",
        Some(t.as_str()),
        Some(json!({"mode": "manual", "security_key": SECURITY_KEY})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");

    assert_eq!(
        toss_hook(&app, time, "", TOSS_BODY).await,
        StatusCode::UNAUTHORIZED,
        "unsigned event refused once a key exists"
    );
    assert_eq!(toss_hook(&app, time, "v1:AAAA", TOSS_BODY).await, StatusCode::UNAUTHORIZED, "made-up signature");
    let wrong_key = toss_signature("not-the-security-key", time, TOSS_BODY);
    assert_eq!(
        toss_hook(&app, time, &wrong_key, TOSS_BODY).await,
        StatusCode::UNAUTHORIZED,
        "signed with another key"
    );
    let signed = toss_signature(SECURITY_KEY, time, TOSS_BODY);
    assert_eq!(
        toss_hook(&app, time, &signed, r#"{"eventType":"payout.changed"}"#).await,
        StatusCode::UNAUTHORIZED,
        "body changed after signing"
    );
    assert_eq!(
        toss_hook(&app, "1800000001", &signed, TOSS_BODY).await,
        StatusCode::UNAUTHORIZED,
        "transmission time changed"
    );
    assert_eq!(
        toss_hook(&app, time, &signed, TOSS_BODY).await,
        StatusCode::OK,
        "a correct signature is accepted"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_webhook_is_unavailable_outside_dev_mode_without_a_key(pool: PgPool) {
    let app = common::prod_app(pool.clone(), "http://127.0.0.1:9");
    let signed = toss_signature(SECURITY_KEY, "1800000000", TOSS_BODY);
    assert_eq!(toss_hook(&app, "1800000000", &signed, TOSS_BODY).await, StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_cannot_read_setup(pool: PgPool) {
    let app = common::app(pool.clone());
    let (token, _) = common::login(&app, "010-5000-0900", "customer").await;
    let (st, _) = call(&app, "GET", "/v1/admin/setup", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_samples_seed_two_bookings_once_and_only_while_active(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    common::seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    create_owner(&app, "ABCD2345EFGH").await;
    let (t, secret) = owner_session(&app, &pool).await;
    let body = json!({"phone": "010-9999-0001", "code": "123456"});
    let (st, b) = put_review(&app, &pool, &t, &secret, body.clone()).await;
    assert_eq!((st, b["samples"].as_i64()), (StatusCode::OK, Some(2)), "{b}");
    let (st, b) = put_review(&app, &pool, &t, &secret, body).await;
    assert_eq!((st, b["samples"].as_i64()), (StatusCode::OK, Some(0)), "saving again adds no samples: {b}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM bookings").fetch_one(&pool).await.expect("count");
    assert_eq!(n, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_account_signs_in_with_the_owner_code_and_no_sms_code_is_echoed(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let app = common::app(pool.clone());
    create_owner(&app, "ABCD2345EFGH").await;
    let (t, secret) = owner_session(&app, &pool).await;
    let (st, b) = put_review(&app, &pool, &t, &secret, json!({"phone": "010-9999-0001", "code": "123456", "days": 7})).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, _) = put_review(&app, &pool, &t, &secret, json!({"phone": "010-9999-0001", "code": "12ab"})).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    let (_, sent) = call(&app, "POST", "/v1/auth/otp", None, Some(json!({"phone": "010-9999-0001"}))).await;
    assert!(sent.get("dev_code").is_none_or(Value::is_null), "fixed code is never echoed: {sent}");
    let (st, b) = call(
        &app,
        "POST",
        "/v1/auth/verify",
        None,
        Some(json!({"phone": "010-9999-0001", "code": "123456", "audience": "customer", "consents": common::required_consents()})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");

    let (st, b) = call(&app, "DELETE", "/v1/admin/setup/review", Some(t.as_str()), None).await;
    assert_eq!(
        (st, &b["error"]["code"]),
        (StatusCode::BAD_REQUEST, &json!("totp_required")),
        "switching off needs the owner code too: {b}"
    );
    let code = owner_code(&pool, &secret).await;
    let (st, b) = common::call_owner(&app, "DELETE", "/v1/admin/setup/review", &t, Some(&code), Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    call(&app, "POST", "/v1/auth/otp", None, Some(json!({"phone": "010-9999-0001"}))).await;
    let (st, _) = call(
        &app,
        "POST",
        "/v1/auth/verify",
        None,
        Some(json!({"phone": "010-9999-0001", "code": "123456", "audience": "customer"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "switched off: the fixed code no longer works");
}

/// PUT /setup/review with a fresh owner code.
async fn put_review(app: &axum::Router, db: &PgPool, token: &str, secret: &[u8], body: Value) -> (StatusCode, Value) {
    let code = owner_code(db, secret).await;
    common::call_owner(app, "PUT", "/v1/admin/setup/review", token, Some(&code), body).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_login_needs_the_owner_code_and_refuses_a_phone_someone_already_holds(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let app = common::app(pool.clone());
    create_owner(&app, "ABCD2345EFGH").await;
    let (t, secret) = owner_session(&app, &pool).await;
    let free = json!({"phone": "010-9999-0001", "code": "123456"});
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/review", Some(t.as_str()), Some(free.clone())).await;
    assert_eq!((st, &b["error"]["code"]), (StatusCode::BAD_REQUEST, &json!("totp_required")), "{b}");

    common::login(&app, "010-5000-0901", "customer").await;
    let (_, pro) = common::verified_pro(&app, &pool, "010-5000-0902").await;
    sqlx::query("INSERT INTO team_members (pro_id, name, phone, role, invite_status) VALUES ($1, 'Member', '01050000903', 'member', 'active')")
        .bind(pro)
        .execute(&pool)
        .await
        .expect("member");
    sqlx::query("UPDATE staff SET phone = '01050000904' WHERE email = 'owner@test.local'")
        .execute(&pool)
        .await
        .expect("staff phone");
    for phone in ["010-5000-0901", "010-5000-0902", "010-5000-0903", "010-5000-0904"] {
        let (st, b) = put_review(&app, &pool, &t, &secret, json!({"phone": phone, "code": "123456"})).await;
        assert_eq!((st, &b["error"]["code"]), (StatusCode::CONFLICT, &json!("phone_in_use")), "{phone}: {b}");
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM settings WHERE key = 'review_account'")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 0, "a refused phone stores no review login");

    let (st, b) = put_review(&app, &pool, &t, &secret, free.clone()).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = put_review(&app, &pool, &t, &secret, free).await;
    assert_eq!(st, StatusCode::OK, "the review account's own row is not a conflict: {b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_owner_can_switch_the_review_login_on_or_off(pool: PgPool) {
    let app = common::app(pool.clone());
    // A non-owner admin: role admin, is_owner false.
    let (admin, _) = common::staff_token(&pool, api::auth::Role::StaffAdmin).await;
    let body = json!({"phone": "010-9999-0001", "code": "123456"});
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/review", Some(admin.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{b}");
    let (st, b) = call(&app, "DELETE", "/v1/admin/setup/review", Some(admin.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{b}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM settings WHERE key = 'review_account'")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 0, "a refused call stores no review login");
}

/// Writes a saved card straight into the settings table (locked, as the console leaves it after a save).
async fn save_card_row(db: &PgPool, section: &str, values: Value) {
    let by: uuid::Uuid = sqlx::query_scalar("SELECT id FROM staff LIMIT 1").fetch_one(db).await.expect("owner");
    let enc = api::crypto::encrypt(&common::test_config().data_key, api::crypto::aad::INTEGRATION, &values.to_string());
    sqlx::query("INSERT INTO integration_settings (section, secret, updated_by) VALUES ($1, $2, $3)")
        .bind(section)
        .bind(enc)
        .bind(by)
        .execute(db)
        .await
        .expect("card");
}

#[sqlx::test(migrations = "../../migrations")]
async fn open_toss_transfer_refuses_manual_switch_and_card_clear_until_it_settles(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let app = common::app(pool.clone());
    create_owner(&app, "ABCD2345EFGH").await;
    let (token, secret) = owner_session(&app, &pool).await;
    let t = Some(token.as_str());
    save_card_row(
        &pool,
        "payout",
        json!({"mode": "toss", "secret_key": "live_sk_zz9876", "security_key": SECURITY_KEY}),
    )
    .await;
    let pro: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO pros (phone, name, team_name, status) VALUES ('01073000101', 'pro', 'team', 'verified') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("pro");
    let batch: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO payouts (pro_id, payout_date, amount, items, provider, state, bank, holder, account_enc, account_last4, provider_transfer_id, attempts)
         VALUES ($1, '2026-11-10', 80000, 1, 'toss', 'sent', 'x', 'x', 'x', '0000', 'tp_open', 1) RETURNING id",
    )
    .bind(pro)
    .fetch_one(&pool)
    .await
    .expect("batch");

    unlock(&app, &pool, &token, &secret, "payout").await;
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/payout", t, Some(json!({"mode": "manual"}))).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("payout_in_transfer")), "{b}");
    let (st, b) = call(&app, "DELETE", "/v1/admin/setup/payout", t, None).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("payout_in_transfer")), "{b}");
    let saved: String = sqlx::query_scalar("SELECT secret FROM integration_settings WHERE section = 'payout'")
        .fetch_one(&pool)
        .await
        .expect("row");
    let saved = api::crypto::decrypt(&common::test_config().data_key, api::crypto::aad::INTEGRATION, &saved).expect("decrypt");
    assert!(saved.contains("live_sk_zz9876"), "the card is unchanged while the transfer is open");

    sqlx::query("UPDATE payouts SET state = 'paid', provider_transfer_id = 'tp_open' WHERE id = $1")
        .bind(batch)
        .execute(&pool)
        .await
        .expect("settle");
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/payout", t, Some(json!({"mode": "manual"}))).await;
    assert_eq!(st, StatusCode::OK, "a settled transfer no longer blocks the switch: {b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn env_webhook_secret_shows_as_set_on_the_saved_portone_card(pool: PgPool) {
    ensure_setup_code(&pool, Some("ABCD2345EFGH"), None).await.expect("code");
    let mut cfg = common::test_config();
    cfg.portone.webhook_secret = Some("whsec_ZW52".into());
    let app = api::routes::router(api::build_state(pool.clone(), cfg).expect("state"));
    create_owner(&app, "ABCD2345EFGH").await;
    let (token, _) = owner_session(&app, &pool).await;
    // The card was saved without a webhook secret: the env one still supplies it.
    save_card_row(&pool, "portone", json!({"store_id": "store-1"})).await;

    let (_, all) = call(&app, "GET", "/v1/admin/setup", Some(token.as_str()), None).await;
    let card = all["integrations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == "portone")
        .expect("portone card");
    let field = card["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "webhook_secret")
        .expect("webhook field");
    assert_eq!(field["set"], true, "{card}");
    assert!(!card.to_string().contains("whsec_ZW52"), "the env secret is never shown");
}
