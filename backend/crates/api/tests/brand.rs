// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Brand profile v2 over the real router: slot checks, `X-Brand-Version`, unsigned brand files, `/v1/brand`,
//! manifests, `/v1/config` additions, and the operator settings that moved into the console.

mod common;

use api::auth::Role;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::ConnectInfo,
    http::{HeaderMap, Request, StatusCode, header},
};
use common::{call, call_owner};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

const SEED: &[u8] = b"brand-test-owner-seed-0123456789";

async fn raw(app: &Router, method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, HeaderMap, Bytes) {
    let mut req = Request::builder().method(method).uri(uri);
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
    let mut request = req.body(payload).expect("request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let resp = app.clone().oneshot(request).await.expect("router");
    let (status, headers) = (resp.status(), resp.headers().clone());
    (status, headers, axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body"))
}

fn brand_header(h: &HeaderMap) -> String {
    h.get("x-brand-version").and_then(|v| v.to_str().ok()).expect("X-Brand-Version").to_string()
}

/// A random upload name, its file in the test upload dir (a PNG header of `w`x`h`) and, with `row`, its uploads row.
async fn png_upload(db: &PgPool, w: u32, h: u32, row: bool) -> String {
    let name = format!("{}.png", Uuid::now_v7().simple());
    let mut bytes = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&w.to_be_bytes());
    bytes.extend_from_slice(&h.to_be_bytes());
    let dir = common::test_config().upload_dir;
    std::fs::create_dir_all(&dir).expect("upload dir");
    std::fs::write(dir.join(&name), &bytes).expect("png");
    if row {
        sqlx::query("INSERT INTO uploads (owner_role, owner_id, file, bytes) VALUES ('staff', $1, $2, 24)")
            .bind(Uuid::now_v7())
            .bind(&name)
            .execute(db)
            .await
            .expect("uploads row");
    }
    format!("/files/{name}")
}

async fn put_business(app: &Router, admin: &str, body: Value) -> (StatusCode, Value) {
    call(app, "PUT", "/v1/admin/setup/business", Some(admin), Some(body)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn brand_slots_are_checked_against_uploads_and_png_size(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let good = png_upload(&pool, 192, 192, true).await;
    let wrong_size = png_upload(&pool, 200, 192, true).await;
    let no_row = png_upload(&pool, 192, 192, false).await;
    for (assets, why) in [
        (json!({"icon_192": wrong_size}), "wrong IHDR size"),
        (json!({"icon_192": no_row}), "no uploads row"),
        (json!({"favicon_32": good}), "192 px in a 32 px slot"),
        (json!({"nope": good}), "unknown slot"),
        (json!({"icon_192": "https://evil.example/x.png"}), "foreign URL"),
    ] {
        let (st, b) = put_business(&app, &admin, json!({"name": "반짝", "brand_assets": assets})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{why}: {b}");
        assert_eq!(b["error"]["code"], "invalid_brand_asset", "{why}: {b}");
    }
    let (st, b) = put_business(&app, &admin, json!({"name": "반짝", "short_name": "반짝", "brand_assets": {"icon_192": good}})).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["brand_assets"]["icon_192"], good.as_str());
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_response_carries_the_brand_version_and_it_changes_on_save(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let (_, h, _) = raw(&app, "GET", "/v1/config", None, None).await;
    let before = brand_header(&h);
    assert_eq!(before.len(), 16);
    for uri in ["/health", "/v1/me", "/v1/does-not-exist"] {
        let (_, h, _) = raw(&app, "GET", uri, None, None).await;
        assert_eq!(brand_header(&h), before, "{uri}");
    }
    let (st, b) = put_business(&app, &admin, json!({"name": "반짝"})).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, h, body) = raw(&app, "GET", "/v1/me", None, None).await;
    let after = brand_header(&h);
    assert_ne!(after, before, "the saving node serves the new version at once");
    let (_, _, cfg) = raw(&app, "GET", "/v1/config", None, None).await;
    let cfg: Value = serde_json::from_slice(&cfg).expect("json");
    assert_eq!(cfg["brand_version"], after.as_str(), "{body:?}");
    let (_, brand) = call(&app, "GET", "/v1/brand", None, None).await;
    assert_eq!(brand["version"], after.as_str());
}

#[sqlx::test(migrations = "../../migrations")]
async fn fill_colour_is_optional_reaches_the_apps_and_moves_the_brand_version(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let (st, b) = put_business(&app, &admin, json!({"name": "우집사", "primary_color": "#00838F"})).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert!(b.get("fill_color").is_none(), "unset: apps fill with the primary colour");
    let (_, before) = call(&app, "GET", "/v1/brand", None, None).await;
    // Light sky blue fails white text (2.3:1) but is still accepted: the apps pick a dark label for it.
    let body = json!({"name": "우집사", "primary_color": "#00838F", "fill_color": "#00bcd4"});
    let (st, b) = put_business(&app, &admin, body).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["fill_color"], "#00BCD4");
    let (_, brand) = call(&app, "GET", "/v1/brand", None, None).await;
    assert_eq!(brand["fill_color"], "#00BCD4");
    assert_eq!(brand["primary_color"], "#00838F", "links keep the primary colour");
    assert_ne!(brand["version"], before["version"], "apps refetch the brand");
    let (_, _, cfg) = raw(&app, "GET", "/v1/config", None, None).await;
    let cfg: Value = serde_json::from_slice(&cfg).expect("json");
    assert_eq!(cfg["brand_version"], brand["version"]);

    let (_, setup) = call(&app, "GET", "/v1/admin/setup", Some(&admin), None).await;
    assert_eq!(setup["business"]["fill_color"], "#00BCD4", "the console form loads it back");
    let (st, b) = put_business(&app, &admin, json!({"name": "우집사", "fill_color": "sky"})).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "invalid_color");
    let (st, b) = put_business(&app, &admin, json!({"name": "우집사", "fill_color": null})).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert!(b.get("fill_color").is_none(), "null clears it back to the primary colour");
}

#[sqlx::test(migrations = "../../migrations")]
async fn current_brand_files_are_public_and_others_stay_signed(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let logo = png_upload(&pool, 300, 100, true).await;
    let icon = png_upload(&pool, 512, 512, true).await;
    let other = png_upload(&pool, 10, 10, true).await;
    let (st, b) = put_business(&app, &admin, json!({"name": "반짝", "logo_url": logo, "brand_assets": {"icon_512": icon}})).await;
    assert_eq!(st, StatusCode::OK, "{b}");

    for path in [&logo, &icon] {
        let (st, h, _) = raw(&app, "GET", &format!("/v1{path}"), None, None).await;
        assert_eq!(st, StatusCode::OK, "{path}");
        assert_eq!(h[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }
    let (st, _, _) = raw(&app, "GET", &format!("/v1{other}"), None, None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "a non-brand upload needs a signature");

    let (_, brand) = call(&app, "GET", "/v1/brand", None, None).await;
    let signed = brand["logo_url"].as_str().expect("logo_url");
    assert!(
        signed.contains("/v1/files/") && signed.contains("?exp=") && signed.contains("&sig="),
        "old apps get a signed URL: {signed}"
    );
    assert_eq!(brand["assets"]["logo"], format!("{}/v1{logo}", common::BASE_URL));
    assert_eq!(brand["assets"]["icon_512"], format!("{}/v1{icon}", common::BASE_URL));
    let uri = signed.strip_prefix(common::BASE_URL).expect("base");
    let (st, h, _) = raw(&app, "GET", uri, None, None).await;
    assert_eq!(st, StatusCode::OK, "signed URLs work as before");
    assert_eq!(h[header::CACHE_CONTROL], "private, max-age=3600");

    let new_logo = png_upload(&pool, 300, 100, true).await;
    let (st, b) = put_business(&app, &admin, json!({"name": "반짝", "logo_url": new_logo})).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, _, _) = raw(&app, "GET", &format!("/v1{logo}"), None, None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "the old logo is no longer public");
    let (st, _, _) = raw(&app, "GET", &format!("/v1{new_logo}"), None, None).await;
    assert_eq!(st, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn manifests_come_from_the_profile(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let (st, h, body) = raw(&app, "GET", "/v1/brand/manifest/customer", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "application/manifest+json");
    assert_eq!(h[header::CACHE_CONTROL], "no-cache");
    let m: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(m["name"], "다해");
    assert_eq!(m["icons"][0]["src"], "/app/icons/Icon-192.png", "built-in icons without brand files");

    let icon = png_upload(&pool, 192, 192, true).await;
    let (st, b) = put_business(
        &app,
        &admin,
        json!({"name": "반짝", "primary_color": "#112233", "brand_assets": {"icon_192": icon}}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    for app_name in ["site", "customer", "console"] {
        let (st, _, body) = raw(&app, "GET", &format!("/v1/brand/manifest/{app_name}"), None, None).await;
        assert_eq!(st, StatusCode::OK, "{app_name}");
        let m: Value = serde_json::from_slice(&body).expect("json");
        assert!(m["name"].as_str().is_some_and(|n| n.starts_with("반짝")), "{app_name}: {m}");
        assert_eq!(m["theme_color"], "#112233", "{app_name}");
        assert_eq!(m["icons"][0]["src"], format!("/v1{icon}"), "{app_name}");
    }
    let (st, _, _) = raw(&app, "GET", "/v1/brand/manifest/partner", None, None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn config_carries_discovery_fields_and_live_providers(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, cfg) = call(&app, "GET", "/v1/config", None, None).await;
    assert_eq!(cfg["api_base"], common::BASE_URL);
    let id = cfg["install_id"].as_str().expect("install_id");
    assert!(id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()), "{id}");
    assert_eq!(cfg["providers"], json!([]), "no sign-in card saved in the test config");
    let (_, again) = call(&app, "GET", "/v1/config", None, None).await;
    assert_eq!(again["install_id"], id, "stable");
}

#[sqlx::test(migrations = "../../migrations")]
async fn english_texts_get_the_english_name(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    put_business(&app, &admin, json!({"name": "반짝", "name_en": "Shine"})).await;
    for (locale, want) in [("en", "Shine"), ("ko", "반짝")] {
        let (st, p) = call(
            &app,
            "POST",
            "/v1/admin/notify-templates/preview",
            Some(admin.as_str()),
            Some(json!({ "kind": "waitlist.offer", "locale": locale, "title": "{업체명}", "body": "x" })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{p}");
        assert_eq!(p["title"], want, "{locale}");
    }
    assert_eq!(api::routes::setup::brand::brand_name_for(&pool, "en").await, "Shine");
    assert_eq!(api::routes::setup::brand::short_brand(&pool).await, "반짝");
}

#[sqlx::test(migrations = "../../migrations")]
async fn staff_authenticator_label_uses_the_business_name(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    put_business(&app, &admin, json!({"name": "반짝"})).await;
    sqlx::query("INSERT INTO staff (id, email, name, password_hash, role) VALUES ($1, 'new@test.local', 'n', $2, 'admin')")
        .bind(Uuid::now_v7())
        .bind(api::auth::hash_password("new-password-123").expect("hash"))
        .execute(&pool)
        .await
        .expect("staff");
    let (st, b) = call(
        &app,
        "POST",
        "/v1/auth/staff/login",
        None,
        Some(json!({"email": "new@test.local", "password": "new-password-123"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, e) = call(&app, "POST", "/v1/auth/staff/mfa/enrol", None, Some(json!({ "mfa_token": b["mfa_token"] }))).await;
    assert_eq!(st, StatusCode::OK, "{e}");
    let uri = e["otpauth_uri"].as_str().expect("uri");
    assert!(uri.contains("%EB%B0%98%EC%A7%9D%20staff"), "issuer is `반짝 staff`: {uri}");
}

fn code() -> String {
    format!(
        "{:06}",
        api::totp::code_at(SEED, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    )
}

async fn owner(pool: &PgPool) -> String {
    let (token, id) = common::staff_token(pool, Role::StaffAdmin).await;
    common::enrol_owner(pool, id, SEED).await;
    token
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_payout_switch_needs_the_owner_code_and_opens_the_gate(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let (st, _) = call_owner(&app, "PUT", "/v1/admin/setup/payout/switch", &admin, Some("000000"), json!({"on": true})).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "not the owner");
    let token = owner(&pool).await;
    let (st, b) = call_owner(&app, "PUT", "/v1/admin/setup/payout/switch", &token, None, json!({"on": true})).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("totp_required")), "{b}");
    assert!(!api::integrations::toss_switch_on(&pool).await.expect("read"));
    let (st, b) = call_owner(&app, "PUT", "/v1/admin/setup/payout/switch", &token, Some(&code()), json!({"on": true})).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["on"], true);
    assert!(
        api::integrations::toss_switch_on(&pool).await.expect("read"),
        "switch on, env off: the gate is open"
    );
    let (st, b) = call(&app, "GET", "/v1/admin/setup", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["payout_toss_switch"], json!({"on": true, "env": false}), "the console reads the saved switch");
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'setup.payout_switch'")
        .fetch_one(&pool)
        .await
        .expect("audit");
    assert_eq!(audited, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn business_hours_round_trip_with_validation(pool: PgPool) {
    let app = common::app(pool.clone());
    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let uri = "/v1/admin/setup/business-hours";
    let (st, b) = call(
        &app,
        "PUT",
        uri,
        Some(&admin),
        Some(json!({"weekdays": [5, 0], "open": "10:00", "close": "09:00"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    let (st, b) = call(
        &app,
        "PUT",
        uri,
        Some(&admin),
        Some(json!({"weekdays": [5, 0], "open": "10:00", "close": "20:00"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, got) = call(&app, "GET", uri, Some(&admin), None).await;
    assert_eq!(got, json!({"weekdays": [0, 5], "open": "10:00", "close": "20:00"}));
    let (customer, _) = common::staff_token(&pool, Role::StaffConsultant).await;
    let (st, _) = call(&app, "PUT", uri, Some(&customer), Some(got)).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "admin only");
}

#[sqlx::test(migrations = "../../migrations")]
async fn store_build_notice_lists_changes_until_the_owner_acks(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = owner(&pool).await;
    let (_, setup) = call(&app, "GET", "/v1/admin/setup", Some(&token), None).await;
    assert_eq!(
        setup["store_build"]["changed"],
        json!([]),
        "no alarm on the first read: {}",
        setup["store_build"]
    );
    put_business(&app, &token, json!({"name": "반짝", "primary_color": "#112233"})).await;
    let (_, setup) = call(&app, "GET", "/v1/admin/setup", Some(&token), None).await;
    assert_eq!(setup["store_build"]["changed"], json!(["name", "color"]));

    let (admin, _) = common::staff_token(&pool, Role::StaffAdmin).await;
    let (st, _) = call(&app, "POST", "/v1/admin/setup/store-build/ack", Some(&admin), Some(json!({}))).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "owner only");
    let (st, b) = call(
        &app,
        "POST",
        "/v1/admin/setup/store-build/ack",
        Some(&token),
        Some(json!({"build_url": "http://x"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    let url = "https://github.com/example/repo/actions/workflows/store-apps.yml";
    let (st, b) = call(&app, "POST", "/v1/admin/setup/store-build/ack", Some(&token), Some(json!({"build_url": url}))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, setup) = call(&app, "GET", "/v1/admin/setup", Some(&token), None).await;
    assert_eq!(setup["store_build"]["changed"], json!([]));
    assert_eq!(setup["store_build"]["build_url"], url);
    assert!(setup["store_build"]["acked_at"].is_string());
}
