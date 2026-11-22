// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Saved console payment cards lock: a save or clear is refused until the owner unlocks the card with a TOTP,
//! the unlock lasts a few minutes, and every step is audited. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{crypto, totp};
use axum::{Router, http::StatusCode};
use chrono::Utc;
use common::{staff_token, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const TOTP_SEED: &[u8] = b"12345678901234567890";
const SECRET: &str = "fake-portone-api-secret-000000";
const PORTONE: &str = "/v1/admin/setup/portone";
/// Shaped like a Toss security key (64 hex characters) and obviously not a real one.
const SECURITY_KEY: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Owner with an enrolled, confirmed TOTP. Returns the access token and the staff id.
async fn enrolled_owner(db: &PgPool) -> (String, Uuid) {
    let (token, id) = staff_token(db, api::auth::Role::StaffAdmin).await;
    enrol(db, id, true).await;
    (token, id)
}

async fn enrol(db: &PgPool, id: Uuid, owner: bool) {
    let enc = crypto::encrypt(&test_config().data_key, crypto::aad::STAFF_TOTP, &totp::encode_secret(TOTP_SEED));
    sqlx::query("UPDATE staff SET totp_secret = $2, totp_enabled_at = now(), recovery_confirmed_at = now(), is_owner = $3 WHERE id = $1")
        .bind(id)
        .bind(enc)
        .bind(owner)
        .execute(db)
        .await
        .expect("enrol totp");
}

fn code_now() -> String {
    format!("{:06}", totp::code_at(TOTP_SEED, (Utc::now().timestamp() / totp::STEP_SECS) as u64))
}

/// The previous step's code: a first save spends its code, so the unlock that follows uses the current step.
fn prev_code() -> String {
    format!("{:06}", totp::code_at(TOTP_SEED, (Utc::now().timestamp() / totp::STEP_SECS - 1) as u64))
}

fn portone_card(all: &Value) -> Value {
    all["integrations"].as_array().unwrap().iter().find(|c| c["code"] == "portone").unwrap().clone()
}

fn keys(secret: &str) -> Value {
    json!({ "store_id": "store-00000000-0000-0000-0000-000000000000", "api_secret": secret })
}

/// Saves the card once (a first save of an empty section is never locked out).
async fn save(app: &Router, token: &str, secret: &str) {
    let (st, body) = common::call_owner(app, "PUT", PORTONE, token, Some(prev_code().as_str()), keys(secret)).await;
    assert_eq!(st, StatusCode::OK, "first save: {body}");
}

async fn unlock(app: &Router, token: &str, code: &str) -> (StatusCode, Value) {
    unlock_section(app, token, "portone", code).await
}

async fn unlock_section(app: &Router, token: &str, section: &str, code: &str) -> (StatusCode, Value) {
    common::call_owner(app, "POST", &format!("/v1/admin/setup/{section}/unlock"), token, Some(code), Value::Null).await
}

async fn actions(db: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT action FROM audit_log WHERE target = 'portone' ORDER BY created_at, id")
        .fetch_all(db)
        .await
        .expect("audit rows")
}

#[sqlx::test(migrations = "../../migrations")]
async fn save_is_refused_while_locked_and_the_stored_value_is_kept(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    save(&app, &token, SECRET).await;

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, Some(prev_code().as_str()), keys("another-fake-secret-000000")).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("card_locked")), "{body}");
    let (st, body) = common::call_owner(&app, "DELETE", PORTONE, &token, None, Value::Null).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("card_locked")), "{body}");

    let (_, all) = common::call_owner(&app, "GET", "/v1/admin/setup", &token, None, Value::Null).await;
    let card = all["integrations"].as_array().unwrap().iter().find(|c| c["code"] == "portone").unwrap().clone();
    assert_eq!(card["locked"], true);
    assert_eq!(card["unlock_expires_at"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unlock_needs_a_valid_owner_code(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    save(&app, &token, SECRET).await;

    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/portone/unlock", &token, None, Value::Null).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("totp_required")), "{body}");
    let (st, body) = unlock(&app, &token, "000000").await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("totp_invalid")), "{body}");
    assert_eq!(actions(&db).await.last().map(String::as_str), Some("setup.unlock_failed"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_unlock_lets_one_save_through_then_relocks(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    save(&app, &token, SECRET).await;

    let (st, body) = unlock(&app, &token, &code_now()).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert!(body["unlocked_until"].is_string(), "{body}");

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, Some(prev_code().as_str()), keys("another-fake-secret-000000")).await;
    assert_eq!(st, StatusCode::OK, "unlocked save: {body}");
    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, Some(prev_code().as_str()), keys("third-fake-secret-000000")).await;
    assert_eq!(
        (st, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("card_locked")),
        "save relocks: {body}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn consultant_cannot_unlock(db: PgPool) {
    let app = common::app(db.clone());
    let (owner, _) = enrolled_owner(&db).await;
    save(&app, &owner, SECRET).await;
    let (consultant, cid) = staff_token(&db, api::auth::Role::StaffConsultant).await;
    enrol(&db, cid, false).await;

    // The Admin extractor refuses consultants before the handler, so nothing is audited for them.
    let (st, body) = unlock(&app, &consultant, &code_now()).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{body}");
    let (_, all) = common::call_owner(&app, "GET", "/v1/admin/setup", &owner, None, Value::Null).await;
    assert_eq!(portone_card(&all)["locked"], true, "a refused consultant opens no window");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_who_is_not_owner_cannot_unlock(db: PgPool) {
    let app = common::app(db.clone());
    let (owner, _) = enrolled_owner(&db).await;
    save(&app, &owner, SECRET).await;
    let (admin, aid) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    enrol(&db, aid, false).await;

    let (st, body) = unlock(&app, &admin, &code_now()).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_used_code_cannot_unlock_twice(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    save(&app, &token, SECRET).await;
    let code = code_now();

    assert_eq!(unlock(&app, &token, &code).await.0, StatusCode::OK);
    let (st, _) = common::call_owner(&app, "POST", "/v1/admin/setup/portone/lock", &token, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK);
    let (st, body) = unlock(&app, &token, &code).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("totp_reused")), "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_card_relocks_once_the_window_has_passed(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    save(&app, &token, SECRET).await;
    assert_eq!(unlock(&app, &token, &code_now()).await.0, StatusCode::OK);

    sqlx::query("UPDATE integration_settings SET unlocked_until = now() - interval '1 second' WHERE section = 'portone'")
        .execute(&db)
        .await
        .expect("expire window");
    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, Some(prev_code().as_str()), keys("another-fake-secret-000000")).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("card_locked")), "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unlock_of_an_unsaved_card_is_refused(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    let (admin, aid) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    enrol(&db, aid, false).await;

    let (st, body) = unlock(&app, &admin, &code_now()).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "non-owner is refused before the card state: {body}");
    let (st, body) = unlock(&app, &token, &code_now()).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("nothing_saved")), "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_change_inside_the_window_needs_no_second_code(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    let toss = json!({ "mode": "toss", "secret_key": "test_sk_fakefakefakefake", "security_key": SECURITY_KEY });
    // A manual first save needs no code; spending one here would make the unlock below a reused step.
    let (st, body) = common::call_owner(&app, "PUT", "/v1/admin/setup/payout", &token, None, json!({ "mode": "manual" })).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/payout/unlock", &token, Some(&code_now()), Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{body}");

    // No code header inside the window: the request reaches the value checks. A key that was never
    // connection-tested is refused there, which proves the owner gate was passed without a second code.
    let (st, body) = common::call_owner(&app, "PUT", "/v1/admin/setup/payout", &token, None, toss.clone()).await;
    assert_eq!(
        (st, body["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("toss_unverified")),
        "{body}"
    );

    // Once the window has passed the same request is refused as a locked card.
    sqlx::query("UPDATE integration_settings SET unlocked_until = now() - interval '1 second' WHERE section = 'payout'")
        .execute(&db)
        .await
        .expect("expire window");
    let (st, body) = common::call_owner(&app, "PUT", "/v1/admin/setup/payout", &token, None, toss).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("card_locked")), "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn other_admins_cannot_change_or_clear_an_open_card(db: PgPool) {
    let app = common::app(db.clone());
    let (owner, _) = enrolled_owner(&db).await;
    save(&app, &owner, SECRET).await;
    assert_eq!(unlock(&app, &owner, &code_now()).await.0, StatusCode::OK);
    let (admin, aid) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    enrol(&db, aid, false).await;

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &admin, Some(prev_code().as_str()), keys("another-fake-secret-000000")).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{body}");
    let (st, body) = common::call_owner(&app, "DELETE", PORTONE, &admin, None, Value::Null).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{body}");

    let (_, all) = common::call_owner(&app, "GET", "/v1/admin/setup", &admin, None, Value::Null).await;
    assert_eq!(portone_card(&all)["unlock_expires_at"], Value::Null, "window hidden from non-owners");
    let (_, all) = common::call_owner(&app, "GET", "/v1/admin/setup", &owner, None, Value::Null).await;
    assert!(portone_card(&all)["unlock_expires_at"].is_string(), "owner sees the window");
}

