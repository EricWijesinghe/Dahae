//! Staff authorization: exports need a live admin and a fresh step-up, copy-channel resets stay off
//! accounts with no second factor, deactivation ends the staff session, and OTP sends are not cut off
//! globally by one busy client.

mod common;

use api::auth::Role;
use axum::http::StatusCode;
use common::{call, staff_token};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PERIOD: &str = "period=custom&from=2026-03-01&to=2026-03-31";

/// An active staff row with a password, not enrolled in TOTP (the reset target in the findings).
async fn plain_staff(db: &PgPool, role: &str, phone: Option<&str>) -> Uuid {
    sqlx::query_scalar("INSERT INTO staff (email, name, password_hash, role, phone) VALUES ($1, 'Target', 'x', $2, $3) RETURNING id")
        .bind(format!("{}@test.local", Uuid::now_v7()))
        .bind(role)
        .bind(phone)
        .fetch_one(db)
        .await
        .expect("staff row")
}

#[sqlx::test(migrations = "../../migrations")]
async fn exports_need_a_live_admin_and_a_fresh_step_up(db: PgPool) {
    let app = common::app(db.clone());
    let (token, id) = staff_token(&db, Role::StaffAdmin).await;
    let uris = [
        format!("/v1/admin/finance/export/payroll?{PERIOD}"),
        format!("/v1/admin/finance/export/withholding?{PERIOD}"),
        format!("/v1/admin/finance/export/ledger?{PERIOD}"),
        format!("/v1/admin/finance/export/settlements?{PERIOD}"),
        format!("/v1/admin/finance/export/monthly?{PERIOD}"),
        "/v1/admin/finance/export/vat?year=2026&quarter=1".to_string(),
        format!("/v1/admin/finance/export/tax-invoices?{PERIOD}"),
    ];
    // No step-up in the last few minutes: every export refuses.
    sqlx::query("UPDATE staff SET stepup_at = NULL WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await
        .expect("stale");
    for uri in &uris {
        let (st, b) = call(&app, "GET", uri, Some(token.as_str()), None).await;
        assert_eq!(
            (st, b["error"]["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("stepup_required")),
            "{uri}: {b}"
        );
    }
    // A stepped-up admin demoted to consultant: the stored role decides, not the token claim.
    sqlx::query("UPDATE staff SET stepup_at = now(), role = 'consultant' WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await
        .expect("demote");
    for uri in &uris {
        let (st, b) = call(&app, "GET", uri, Some(token.as_str()), None).await;
        assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("forbidden")), "{uri}: {b}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn copy_channel_reset_is_refused_for_an_account_with_no_second_factor(db: PgPool) {
    let app = common::app(db.clone());
    let (admin, _) = staff_token(&db, Role::StaffAdmin).await;
    let target = plain_staff(&db, "consultant", Some("01012345678")).await;
    let uri = format!("/v1/admin/staff/{target}/password-reset");
    let (st, b): (StatusCode, Value) = call(&app, "POST", &uri, Some(admin.as_str()), Some(json!({ "channel": "copy" }))).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("sms_required")), "{b}");
    let (st, b) = call(&app, "POST", &uri, Some(admin.as_str()), Some(json!({ "channel": "sms" }))).await;
    assert_eq!(st, StatusCode::OK, "the code goes to the staff member's own phone: {b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sms_reset_with_an_admin_supplied_phone_is_refused_for_an_account_with_no_second_factor(db: PgPool) {
    let app = common::app(db.clone());
    let (admin, _) = staff_token(&db, Role::StaffAdmin).await;
    let target = plain_staff(&db, "consultant", None).await;
    let uri = format!("/v1/admin/staff/{target}/password-reset");
    let (st, b): (StatusCode, Value) = call(
        &app,
        "POST",
        &uri,
        Some(admin.as_str()),
        Some(json!({ "channel": "sms", "phone": "010-1234-5678" })),
    )
    .await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("sms_required")), "{b}");
    let stored: Option<String> = sqlx::query_scalar("SELECT phone FROM staff WHERE id = $1")
        .bind(target)
        .fetch_one(&db)
        .await
        .expect("phone");
    assert_eq!(stored, None, "a refused reset saves no phone number");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivating_staff_ends_their_access_token(db: PgPool) {
    let app = common::app(db.clone());
    let (admin, _) = staff_token(&db, Role::StaffAdmin).await;
    let (other, other_id) = staff_token(&db, Role::StaffAdmin).await;
    let (st, _) = call(&app, "GET", "/v1/auth/sessions", Some(other.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "token works before deactivation");
    let (st, b) = call(
        &app,
        "POST",
        &format!("/v1/admin/staff/{other_id}/active"),
        Some(admin.as_str()),
        Some(json!({ "active": false })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = call(&app, "POST", "/v1/realtime/ticket", Some(other.as_str()), None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "the deactivated session's token is dead at once: {b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn otp_sends_are_not_cut_off_for_every_number_by_a_global_count(db: PgPool) {
    // 3000 codes for other numbers in the last hour: the old global ceiling refused every sign-in.
    sqlx::query(
        "INSERT INTO otp_codes (phone, code_hash, expires_at) SELECT '0108' || lpad(g::text, 7, '0'), 'x', now() + interval '3 minutes' FROM generate_series(1, 3000) g",
    )
    .execute(&db)
    .await
    .expect("seed codes");
    let app = common::app(db.clone());
    let (st, b) = call(&app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": "010-5555-0101" }))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn otp_sends_stop_at_the_hard_ceiling(db: PgPool) {
    let ceiling = api::ops_alerts::OTP_GLOBAL_CEILING_PER_HOUR;
    sqlx::query(
        "INSERT INTO otp_codes (phone, code_hash, expires_at) SELECT '0108' || lpad(g::text, 7, '0'), 'x', now() + interval '3 minutes' FROM generate_series(1, $1::int) g",
    )
    .bind(ceiling as i32)
    .execute(&db)
    .await
    .expect("seed codes");
    let app = common::app(db.clone());
    let (st, b) = call(&app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": "010-5555-0102" }))).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("rate_limited")), "{b}");
    // Numbers that already have an account keep signing in: the ceiling stops new sign-ups, not the service.
    sqlx::query("INSERT INTO users (phone, name) VALUES ('01055550103', 'c')")
        .execute(&db)
        .await
        .expect("customer");
    sqlx::query("INSERT INTO pros (phone, name) VALUES ('01055550104', 'p')")
        .execute(&db)
        .await
        .expect("pro");
    for phone in ["010-5555-0103", "010-5555-0104"] {
        let (st, b) = call(&app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": phone }))).await;
        assert_eq!(st, StatusCode::OK, "{phone}: {b}");
    }
}

/// Sets the hourly sign-in code counter of `key` (an address or a network) as if `hits` codes had gone out.
async fn spend_otp_window(db: &PgPool, key: &str, hits: i64) {
    sqlx::query("INSERT INTO rate_limits (key, window_start, hits) VALUES ($1, now(), $2)")
        .bind(key)
        .bind(hits as i32)
        .execute(db)
        .await
        .expect("counter");
}

#[sqlx::test(migrations = "../../migrations")]
async fn otp_sends_are_capped_per_address_and_per_network(db: PgPool) {
    let app = common::app(db.clone());
    let (st, b) = call(&app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": "010-5555-0201" }))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let hits: i32 = sqlx::query_scalar("SELECT hits FROM rate_limits WHERE key = 'otp_net:127.0.0.0/24'")
        .fetch_one(&db)
        .await
        .expect("network counter");
    assert_eq!(hits, 1, "the code counts against the /24");

    // Other addresses in the same /24 spent the network's hour.
    sqlx::query("DELETE FROM rate_limits").execute(&db).await.expect("reset");
    spend_otp_window(&db, "otp_net:127.0.0.0/24", api::ops_alerts::OTP_MAX_PER_NETWORK_PER_HOUR).await;
    let (st, b) = call(&app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": "010-5555-0202" }))).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "{b}");

    sqlx::query("DELETE FROM rate_limits").execute(&db).await.expect("reset");
    spend_otp_window(&db, "otp_ip:127.0.0.1", api::ops_alerts::OTP_MAX_PER_IP_PER_HOUR).await;
    let (st, b) = call(&app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": "010-5555-0203" }))).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "{b}");
}
