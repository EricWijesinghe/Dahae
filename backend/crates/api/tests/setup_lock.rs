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

fn portone_card(all: &Value) -> Value {
    all["integrations"].as_array().unwrap().iter().find(|c| c["code"] == "portone").unwrap().clone()
}

fn keys(secret: &str) -> Value {
    json!({ "store_id": "store-00000000-0000-0000-0000-000000000000", "api_secret": secret })
}

/// Saves the card once (a first save of an empty section is never locked out).
async fn save(app: &Router, token: &str, secret: &str) {
    let (st, body) = common::call_owner(app, "PUT", PORTONE, token, None, keys(secret)).await;
    assert_eq!(st, StatusCode::OK, "first save: {body}");
}

async fn unlock(app: &Router, token: &str, code: &str) -> (StatusCode, Value) {
    common::call_owner(app, "POST", "/v1/admin/setup/portone/unlock", token, Some(code), Value::Null).await
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

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, None, keys("another-fake-secret-000000")).await;
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

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, None, keys("another-fake-secret-000000")).await;
    assert_eq!(st, StatusCode::OK, "unlocked save: {body}");
    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, None, keys("third-fake-secret-000000")).await;
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
    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, None, keys("another-fake-secret-000000")).await;
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

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &admin, None, keys("another-fake-secret-000000")).await;
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

    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &admin, None, keys("another-fake-secret-000000")).await;
    assert_eq!((st, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("card_locked")), "{body}");
    let (_, all) = common::call_owner(&app, "GET", "/v1/admin/setup", &admin, None, Value::Null).await;
    assert_eq!(portone_card(&all)["locked"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn short_secrets_are_not_previewed(db: PgPool) {
    let app = common::app(db.clone());
    let (token, _) = enrolled_owner(&db).await;
    let (st, body) = common::call_owner(&app, "PUT", PORTONE, &token, None, keys("abc1")).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let field = body["fields"].as_array().unwrap().iter().find(|f| f["name"] == "api_secret").unwrap();
    assert_eq!(field["preview"], "••••", "{body}");
    assert!(!body.to_string().contains("abc1"), "{body}");
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
    bodies.push(common::call_owner(&app, "PUT", PORTONE, &token, None, keys(SECRET)).await.1);
    bodies.push(common::call_owner(&app, "GET", "/v1/admin/setup", &token, None, Value::Null).await.1);
    bodies.push(unlock(&app, &token, &code_now()).await.1);
    bodies.push(
        common::call_owner(&app, "POST", "/v1/admin/setup/portone/lock", &token, None, Value::Null)
            .await
            .1,
    );
    for body in &bodies {
        assert!(!body.to_string().contains(SECRET), "raw secret in response: {body}");
    }
    let card = &bodies[0];
    let field = card["fields"].as_array().unwrap().iter().find(|f| f["name"] == "api_secret").unwrap();
    assert_eq!(field["preview"], "••••0000");
}

const CHANNEL: &str = "channel-key-00000000-0000-0000-0000-000000000000";
const BILLING_CHANNEL: &str = "channel-key-11111111-1111-1111-1111-111111111111";

/// Saves the PortOne card with these channel keys as the owner (the card must be unlocked first if already saved).
async fn save_channels(app: &Router, token: &str, channels: Value) {
    let body = json!({ "store_id": "store-00000000-0000-0000-0000-000000000000", "api_secret": SECRET, "channel_keys": channels });
    let (st, body) = common::call_owner(app, "PUT", PORTONE, token, None, body).await;
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
    let has_key = |code: &str| body.as_array().unwrap().iter().find(|m| m["code"] == code).unwrap()["has_key"].clone();
    assert_eq!((has_key("card"), has_key("kakaopay")), (json!(true), json!(false)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn billing_is_ready_only_with_its_own_channel_key(db: PgPool) {
    let app = common::prod_app(db.clone(), "http://127.0.0.1:9");
    let (token, _) = enrolled_owner(&db).await;
    save_channels(&app, &token, json!({ "card": CHANNEL })).await;
    let dev = common::app(db.clone());
    let c = common::customer(&dev, &db, "010-7100-0002").await;
    let (_, setup) = common::call(&app, "GET", "/v1/me/payment-methods/setup", Some(&c.token), None).await;
    assert_eq!(setup["enabled"], false, "one-off card key does not enable billing");

    assert_eq!(unlock(&app, &token, &code_now()).await.0, StatusCode::OK);
    save_channels(&app, &token, json!({ "card": CHANNEL, "card_billing": BILLING_CHANNEL })).await;
    let (_, setup) = common::call(&app, "GET", "/v1/me/payment-methods/setup", Some(&c.token), None).await;
    assert_eq!(setup["enabled"], true);
    assert_eq!(setup["channels"]["card"], BILLING_CHANNEL, "the app reads billing under card");
}
