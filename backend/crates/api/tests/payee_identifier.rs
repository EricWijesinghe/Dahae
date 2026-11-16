//! Payee identifier (주민등록번호 or 사업자등록번호) for the 지급명세서: stored sealed, shown masked in profile reads,
//! revealed only on request, and every set or reveal is audit-logged. A profile edit that omits it keeps it.

mod common;

use api::auth::Role;
use axum::http::StatusCode;
use common::{call, login, staff_token};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const RRN: &str = "900101-1234568";
const RRN_DIGITS: &str = "9001011234568";

async fn audit_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .expect("audit count")
}

#[sqlx::test(migrations = "../../migrations")]
async fn payee_identifier_is_sealed_masked_and_revealed_only_on_request(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0301", "partner").await;
    let uri = format!("/v1/admin/pros/{pro}/profile");

    let (st, b) = call(&app, "PUT", &uri, Some(staff.as_str()), Some(json!({ "payee_identifier": RRN }))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = call(&app, "GET", &uri, Some(staff.as_str()), None).await;

    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["profile"]["payee_identifier"], json!({ "kind": "rrn", "masked": "*********4568" }));
    assert!(!b.to_string().contains(RRN_DIGITS), "a profile read never carries the digits");
    let stored: Option<String> = sqlx::query_scalar("SELECT payee_identifier_enc FROM pro_profiles WHERE pro_id = $1")
        .bind(pro.parse::<Uuid>().expect("id"))
        .fetch_one(&pool)
        .await
        .expect("profile row");
    assert!(stored.is_some_and(|s| !s.contains(RRN_DIGITS)), "stored sealed");
    assert_eq!(audit_count(&pool, "pro.payee_identifier.set").await, 1);

    let reveal = format!("/v1/admin/pros/{pro}/payee-identifier/reveal");
    let (st, b) = call(&app, "POST", &reveal, Some(staff.as_str()), None).await;

    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["identifier"], RRN_DIGITS);
    assert_eq!(audit_count(&pool, "pro.payee_identifier.reveal").await, 1, "every reveal is audited");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_edit_without_the_identifier_keeps_it(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0302", "partner").await;
    let uri = format!("/v1/admin/pros/{pro}/profile");
    call(&app, "PUT", &uri, Some(staff.as_str()), Some(json!({ "payee_identifier": RRN }))).await;

    let (st, b) = call(&app, "PUT", &uri, Some(staff.as_str()), Some(json!({ "bio": "꼼꼼하게 청소합니다." }))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let reveal = format!("/v1/admin/pros/{pro}/payee-identifier/reveal");
    let (st, b) = call(&app, "POST", &reveal, Some(staff.as_str()), None).await;

    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["identifier"], RRN_DIGITS, "the bio edit did not erase the identifier");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invalid_or_missing_payee_identifier_is_refused(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0303", "partner").await;
    let uri = format!("/v1/admin/pros/{pro}/profile");

    let (st, b): (StatusCode, Value) = call(&app, "PUT", &uri, Some(staff.as_str()), Some(json!({ "payee_identifier": "9001011234567" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "invalid_payee_identifier");
    let reveal = format!("/v1/admin/pros/{pro}/payee-identifier/reveal");
    let (st, b) = call(&app, "POST", &reveal, Some(staff.as_str()), None).await;

    assert_eq!(st, StatusCode::NOT_FOUND, "nothing stored, nothing to reveal: {b}");
    assert_eq!(audit_count(&pool, "pro.payee_identifier.reveal").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_consultant_cannot_write_the_payee_identifier(pool: PgPool) {
    let app = common::app(pool.clone());
    let (consultant, _) = staff_token(&pool, Role::StaffConsultant).await;
    let (_, pro) = login(&app, "010-5600-0304", "partner").await;
    let uri = format!("/v1/admin/pros/{pro}/profile");

    let (st, b) = call(&app, "PUT", &uri, Some(consultant.as_str()), Some(json!({ "payee_identifier": RRN }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(audit_count(&pool, "pro.payee_identifier.set").await, 0, "a refused write leaves no set row");
    let stored: Option<String> = sqlx::query_scalar("SELECT payee_identifier_enc FROM pro_profiles WHERE pro_id = $1")
        .bind(pro.parse::<Uuid>().expect("id"))
        .fetch_optional(&pool)
        .await
        .expect("profile lookup")
        .flatten();
    assert_eq!(stored, None, "nothing was stored");
}
