// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Android App Links and Apple Universal Links over the real router: env fallback, owner-saved values
//! beating env, validation, the dry run, and who may save.

mod common;

use api::{auth::Role, build_state, routes, totp};
use axum::{Router, http::StatusCode};
use chrono::Utc;
use common::{call, call_owner, enrol_owner, staff_token};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const ENV_CERT: &str = "CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD:CD";
const SAVED_CERT: &str = "EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF:EF";
const ENV_TEAM: &str = "ABCDE12345";
const SAVED_TEAM: &str = "ZYXWV98765";
const TOTP_SEED: &[u8] = b"12345678901234567890";

/// Router whose env already carries link values (production-style deploy before any console save).
fn env_app(db: PgPool) -> Router {
    let mut cfg = common::test_config();
    cfg.android_cert_sha256 = vec![ENV_CERT.into()];
    cfg.apple_team_id = Some(ENV_TEAM.into());
    routes::router(build_state(db, cfg).expect("http client"))
}

async fn well_known(app: &Router, path: &str) -> (StatusCode, Value) {
    call(app, "GET", path, None, None).await
}

/// The owner with an enrolled code. Returns (token, staff id).
async fn owner(db: &PgPool) -> (String, Uuid) {
    let (token, id) = staff_token(db, Role::StaffAdmin).await;
    enrol_owner(db, id, TOTP_SEED).await;
    (token, id)
}

