//! Cleaner verification: a pro is verified only with a complete checklist, and the checklist, the status
//! and the audit row commit together. The employee profile round-trips, and its emergency contact is
//! encrypted at rest. Same DB and fakes as the other flow tests.

mod common;

use api::auth::Role;
use axum::http::StatusCode;
use common::{PHOTO, call, login, staff_token};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn full_checklist() -> Value {
    json!({ "identity": true, "tax_type": true, "experience": true, "emergency_contact": true })
}

async fn pro_status(pool: &PgPool, pro: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM pros WHERE id = $1")
        .bind(pro)
        .fetch_one(pool)
        .await
        .expect("pro status")
}

async fn set_status(app: &axum::Router, staff: &str, pro: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", &format!("/v1/admin/pros/{pro}/status"), Some(staff), Some(body)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn verifying_without_a_checklist_is_refused(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0201", "partner").await;

    let (st, b) = set_status(&app, &staff, &pro, json!({ "status": "verified" })).await;

    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "checklist_incomplete");
    assert_eq!(pro_status(&pool, pro.parse().expect("id")).await, "pending");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM pro_verifications")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(rows, 0, "nothing is recorded for a refused verification");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_incomplete_checklist_is_refused(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0202", "partner").await;

    let partial = json!({ "identity": true, "tax_type": true, "experience": true, "emergency_contact": false });
    let (st, b) = set_status(&app, &staff, &pro, json!({ "status": "verified", "checklist": partial })).await;

    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "checklist_incomplete");
    assert_eq!(pro_status(&pool, pro.parse().expect("id")).await, "pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_checklist_with_unknown_items_is_refused(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0203", "partner").await;

    let mut list = full_checklist();
    list["made_up"] = json!(true);
    let (st, b) = set_status(&app, &staff, &pro, json!({ "status": "verified", "checklist": list })).await;

    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "invalid_checklist");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_complete_checklist_verifies_and_records_the_decision(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, staff_id) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0204", "partner").await;

    let (st, b) = set_status(&app, &staff, &pro, json!({ "status": "verified", "checklist": full_checklist() })).await;

    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(pro_status(&pool, pro.parse().expect("id")).await, "verified");
    let (decided_by, checklist_ok): (Uuid, bool) = sqlx::query_as(
        "SELECT decided_by, (checklist->>'identity')::boolean AND (checklist->>'emergency_contact')::boolean
         FROM pro_verifications WHERE pro_id = $1",
    )
    .bind(pro.parse::<Uuid>().expect("id"))
    .fetch_one(&pool)
    .await
    .expect("verification row");
    assert_eq!(decided_by, staff_id);
    assert!(checklist_ok, "the ticked items are stored");
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'pro.status' AND target = $1 AND data->'checklist'->>'tax_type' = 'true'")
            .bind(&pro)
            .fetch_one(&pool)
            .await
            .expect("audit");
    assert_eq!(audited, 1, "the decision is audited with the checklist");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_partial_checklist_is_kept_without_verifying(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0205", "partner").await;

    let (st, b) = set_status(&app, &staff, &pro, json!({ "status": "pending", "checklist": { "identity": true } })).await;

    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(pro_status(&pool, pro.parse().expect("id")).await, "pending");
    let stored: Value = sqlx::query_scalar("SELECT checklist FROM pro_verifications WHERE pro_id = $1")
        .bind(pro.parse::<Uuid>().expect("id"))
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(stored, json!({ "identity": true }), "a partial checklist is saved as progress");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_employee_profile_round_trips_and_encrypts_the_emergency_contact(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, staff_id) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0206", "partner").await;
    common::register_photo(&pool, staff_id).await;
    let uri = format!("/v1/admin/pros/{pro}/profile");
    let body = json!({
        "photo_url": PHOTO,
        "bio": "꼼꼼하게 청소합니다.",
        "employment_type": "employee",
        "emergency_contact": "010-1234-5678",
        "hired_on": "2026-03-01"
    });

    let (st, b) = call(&app, "PUT", &uri, Some(staff.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = call(&app, "GET", &uri, Some(staff.as_str()), None).await;

    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["profile"]["bio"], "꼼꼼하게 청소합니다.");
    assert_eq!(b["profile"]["employment_type"], "employee");
    assert_eq!(b["profile"]["hired_on"], "2026-03-01");
    assert_eq!(b["profile"]["emergency_contact"], "010-1234-5678");
    let stored: String = sqlx::query_scalar("SELECT emergency_contact_enc FROM pro_profiles WHERE pro_id = $1")
        .bind(pro.parse::<Uuid>().expect("id"))
        .fetch_one(&pool)
        .await
        .expect("profile row");
    assert!(!stored.contains("1234"), "the number is not stored in the clear");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invalid_emergency_contact_or_employment_type_is_refused(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0207", "partner").await;
    let uri = format!("/v1/admin/pros/{pro}/profile");

    let (st, b) = call(&app, "PUT", &uri, Some(staff.as_str()), Some(json!({ "emergency_contact": "call me" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "invalid_emergency_contact");
    let (st, b) = call(&app, "PUT", &uri, Some(staff.as_str()), Some(json!({ "employment_type": "owner" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "invalid_employment_type");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_photo_must_have_been_uploaded(pool: PgPool) {
    let app = common::app(pool.clone());
    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, pro) = login(&app, "010-5600-0209", "partner").await;
    let uri = format!("/v1/admin/pros/{pro}/profile");

    let (st, b) = call(&app, "PUT", &uri, Some(staff.as_str()), Some(json!({ "photo_url": PHOTO }))).await;

    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(b["error"]["code"], "invalid_photo");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_profile_is_console_only(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, pro) = login(&app, "010-5600-0208", "partner").await;

    let (st, _) = call(&app, "GET", &format!("/v1/admin/pros/{pro}/profile"), None, None).await;

    assert_eq!(st, StatusCode::UNAUTHORIZED);
}
