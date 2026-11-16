//! Staff second factor for every role, and step-up TOTP before customer export, deletion and payouts.
//! Over the real router and a migrated database. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{
    auth::{self, Role},
    totp,
};
use axum::http::StatusCode;
use chrono::Utc;
use common::{SECRET, app, call, customer};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PASSWORD: &str = "staff-password-123";

async fn insert_staff(db: &PgPool, email: &str, role: &str) -> Uuid {
    let hash = auth::hash_password(PASSWORD).expect("hash");
    sqlx::query_scalar("INSERT INTO staff (email, name, password_hash, role) VALUES ($1, 'Test', $2, $3) RETURNING id")
        .bind(email)
        .bind(hash)
        .bind(role)
        .fetch_one(db)
        .await
        .expect("insert staff")
}

async fn password_step(app: &axum::Router, email: &str) -> Value {
    let (st, b) = call(app, "POST", "/v1/auth/staff/login", None, Some(json!({ "email": email, "password": PASSWORD }))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    b
}

/// The TOTP code for the current step plus `ahead` steps.
fn code_ahead(secret: &[u8], ahead: u64) -> String {
    let counter = (Utc::now().timestamp() / totp::STEP_SECS) as u64 + ahead;
    format!("{:06}", totp::code_at(secret, counter))
}

/// Enrols TOTP through the password step's challenge. Returns (secret, access token); enrolment uses the current step.
async fn enrol(app: &axum::Router, email: &str) -> (Vec<u8>, String) {
    let mfa = password_step(app, email).await["mfa_token"].as_str().expect("mfa_token").to_string();
    let (st, e) = call(app, "POST", "/v1/auth/staff/mfa/enrol", None, Some(json!({ "mfa_token": mfa }))).await;
    assert_eq!(st, StatusCode::OK, "{e}");
    let secret = totp::decode_secret(e["secret"].as_str().expect("secret")).expect("base32");
    let (st, c) = call(
        app,
        "POST",
        "/v1/auth/staff/mfa/enrol/confirm",
        None,
        Some(json!({ "mfa_token": mfa, "code": code_ahead(&secret, 0) })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    (secret, c["access_token"].as_str().expect("access token").to_string())
}

#[sqlx::test(migrations = "../../migrations")]
async fn consultant_must_enrol_before_any_token(db: PgPool) {
    insert_staff(&db, "consultant@test.local", "consultant").await;
    let b = password_step(&app(db), "consultant@test.local").await;
    assert_eq!(b["mfa_enrol_required"], json!(true), "{b}");
    assert!(b.get("access_token").is_none(), "no tokens before enrolment: {b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn consultant_cannot_look_up_or_export_customers(db: PgPool) {
    let app = app(db.clone());
    let c = customer(&app, &db, "010-8200-0031").await;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO staff (id, email, name, password_hash, role) VALUES ($1, 'scope@test.local', 'Test', 'x', 'consultant')")
        .bind(id)
        .execute(&db)
        .await
        .expect("staff");
    let token = auth::issue_access(SECRET, id, Role::StaffConsultant).expect("token");

    let (st, _) = call(&app, "GET", "/v1/admin/privacy/customers?phone=010-8200-0031", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "phone lookup is admin-only");
    let (st, _) = call(
        &app,
        "GET",
        &format!("/v1/admin/privacy/customers/{}/export", c.user),
        Some(token.as_str()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "export is admin-only");
}

#[sqlx::test(migrations = "../../migrations")]
async fn export_and_delete_need_a_totp_step_up_within_five_minutes(db: PgPool) {
    insert_staff(&db, "boss@test.local", "admin").await;
    let app = app(db.clone());
    let (secret, token) = enrol(&app, "boss@test.local").await;
    let c = customer(&app, &db, "010-8200-0032").await;
    let export = format!("/v1/admin/privacy/customers/{}/export", c.user);
    let delete = format!("/v1/admin/privacy/customers/{}/delete", c.user);

    let (st, b) = call(&app, "GET", &export, Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "export without step-up: {b}");
    assert_eq!(b["error"]["code"], json!("stepup_required"));
    let (st, b) = call(&app, "POST", &delete, Some(token.as_str()), Some(json!({ "phone": "01082000032" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "delete without step-up: {b}");
    assert_eq!(b["error"]["code"], json!("stepup_required"));

    let step_up = "/v1/auth/staff/mfa/step-up";
    let (st, b) = call(&app, "POST", step_up, Some(token.as_str()), Some(json!({ "code": "000000" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "wrong code refused: {b}");
    let (st, b) = call(&app, "POST", step_up, Some(token.as_str()), Some(json!({ "code": code_ahead(&secret, 1) }))).await;
    assert_eq!(st, StatusCode::OK, "{b}");

    let (st, b) = call(&app, "GET", &export, Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "export after step-up: {b}");

    sqlx::query("UPDATE staff SET stepup_at = now() - interval '6 minutes' WHERE email = 'boss@test.local'")
        .execute(&db)
        .await
        .expect("age step-up");
    let (st, b) = call(&app, "GET", &export, Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "a six-minute-old step-up is stale: {b}");
    assert_eq!(b["error"]["code"], json!("stepup_required"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn consultant_cannot_search_customers(db: PgPool) {
    let app = app(db.clone());
    customer(&app, &db, "010-8200-0033").await;
    let id = insert_staff(&db, "search@test.local", "consultant").await;
    let token = auth::issue_access(SECRET, id, Role::StaffConsultant).expect("token");

    let (st, b) = call(&app, "GET", "/v1/admin/customers?q=8200", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "customer search is admin-only: {b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unenrolled_staff_cannot_refresh_a_session(db: PgPool) {
    let id = insert_staff(&db, "old@test.local", "admin").await;
    let refresh = "signed-in-before-the-enrolment-rule";
    sqlx::query(
        "INSERT INTO refresh_tokens (token_hash, subject, role, expires_at, family_id) VALUES ($1, $2, 'staff_admin', now() + interval '1 day', gen_random_uuid())",
    )
    .bind(auth::sha256_hex(refresh.as_bytes()))
    .bind(id)
    .execute(&db)
    .await
    .expect("refresh row");

    let (st, b) = call(&app(db), "POST", "/v1/auth/refresh", None, Some(json!({ "refresh_token": refresh }))).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "no session without a second factor: {b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn payouts_and_bank_reveal_need_a_fresh_step_up(db: PgPool) {
    insert_staff(&db, "pay@test.local", "admin").await;
    let app = app(db.clone());
    let (secret, token) = enrol(&app, "pay@test.local").await;
    let reveal = format!("/v1/admin/settlements/payouts/{}/account", Uuid::now_v7());
    let run = json!({ "date": "2000-01-01" });

    let (st, b) = call(&app, "POST", "/v1/admin/settlements/run-payouts", Some(token.as_str()), Some(run.clone())).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], json!("stepup_required"));
    let (st, b) = call(&app, "GET", &reveal, Some(token.as_str()), None).await;
    assert_eq!(b["error"]["code"], json!("stepup_required"), "bank details: {st} {b}");

    let (st, b) = call(
        &app,
        "POST",
        "/v1/auth/staff/mfa/step-up",
        Some(token.as_str()),
        Some(json!({ "code": code_ahead(&secret, 1) })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = call(&app, "POST", "/v1/admin/settlements/run-payouts", Some(token.as_str()), Some(run)).await;
    assert_ne!(b["error"]["code"], json!("stepup_required"), "{st} {b}");
    let (_, b) = call(&app, "GET", &reveal, Some(token.as_str()), None).await;
    assert_ne!(b["error"]["code"], json!("stepup_required"), "{b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn demoted_admin_with_a_stale_token_is_refused_after_step_up(db: PgPool) {
    insert_staff(&db, "demoted@test.local", "admin").await;
    let app = app(db.clone());
    let (secret, token) = enrol(&app, "demoted@test.local").await;
    let c = customer(&app, &db, "010-8200-0034").await;
    let (st, b) = call(
        &app,
        "POST",
        "/v1/auth/staff/mfa/step-up",
        Some(token.as_str()),
        Some(json!({ "code": code_ahead(&secret, 1) })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");

    sqlx::query("UPDATE staff SET role = 'consultant' WHERE email = 'demoted@test.local'")
        .execute(&db)
        .await
        .expect("demote");
    let export = format!("/v1/admin/privacy/customers/{}/export", c.user);
    let (st, b) = call(&app, "GET", &export, Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], json!("forbidden"), "the stored role decides, not the token: {b}");
}