/// A save with the current owner code. The spent step is cleared first, so a test may save more than once.
async fn owner_put(app: &Router, db: &PgPool, (token, id): &(String, Uuid), body: Value) -> (StatusCode, Value) {
    sqlx::query("UPDATE staff SET totp_last_step = NULL WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .expect("reset step");
    let code = format!("{:06}", totp::code_at(TOTP_SEED, (Utc::now().timestamp() / totp::STEP_SECS) as u64));
    call_owner(app, "PUT", "/v1/admin/app-links", token, Some(&code), body).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn env_values_serve_both_files_until_the_owner_saves(db: PgPool) {
    let app = env_app(db.clone());
    let (st, android) = well_known(&app, "/.well-known/assetlinks.json").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(android[0]["target"]["sha256_cert_fingerprints"], json!([ENV_CERT]));
    let (st, aasa) = well_known(&app, "/.well-known/apple-app-site-association").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(aasa["webcredentials"]["apps"][0], format!("{ENV_TEAM}.kr.co.dahae.customer"));

    let (token, _) = staff_token(&db, Role::StaffAdmin).await;
    let (st, status) = call(&app, "GET", "/v1/admin/app-links", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(status["source"], "environment");
}

#[sqlx::test(migrations = "../../migrations")]
async fn saved_values_replace_the_env_values(db: PgPool) {
    let app = env_app(db.clone());
    let o = owner(&db).await;
    let (st, saved) = owner_put(
        &app,
        &db,
        &o,
        json!({ "android_cert_sha256": [SAVED_CERT.to_lowercase()], "apple_team_id": SAVED_TEAM }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{saved}");
    assert_eq!(saved["source"], "database");

    let (_, android) = well_known(&app, "/.well-known/assetlinks.json").await;
    assert_eq!(android[0]["target"]["sha256_cert_fingerprints"], json!([SAVED_CERT]));
    let (_, aasa) = well_known(&app, "/.well-known/apple-app-site-association").await;
    assert_eq!(aasa["webcredentials"]["apps"][1], format!("{SAVED_TEAM}.kr.co.dahae.partner"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn clearing_the_values_turns_both_files_off(db: PgPool) {
    let app = env_app(db.clone());
    let o = owner(&db).await;
    let (st, _) = owner_put(&app, &db, &o, json!({ "android_cert_sha256": [], "apple_team_id": null })).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(well_known(&app, "/.well-known/assetlinks.json").await.0, StatusCode::NOT_FOUND);
    assert_eq!(well_known(&app, "/.well-known/apple-app-site-association").await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_values_are_refused_and_the_env_values_stay(db: PgPool) {
    let app = env_app(db.clone());
    let o = owner(&db).await;
    let (st, _) = owner_put(&app, &db, &o, json!({ "android_cert_sha256": ["AA:BB"], "apple_team_id": null })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = owner_put(&app, &db, &o, json!({ "android_cert_sha256": [], "apple_team_id": "short" })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, aasa) = well_known(&app, "/.well-known/apple-app-site-association").await;
    assert_eq!(aasa["webcredentials"]["apps"][0], format!("{ENV_TEAM}.kr.co.dahae.customer"));
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key = 'app_links'")
        .fetch_one(&db)
        .await
        .expect("count");
    assert_eq!(stored, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_shows_the_files_and_saves_nothing(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = staff_token(&db, Role::StaffAdmin).await;
    let (st, dry) = call(
        &app,
        "POST",
        "/v1/admin/app-links/verify",
        Some(&token),
        Some(json!({ "android_cert_sha256": [SAVED_CERT], "apple_team_id": SAVED_TEAM })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(dry["source"], "unsaved");
    assert!(dry["aasa"]["applinks"].is_object());
    assert_eq!(well_known(&app, "/.well-known/assetlinks.json").await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_owner_with_a_code_may_save_and_a_save_is_audited(db: PgPool) {
    let app = env_app(db.clone());
    let body = json!({ "android_cert_sha256": [SAVED_CERT], "apple_team_id": null, "android_packages": ["com.evil.app"] });
    let (consultant, _) = staff_token(&db, Role::StaffConsultant).await;
    let (st, _) = call(&app, "PUT", "/v1/admin/app-links", Some(&consultant), Some(body.clone())).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "consultant");
    // Any other admin could otherwise make its own app the handler of the Naver login callback.
    let (admin, _) = staff_token(&db, Role::StaffAdmin).await;
    let (st, _) = call(&app, "PUT", "/v1/admin/app-links", Some(&admin), Some(body.clone())).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "admin who is not the owner");
    let o = owner(&db).await;
    let (st, err) = call(&app, "PUT", "/v1/admin/app-links", Some(&o.0), Some(body.clone())).await;
    assert_eq!((st, err["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("totp_required")), "{err}");
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key = 'app_links'")
        .fetch_one(&db)
        .await
        .expect("count");
    assert_eq!(stored, 0, "nothing saved without the code");

    let (st, _) = owner_put(&app, &db, &o, body).await;
    assert_eq!(st, StatusCode::OK);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action = 'app_links.update'")
        .fetch_one(&db)
        .await
        .expect("audit");
    assert_eq!(rows, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn saved_app_ids_replace_the_built_in_ones_in_both_files(db: PgPool) {
    let app = env_app(db.clone());
    let o = owner(&db).await;
    let (st, saved) = owner_put(
        &app,
        &db,
        &o,
        json!({
            "android_cert_sha256": [SAVED_CERT], "apple_team_id": SAVED_TEAM,
            "android_packages": ["com.shop.customer", "com.shop.partner"], "ios_bundle_ids": ["com.shop.customer"]
        }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{saved}");
    assert_eq!(saved["android_packages"], json!(["com.shop.customer", "com.shop.partner"]));
    let (_, android) = well_known(&app, "/.well-known/assetlinks.json").await;
    assert_eq!(android[0]["target"]["package_name"], "com.shop.customer");
    assert_eq!(android[1]["target"]["package_name"], "com.shop.partner");
    let (_, aasa) = well_known(&app, "/.well-known/apple-app-site-association").await;
    assert_eq!(aasa["webcredentials"]["apps"], json!([format!("{SAVED_TEAM}.com.shop.customer")]));
    assert_eq!(aasa["applinks"]["details"][0]["appIDs"], aasa["webcredentials"]["apps"]);

    let (st, _) = owner_put(&app, &db, &o, json!({ "android_cert_sha256": [], "android_packages": ["not an id"] })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_row_saved_before_the_id_lists_existed_still_reads(db: PgPool) {
    let app = env_app(db.clone());
    sqlx::query("INSERT INTO settings (key, value) VALUES ('app_links', $1)")
        .bind(json!({ "android_cert_sha256": [SAVED_CERT], "apple_team_id": null }))
        .execute(&db)
        .await
        .expect("old row");
    let (st, android) = well_known(&app, "/.well-known/assetlinks.json").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(android[0]["target"]["package_name"], "kr.co.dahae.customer");
}