#[sqlx::test(migrations = "../../migrations")]
async fn clearing_keeps_the_card_locked(db: PgPool) {
    let app = common::app(db.clone());
    let (owner, _) = enrolled_owner(&db).await;
    save(&app, &owner, SECRET).await;
    assert_eq!(unlock(&app, &owner, &code_now()).await.0, StatusCode::OK);
    let (st, body) = common::call_owner(&app, "DELETE", PORTONE, &owner, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let (admin, aid) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    enrol(&db, aid, false).await;

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &admin, Some(prev_code().as_str()), keys("another-fake-secret-000000")).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("card_locked")), "{body}");
    let (_, all) = common::call_owner(&app, "GET", "/v1/admin/setup", &admin, None, Value::Null).await;
    assert_eq!(portone_card(&all)["locked"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn too_short_api_secret_is_refused_before_anything_is_saved(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, Some(prev_code().as_str()), keys("abc1")).await;
    assert_eq!(
        (st, body["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_api_secret")),
        "{body}"
    );
    assert!(actions(&db).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_trail_records_save_unlock_failure_unlock_and_lock(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    save(&app, &token, SECRET).await;
    assert_eq!(unlock(&app, &token, "000000").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(unlock(&app, &token, &code_now()).await.0, StatusCode::OK);
    let (st, _) = common::call_owner(&app, "POST", "/v1/admin/setup/portone/lock", &token, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK);

    assert_eq!(actions(&db).await, ["setup.integration", "setup.unlock_failed", "setup.unlock", "setup.lock"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn secrets_never_come_back_in_any_response(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    let mut bodies = Vec::new();
    let all_keys = json!({ "store_id": "store-00000000-0000-0000-0000-000000000000", "api_secret": SECRET, "webhook_secret": WEBHOOK_SECRET, "channel_keys": { "card": CHANNEL } });
    bodies.push(common::call_owner(&app, "PUT", PORTONE, &token, Some(prev_code().as_str()), all_keys).await.1);
    bodies.push(common::call_owner(&app, "GET", "/v1/admin/setup", &token, None, Value::Null).await.1);
    bodies.push(unlock(&app, &token, &code_now()).await.1);
    bodies.push(
        common::call_owner(&app, "POST", "/v1/admin/setup/portone/lock", &token, None, Value::Null)
            .await
            .1,
    );
    for body in &bodies {
        for raw in [SECRET, WEBHOOK_SECRET, CHANNEL] {
            assert!(!body.to_string().contains(raw), "raw secret in response: {body}");
        }
    }
    let card = &bodies[0];
    let field = card["fields"].as_array().unwrap().iter().find(|f| f["name"] == "api_secret").unwrap();
    assert_eq!(field["preview"], "••••");
}

const CHANNEL: &str = "channel-key-00000000-0000-0000-0000-000000000000";
const WEBHOOK_SECRET: &str = "whsec_ZmFrZS13ZWJob29rLWtleS0wMDAwMDA=";
const BILLING_CHANNEL: &str = "channel-key-11111111-1111-1111-1111-111111111111";
const IDENTITY_CHANNEL: &str = "channel-key-22222222-2222-2222-2222-222222222222";

#[sqlx::test(migrations = "../../migrations")]
async fn a_payment_card_with_open_payments_cannot_be_cleared(db: PgPool) {
    let app = common::app(db.clone());
    let (owner, _) = enrolled_owner(&db).await;
    save(&app, &owner, SECRET).await;
    assert_eq!(unlock(&app, &owner, &code_now()).await.0, StatusCode::OK);
    let c = common::customer(&app, &db, "010-7100-0900").await; // with a saved PortOne card
    let open: Uuid = sqlx::query_scalar(
        "INSERT INTO checkouts (user_id, kind, method, amount, status, expires_at, gateway)
         VALUES ($1, 'booking', 'card', 9000, 'pending', now() + interval '1 hour', 'portone') RETURNING id",
    )
    .bind(c.user)
    .fetch_one(&db)
    .await
    .expect("checkout");
    let (st, body) = common::call_owner(&app, "DELETE", PORTONE, &owner, None, Value::Null).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("gateway_in_use")), "{body}");
    let (st, body) = common::call_owner(&app, "DELETE", "/v1/admin/setup/tosspayments", &owner, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK, "an unsaved Toss card has nothing to guard: {body}");

    sqlx::query("UPDATE checkouts SET gateway = 'tosspayments' WHERE id = $1")
        .bind(open)
        .execute(&db)
        .await
        .expect("move");
    let (st, body) = common::call_owner(&app, "DELETE", PORTONE, &owner, None, Value::Null).await;
    assert_eq!(
        (st, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("gateway_in_use")),
        "a saved card: {body}"
    );
    sqlx::query("UPDATE payment_methods SET deleted_at = now()")
        .execute(&db)
        .await
        .expect("card deleted");
    sqlx::query("INSERT INTO refund_queue (provider, tx_id, amount, cancellable) VALUES ('portone', 'tx-1', 100, 100)")
        .execute(&db)
        .await
        .expect("refund");
    let (st, body) = common::call_owner(&app, "DELETE", PORTONE, &owner, None, Value::Null).await;
    assert_eq!(
        (st, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("gateway_in_use")),
        "a queued refund: {body}"
    );
    sqlx::query("UPDATE refund_queue SET done_at = now()").execute(&db).await.expect("done");
    let (st, body) = common::call_owner(&app, "DELETE", PORTONE, &owner, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK, "nothing left on PortOne: {body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_payment_card_with_late_payable_checkouts_or_refundable_payments_cannot_be_cleared(db: PgPool) {
    common::seed_catalog(&db).await;
    let app = common::app(db.clone());
    let (owner, _) = enrolled_owner(&db).await;
    save(&app, &owner, SECRET).await;
    assert_eq!(unlock(&app, &owner, &code_now()).await.0, StatusCode::OK);
    let c = common::customer(&app, &db, "010-7100-0901").await;
    sqlx::query("UPDATE payment_methods SET deleted_at = now()")
        .execute(&db)
        .await
        .expect("no saved card");
    let clear = || common::call_owner(&app, "DELETE", PORTONE, &owner, None, Value::Null);
    let code = |b: &Value| b["error"]["code"].as_str().map(str::to_owned);

    // The reconciler still settles a late payment on a checkout that expired within the last day (can_settle).
    let expired: Uuid = sqlx::query_scalar(
        "INSERT INTO checkouts (user_id, kind, method, amount, status, expires_at, gateway, created_at)
         VALUES ($1, 'booking', 'card', 9000, 'expired', now() - interval '30 minutes', 'portone', now() - interval '1 hour') RETURNING id",
    )
    .bind(c.user)
    .fetch_one(&db)
    .await
    .expect("checkout");
    let (st, body) = clear().await;
    assert_eq!((st, code(&body).as_deref()), (StatusCode::CONFLICT, Some("gateway_in_use")), "{body}");
    sqlx::query("UPDATE checkouts SET created_at = now() - interval '25 hours' WHERE id = $1")
        .bind(expired)
        .execute(&db)
        .await
        .expect("older");

    // A finished payment can still be refunded through the gateway for a year (card and virtual account limits).
    let booking: Uuid = sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount)
         VALUES ('BKG-1', $1, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, 3, 10000, 10000, 0) RETURNING id",
    )
    .bind(c.user)
    .fetch_one(&db)
    .await
    .expect("booking");
    let pay = |status: &'static str, refunded: i64, age: &'static str, key: &'static str| {
        sqlx::query(
            "INSERT INTO payments (booking_id, amount, kind, status, refunded_amount, provider, provider_tx_id, idempotency_key, created_at)
             VALUES ($1, 10000, 'full', $2, $3, 'portone', $4, $4, now() - $5::interval)",
        )
        .bind(booking)
        .bind(status)
        .bind(refunded)
        .bind(key)
        .bind(age)
        .execute(&db)
    };
    pay("paid", 0, "10 days", "pay-recent").await.expect("payment");
    let (st, body) = clear().await;
    assert_eq!(
        (st, code(&body).as_deref()),
        (StatusCode::CONFLICT, Some("gateway_has_refundable_payments")),
        "{body}"
    );
    sqlx::query("UPDATE payments SET status = 'refunded', refunded_amount = amount")
        .execute(&db)
        .await
        .expect("refunded");
    pay("partially_refunded", 4000, "366 days", "pay-old").await.expect("old payment");
    let (st, body) = clear().await;
    assert_eq!(st, StatusCode::OK, "only refunded or past-the-window payments are left: {body}");
}

/// Saves the PortOne card with these channel keys as the owner (the card must be unlocked first if already saved).
async fn save_channels(app: &Router, token: &str, channels: Value) {
    let body = json!({ "store_id": "store-00000000-0000-0000-0000-000000000000", "api_secret": SECRET, "channel_keys": channels });
    let (st, body) = common::call_owner(app, "PUT", PORTONE, token, Some(prev_code().as_str()), body).await;
    assert_eq!(st, StatusCode::OK, "{body}");
}

fn method_codes(body: &Value) -> Vec<&str> {
    body.as_array().unwrap().iter().map(|m| m["code"].as_str().unwrap()).collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn customers_see_only_methods_that_have_a_channel_key(db: PgPool) {
    let app = common::prod_app(db.clone(), "http://127.0.0.1:9");
    let (token, _) = enrolled_owner(&db).await;
    save_channels(&app, &token, json!({ "card": CHANNEL, "kakaopay": CHANNEL })).await;
    let (st, body) = common::call(&app, "GET", "/v1/payments/methods", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(method_codes(&body), ["card", "kakaopay"]);

    let dev = common::app(db.clone());
    let (_, all) = common::call(&dev, "GET", "/v1/payments/methods", None, None).await;
    assert_eq!(all.as_array().unwrap().len(), 10, "dev mode keeps every method open");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_list_says_which_methods_have_a_key(db: PgPool) {
    let app = common::prod_app(db.clone(), "http://127.0.0.1:9");
    let (token, _) = enrolled_owner(&db).await;
    save_channels(&app, &token, json!({ "card": CHANNEL })).await;
    let (admin, _) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    let (st, body) = common::call(&app, "GET", "/v1/admin/payment-methods", Some(&admin), None).await;
    assert_eq!(st, StatusCode::OK);
    let configured = |code: &str| body.as_array().unwrap().iter().find(|m| m["code"] == code).unwrap()["configured"].clone();
    assert_eq!((configured("card"), configured("kakaopay")), (json!(true), json!(false)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn billing_uses_its_own_channel_or_the_legacy_card_key(db: PgPool) {
    let app = common::prod_app(db.clone(), "http://127.0.0.1:9");
    let (token, _) = enrolled_owner(&db).await;
    save_channels(&app, &token, json!({ "card": CHANNEL })).await;
    let dev = common::app(db.clone());
    let c = common::customer(&dev, &db, "010-7100-0002").await;
    let (_, setup) = common::call(&app, "GET", "/v1/me/payment-methods/setup", Some(&c.token), None).await;
    assert_eq!(setup["enabled"], true, "an install saved before the billing slot keeps billing on its card key");
    assert_eq!(setup["channels"]["card"], CHANNEL);

    assert_eq!(unlock(&app, &token, &code_now()).await.0, StatusCode::OK);
    save_channels(
        &app,
        &token,
        json!({ "card": CHANNEL, "card_billing": BILLING_CHANNEL, "identity": IDENTITY_CHANNEL, "cash_receipt": CHANNEL }),
    )
    .await;
    let (_, setup) = common::call(&app, "GET", "/v1/me/payment-methods/setup", Some(&c.token), None).await;
    assert_eq!(setup["enabled"], true);
    assert_eq!(setup["channels"]["card"], BILLING_CHANNEL, "the app reads billing under card");
    assert_eq!(setup["channels"]["identity"], IDENTITY_CHANNEL);
    assert!(setup["channels"].get("cash_receipt").is_none(), "server-only channels are not sent to the app");
}

#[sqlx::test(migrations = "../../migrations")]
async fn saving_one_channel_slot_keeps_the_others(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    save_channels(&app, &token, json!({ "card": CHANNEL, "identity": IDENTITY_CHANNEL })).await;
    assert_eq!(unlock(&app, &token, &code_now()).await.0, StatusCode::OK);
    save_channels(&app, &token, json!({ "payco": CHANNEL })).await;

    let (st, all) = common::call_owner(&app, "GET", "/v1/admin/setup", &token, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK);
    let slots = portone_card(&all)["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "channel_keys")
        .unwrap()["preview"]
        .clone();
    assert_eq!(
        slots,
        json!({ "card": "••••", "identity": "••••", "payco": "••••" }),
        "earlier slots survive a later save"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_portone_save_is_owner_only(db: PgPool) {
    let app = common::app(db.clone());
    let (admin, _) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &admin, Some(&code_now()), keys(SECRET)).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "a non-owner admin cannot point checkout at a merchant: {body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_connection_test_runs_while_the_card_is_unlocked(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    let backup = json!({
        "endpoint": "https://backup.invalid", "region": "ap-northeast-2", "bucket": "malkkeum-backup",
        "access_key": "AKIAEXAMPLE", "secret_key": "examplesecret"
    });
    let (st, body) = common::call_owner(&app, "PUT", "/v1/admin/setup/backup", &token, None, backup).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(unlock_section(&app, &token, "backup", &code_now()).await.0, StatusCode::OK);
    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/backup/test", &token, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["code"], "unreachable");
}

/// What a connection test must leave alone: the stored values, who saved them and the lock.
async fn backup_row(db: &PgPool) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    sqlx::query_as("SELECT secret::text, updated_at::text, updated_by::text, unlocked_until::text FROM integration_settings WHERE section = 'backup'")
        .fetch_one(db)
        .await
        .expect("backup row")
}

#[sqlx::test(migrations = "../../migrations")]
async fn connection_test_runs_on_a_locked_card_with_the_saved_values_only(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    let backup = |secret: &str| {
        json!({
            "endpoint": "https://backup.invalid", "region": "ap-northeast-2", "bucket": "malkkeum-backup",
            "access_key": "AKIAEXAMPLE", "secret_key": secret
        })
    };
    let (st, body) = common::call_owner(&app, "PUT", "/v1/admin/setup/backup", &token, None, backup("examplesecret")).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let before = backup_row(&db).await;

    // No body, no unlock, no owner code: the test probes with what is saved.
    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/backup/test", &token, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["code"], "unreachable");
    let (admin, _) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/backup/test", &admin, None, Value::Null).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "only the owner tests a locked card: {body}");
    assert_eq!(backup_row(&db).await, before, "no value, saver or lock changed");

    let (consultant, _) = staff_token(&db, api::auth::Role::StaffConsultant).await;
    let (st, _) = common::call_owner(&app, "POST", "/v1/admin/setup/backup/test", &consultant, None, Value::Null).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "still owner/admin only");
    // New values still need the owner's unlock.
    let (st, body) = common::call_owner(&app, "PUT", "/v1/admin/setup/backup", &token, None, backup("othersecret")).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("card_locked")), "{body}");
    assert_eq!(backup_row(&db).await, before);
}

/// A Toss stub answering `GET /v2/balances` with `status`. Returns its base URL.
async fn toss_stub(status: StatusCode) -> String {
    let router = Router::new().route("/v2/balances", axum::routing::get(move || async move { (status, "{}") }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind toss stub");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, router).await.expect("stub serves") });
    format!("http://{addr}")
}

/// The test router with its Toss calls sent to `toss_base`, so no check leaves the machine.
fn app_with_toss(db: &PgPool, toss_base: &str) -> Router {
    let cfg = test_config();
    let state = api::build_state(db.clone(), cfg.clone()).expect("state");
    let providers = api::providers::Providers::new(cfg, db.clone()).expect("providers").with_toss_api(toss_base);
    api::routes::router(api::state::AppState {
        providers: std::sync::Arc::new(providers),
        ..state
    })
}

const TOSS_TEST_SECRET: &str = "test_sk_fakefakefakefake";

/// Saves a locked Toss payout card whose stored check is `last_test`.
async fn saved_toss_card(db: &PgPool, owner: Uuid, last_test: Option<&Value>) {
    let values = json!({ "mode": "toss", "secret_key": TOSS_TEST_SECRET, "security_key": SECURITY_KEY });
    sqlx::query("INSERT INTO integration_settings (section, secret, updated_by, last_test) VALUES ('payout', $1, $2, $3)")
        .bind(crypto::encrypt(&test_config().data_key, crypto::aad::INTEGRATION, &values.to_string()))
        .bind(owner)
        .bind(last_test)
        .execute(db)
        .await
        .expect("saved toss card");
}

async fn payout_last_test(db: &PgPool) -> Value {
    sqlx::query_scalar::<_, Option<Value>>("SELECT last_test FROM integration_settings WHERE section = 'payout'")
        .fetch_one(db)
        .await
        .expect("last test")
        .unwrap_or(Value::Null)
}

async fn payout_card(app: &Router, token: &str) -> Value {
    let (st, all) = common::call_owner(app, "GET", "/v1/admin/setup", token, None, Value::Null).await;
    assert_eq!(st, StatusCode::OK, "{all}");
    all["integrations"].as_array().unwrap().iter().find(|c| c["code"] == "payout").unwrap().clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_payout_test_keeps_the_saved_pass_and_the_toss_settings(db: PgPool) {
    let app = app_with_toss(&db, &toss_stub(StatusCode::INTERNAL_SERVER_ERROR).await);
    let (token, owner) = enrolled_owner(&db).await;
    let key = test_config().data_key;
    let pass = json!({ "ok": true, "code": "sandbox_connected", "key_fp": api::integrations::key_fingerprint(&key, TOSS_TEST_SECRET), "at": Utc::now() });
    saved_toss_card(&db, owner, Some(&pass)).await;

    // The Toss stub answers 500: the check fails.
    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/payout/test", &token, None, Value::Null).await;
    assert_eq!((st, body["ok"].as_bool()), (StatusCode::OK, Some(false)), "{body}");
    assert_eq!(body["last_test"], pass, "the answer names the pass still in use: {body}");
    let last = payout_last_test(&db).await;
    assert_eq!(last, pass, "a failed check never replaces the recorded pass");
    assert!(
        api::integrations::toss_ready(true, Some(&last), &key, TOSS_TEST_SECRET),
        "the saved Toss settings still load"
    );

    // A reopened card shows the failed check as the latest one, the pass separately.
    let card = payout_card(&app, &token).await;
    assert_eq!(card["last_check"]["ok"], false, "{card}");
    assert_eq!(card["last_check"]["code"], body["code"], "{card}");
    assert_eq!(card["last_test"], pass, "{card}");
    assert_eq!(card["can_test"], true, "the owner may test the locked card: {card}");

    let (admin, _) = staff_token(&db, api::auth::Role::StaffAdmin).await;
    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/payout/test", &admin, None, Value::Null).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "only the owner tests the locked payout card: {body}");
    assert_eq!(payout_card(&app, &admin).await["can_test"], false, "a non-owner gets no test button");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_passing_payout_test_records_the_pass_and_opens_toss(db: PgPool) {
    let app = app_with_toss(&db, &toss_stub(StatusCode::OK).await);
    let (token, owner) = enrolled_owner(&db).await;
    saved_toss_card(&db, owner, None).await;

    let (st, body) = common::call_owner(&app, "POST", "/v1/admin/setup/payout/test", &token, None, Value::Null).await;
    assert_eq!(
        (st, body["ok"].as_bool(), body["code"].as_str()),
        (StatusCode::OK, Some(true), Some("sandbox_connected")),
        "{body}"
    );
    let last = payout_last_test(&db).await;
    assert_eq!(last["ok"], true, "{last}");
    assert_eq!(body["last_test"], last, "{body}");
    assert!(
        api::integrations::toss_ready(true, Some(&last), &test_config().data_key, TOSS_TEST_SECRET),
        "the pass opens Toss for this key"
    );
    let card = payout_card(&app, &token).await;
    assert_eq!((card["last_check"].clone(), card["last_test"].clone()), (last.clone(), last), "{card}");
}
