// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Former service domains over the real router and a migrated database: the move is recorded once at start,
//! Caddy's certificate question answers for aliases only, retiring needs the owner code, CORS lists the aliases,
//! and the checklist ticks stick. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, crypto, domains, routes, totp};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use chrono::Utc;
use common::{call, staff_token, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;

const TOTP_SEED: &[u8] = b"12345678901234567890";

fn app_at(db: PgPool, base: &str) -> (Router, api::config::Config) {
    let mut cfg = test_config();
    cfg.public_base_url = base.into();
    (routes::router(api::build_state(db, cfg.clone()).expect("state")), cfg)
}

async fn ask(app: &Router, host: &str) -> StatusCode {
    call(app, "GET", &format!("/internal/tls-ask?domain={host}"), None, None).await.0
}

async fn owner_token(db: &PgPool) -> String {
    let (token, id) = staff_token(db, Role::StaffAdmin).await;
    let enc = crypto::encrypt(&test_config().data_key, crypto::aad::STAFF_TOTP, &totp::encode_secret(TOTP_SEED));
    sqlx::query("UPDATE staff SET totp_secret = $2, totp_enabled_at = now(), recovery_confirmed_at = now(), is_owner = true WHERE id = $1")
        .bind(id)
        .bind(enc)
        .execute(db)
        .await
        .expect("enrol totp");
    token
}

fn code_now() -> String {
    let step = Utc::now().timestamp().div_euclid(totp::STEP_SECS);
    format!("{:06}", totp::code_at(TOTP_SEED, step as u64))
}

async fn retire(app: &Router, token: &str, host: &str, totp: Option<&str>) -> StatusCode {
    let mut req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/admin/domains/{host}"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(code) = totp {
        req = req.header("x-owner-totp", code);
    }
    let mut request = req.body(Body::empty()).expect("request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    app.clone().oneshot(request).await.expect("router").status()
}

/// Starts the API on `old`, then on `new`, the way a domain move restarts it.
async fn moved(db: &PgPool) -> (Router, api::config::Config) {
    let (_, mut cfg) = app_at(db.clone(), "https://old.co.kr");
    domains::startup(db, &mut cfg).await;
    let (_, mut cfg) = app_at(db.clone(), "https://new.co.kr");
    domains::startup(db, &mut cfg).await;
    (routes::router(api::build_state(db.clone(), cfg.clone()).expect("state")), cfg)
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_move_is_recorded_once_and_the_old_domain_becomes_an_alias(db: PgPool) {
    let (_, cfg) = moved(&db).await;
    let mut again = cfg.clone();
    domains::startup(&db, &mut again).await;
    let d = domains::load(&db).await.expect("row");
    assert_eq!(d.current.as_deref(), Some("new.co.kr"));
    assert_eq!(
        d.aliases.iter().map(|a| a.host.as_str()).collect::<Vec<_>>(),
        vec!["old.co.kr"],
        "a restart adds nothing"
    );
    assert_eq!(again.allowed_origins.iter().filter(|o| *o == "https://old.co.kr").count(), 1, "listed once");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_dev_address_records_nothing(db: PgPool) {
    let mut cfg = test_config();
    cfg.public_base_url = "http://127.0.0.1:8080".into();
    let before = cfg.allowed_origins.clone();
    domains::startup(&db, &mut cfg).await;
    assert_eq!(cfg.allowed_origins, before);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key = 'domains'")
        .fetch_one(&db)
        .await
        .expect("count");
    assert_eq!(rows, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tls_ask_answers_for_aliases_only(db: PgPool) {
    let (app, _) = moved(&db).await;
    assert_eq!(ask(&app, "old.co.kr").await, StatusCode::OK);
    assert_eq!(ask(&app, "www.old.co.kr").await, StatusCode::OK);
    assert_eq!(ask(&app, "new.co.kr").await, StatusCode::NOT_FOUND, "current has its own site block");
    assert_eq!(ask(&app, "evil.example").await, StatusCode::NOT_FOUND);
    assert_eq!(ask(&app, "sub.old.co.kr").await, StatusCode::NOT_FOUND);

    let owner = owner_token(&db).await;
    assert_eq!(retire(&app, &owner, "old.co.kr", Some(&code_now())).await, StatusCode::OK);
    assert_eq!(ask(&app, "old.co.kr").await, StatusCode::NOT_FOUND, "retired");
    assert_eq!(ask(&app, "www.old.co.kr").await, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn retiring_needs_the_owner_code_and_an_alias(db: PgPool) {
    let (app, _) = moved(&db).await;
    let (admin, _) = staff_token(&db, Role::StaffAdmin).await;
    assert_eq!(retire(&app, &admin, "old.co.kr", Some("123456")).await, StatusCode::FORBIDDEN, "not the owner");
    let owner = owner_token(&db).await;
    assert_eq!(retire(&app, &owner, "old.co.kr", None).await, StatusCode::BAD_REQUEST, "no code");
    assert_eq!(retire(&app, &owner, "old.co.kr", Some("000000")).await, StatusCode::BAD_REQUEST, "wrong code");
    let code = code_now();
    assert_eq!(
        retire(&app, &owner, "new.co.kr", Some(&code)).await,
        StatusCode::NOT_FOUND,
        "current is not an alias"
    );
    assert_eq!(
        retire(&app, &owner, "OLD.co.kr", Some(&code)).await,
        StatusCode::OK,
        "the code was not spent on the refusal"
    );
    let d = domains::load(&db).await.expect("row");
    assert!(d.aliases.is_empty());
    assert_eq!(d.retired[0].host, "old.co.kr");
    let audited: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action = 'domains.retire'")
        .fetch_one(&db)
        .await
        .expect("audit");
    assert_eq!(audited, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cors_lets_the_old_origin_in(db: PgPool) {
    let (app, cfg) = moved(&db).await;
    assert!(cfg.allowed_origins.contains(&"https://old.co.kr".to_string()));
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/v1/config")
        .header(header::ORIGIN, "https://old.co.kr")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("router");
    assert_eq!(
        resp.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).and_then(|v| v.to_str().ok()),
        Some("https://old.co.kr")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_shows_the_checklist_and_ticks_stick(db: PgPool) {
    let (app, _) = moved(&db).await;
    let (admin, _) = staff_token(&db, Role::StaffAdmin).await;
    let (st, doc) = call(&app, "GET", "/v1/admin/domains", Some(&admin), None).await;
    assert_eq!(st, StatusCode::OK, "{doc}");
    assert_eq!(doc["current"], "new.co.kr");
    assert_eq!(doc["aliases"][0]["host"], "old.co.kr");
    let item = |doc: &Value, id: &str| doc["checklist"].as_array().expect("list").iter().find(|i| i["id"] == id).cloned().expect(id);
    assert_eq!(item(&doc, "portone_webhook")["value"], "https://new.co.kr/v1/webhooks/portone");
    assert_eq!(item(&doc, "portone_webhook")["done"], false);

    let (st, doc) = call(
        &app,
        "PUT",
        "/v1/admin/domains/checklist/portone_webhook",
        Some(&admin),
        Some(json!({ "done": true })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(item(&doc, "portone_webhook")["done"], true);
    let (st, _) = call(&app, "PUT", "/v1/admin/domains/checklist/made_up", Some(&admin), Some(json!({ "done": true }))).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (consultant, _) = staff_token(&db, Role::StaffConsultant).await;
    let (st, _) = call(&app, "GET", "/v1/admin/domains", Some(&consultant), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}
