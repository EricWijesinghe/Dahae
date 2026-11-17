//! PIPA customer rights over the real router and a migrated database: consent records and re-consent,
//! marketing withdrawal gating sends, the data export, the account deletion cascade, the retention
//! purge, and the staff access record for customer contact details.

mod common;

use api::{
    auth::{self, Role},
    build_state,
    privacy::{self, CONSENT_VERSION},
    realtime::Recipient,
    retention,
};
use axum::http::StatusCode;
use chrono::{Duration, TimeZone, Utc};
use common::{SECRET, app, book, call, customer, prod_app, seed_catalog, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn all_required_consents() -> serde_json::Value {
    json!([
        { "kind": "terms", "granted": true, "version": CONSENT_VERSION },
        { "kind": "privacy", "granted": true, "version": CONSENT_VERSION },
        { "kind": "overseas_transfer", "granted": true, "version": CONSENT_VERSION }
    ])
}

#[sqlx::test(migrations = "../../migrations")]
async fn location_terms_consent_is_recorded_on_its_own_and_withdrawable(pool: PgPool) {
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0009").await;
    let location = |granted: bool| json!({ "consents": [{ "kind": "location", "granted": granted, "version": CONSENT_VERSION }] });
    let granted = |s: &serde_json::Value| {
        s["items"]
            .as_array()
            .and_then(|i| i.iter().find(|i| i["kind"] == "location"))
            .map(|i| i["granted"].clone())
    };

    let (_, s) = call(&app, "GET", "/v1/me/consents", Some(c.token.as_str()), None).await;
    assert_eq!(granted(&s), Some(json!(false)), "never agreed: {s}");

    // The location terms record on their own, separately from the PIPA consents the signup already holds.
    let (st, s) = call(&app, "PUT", "/v1/me/consents", Some(c.token.as_str()), Some(location(true))).await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert_eq!(granted(&s), Some(json!(true)));
    assert_eq!(s["reconsent_required"], false, "location terms leave the required consents as signed up");

    let (st, s) = call(&app, "PUT", "/v1/me/consents", Some(c.token.as_str()), Some(location(false))).await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert_eq!(granted(&s), Some(json!(false)));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM user_consents WHERE user_id = $1 AND kind = 'location'")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(rows, 2, "grant and withdrawal are both kept as evidence");
}

#[sqlx::test(migrations = "../../migrations")]
async fn required_consents_gate_the_service_and_marketing_is_withdrawable(pool: PgPool) {
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0001").await;

    let (st, s) = call(&app, "GET", "/v1/me/consents", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["reconsent_required"], false, "signup recorded the required consents");
    assert_eq!(s["items"].as_array().map(Vec::len), Some(5));

    let (st, body) = call(
        &app,
        "PUT",
        "/v1/me/consents",
        Some(c.token.as_str()),
        Some(json!({ "consents": [{ "kind": "terms", "granted": false, "version": CONSENT_VERSION }] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "a required consent cannot be withdrawn: {body}");
    assert_eq!(body["error"]["code"], "required_consent");

    let (st, body) = call(
        &app,
        "PUT",
        "/v1/me/consents",
        Some(c.token.as_str()),
        Some(json!({ "consents": [{ "kind": "terms", "granted": true, "version": "2020-01-01" }] })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "stale policy version: {body}");
    assert_eq!(body["error"]["code"], "stale_consent_version");

    let (st, body) = call(
        &app,
        "PUT",
        "/v1/me/consents",
        Some(c.token.as_str()),
        Some(json!({ "consents": all_required_consents() })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["reconsent_required"], false);

    let (st, body) = call(
        &app,
        "PUT",
        "/v1/me/consents",
        Some(c.token.as_str()),
        Some(json!({ "consents": [{ "kind": "marketing", "granted": true, "version": CONSENT_VERSION }] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert!(privacy::marketing_allowed(&pool, c.user).await.expect("query"));

    let (st, _) = call(
        &app,
        "PUT",
        "/v1/me/consents",
        Some(c.token.as_str()),
        Some(json!({ "consents": [{ "kind": "marketing", "granted": false, "version": CONSENT_VERSION }] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(!privacy::marketing_allowed(&pool, c.user).await.expect("query"), "withdrawal stops marketing");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM user_consents WHERE user_id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(
        rows, 8,
        "every choice is kept as evidence (3 at signup, 3 re-recorded, marketing grant + withdrawal)"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn marketing_message_is_dropped_without_consent_and_transactional_still_sent(pool: PgPool) {
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0002").await;
    let state = build_state(pool.clone(), test_config()).expect("state");

    api::realtime::notify(&state, Recipient::User(c.user), "marketing.autumn", "t", "b", json!({}))
        .await
        .expect("notify");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE owner_id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 0, "no marketing without consent");

    privacy::update_consents(
        &pool,
        c.user,
        serde_json::from_value(json!({ "consents": all_required_consents() })).expect("body"),
    )
    .await
    .expect("required");
    privacy::update_consents(
        &pool,
        c.user,
        serde_json::from_value(json!({ "consents": [{ "kind": "marketing", "granted": true, "version": CONSENT_VERSION }] })).expect("body"),
    )
    .await
    .expect("grant");
    // 2026-11-11 12:00 KST: outside quiet hours, so the advertising notice is stored at a fixed instant.
    let noon = Utc.with_ymd_and_hms(2026, 11, 11, 3, 0, 0).single().expect("valid instant");
    api::realtime::notify_at(&state, Recipient::User(c.user), "marketing.autumn", "t", "b", json!({}), noon)
        .await
        .expect("notify");
    api::realtime::notify(&state, Recipient::User(c.user), "booking.status", "t", "b", json!({}))
        .await
        .expect("notify");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE owner_id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 2, "consented marketing and a transactional message are both stored");
}

#[sqlx::test(migrations = "../../migrations")]
async fn export_contains_the_customers_data_but_no_gateway_keys(pool: PgPool) {
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0003").await;
    let (st, doc) = call(&app, "GET", "/v1/me/export", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(doc["profile"]["phone"].is_string(), "profile phone exported");
    assert_eq!(doc["addresses"].as_array().map(Vec::len), Some(1));
    assert_eq!(doc["payment_methods"][0]["display_name"], "test card");
    assert!(!doc.to_string().contains("bk_test_"), "billing keys never leave the server");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deletion_anonymises_and_removes_devices_but_keeps_records(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0004").await;
    let (booking, _) = book(&app, &c, 10, None).await;
    privacy::update_consents(
        &pool,
        c.user,
        serde_json::from_value(json!({ "consents": all_required_consents() })).expect("body"),
    )
    .await
    .expect("consent");
    sqlx::query("INSERT INTO push_tokens (token, owner_role, owner_id, platform) VALUES ('tok-del-1', 'customer', $1, 'android')")
        .bind(c.user)
        .execute(&pool)
        .await
        .expect("push");

    let (st, body) = call(&app, "DELETE", "/v1/me", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");

    let (phone, name): (String, String) = sqlx::query_as("SELECT phone, name FROM users WHERE id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("user");
    assert!(phone.starts_with("deleted:") && name.is_empty());
    let addresses: i64 = sqlx::query_scalar("SELECT count(*) FROM addresses WHERE user_id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("addr");
    assert_eq!(addresses, 0, "addresses removed");
    let key: String = sqlx::query_scalar("SELECT billing_key FROM payment_methods WHERE id = $1")
        .bind(c.card)
        .fetch_one(&pool)
        .await
        .expect("card");
    assert!(key.is_empty(), "saved card key cleared");
    let tokens: i64 = sqlx::query_scalar("SELECT count(*) FROM push_tokens WHERE owner_id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("push");
    assert_eq!(tokens, 0, "push tokens removed");
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM bookings WHERE id = $1")
        .bind(booking)
        .fetch_one(&pool)
        .await
        .expect("booking");
    assert_eq!(kept, 1, "bookings kept for the 5-year record rule");
    let consents: i64 = sqlx::query_scalar("SELECT count(*) FROM user_consents WHERE user_id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("consent");
    // Three from the signup and three re-recorded below; none is removed by the deletion.
    assert_eq!(consents, 6, "consent evidence kept");
}

/// A deleted customer's finished booking loses its street address, answers, visit notes and household,
/// keeps the district and size, and keeps every amount.
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_pseudonymises_finished_bookings_and_keeps_amounts(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0011").await;
    let (done, _) = book(&app, &c, 10, None).await;
    sqlx::query(
        "UPDATE bookings SET status = 3, address = address || '{\"kind\": \"apartment\"}'::jsonb,
           answers = '{\"size_pyeong\": 24, \"note\": \"door code 1234\"}'::jsonb,
           visit_notes = '{\"parking\": \"B2 visitor spot\"}'::jsonb, household = '{\"pets\": [{\"kind\": \"cat\", \"count\": 2}]}'::jsonb
         WHERE id = $1",
    )
    .bind(done)
    .execute(&pool)
    .await
    .expect("finish booking");
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(b) - 'address' - 'answers' - 'visit_notes' - 'household' FROM bookings b WHERE id = $1")
            .bind(done)
            .fetch_one(&pool)
            .await
            .expect("before");

    let (st, body) = call(&app, "DELETE", "/v1/me", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");

    let (address, answers, notes, household): (serde_json::Value, Option<serde_json::Value>, serde_json::Value, serde_json::Value) =
        sqlx::query_as("SELECT address, answers, visit_notes, household FROM bookings WHERE id = $1")
            .bind(done)
            .fetch_one(&pool)
            .await
            .expect("after");
    assert!(
        address.get("road_address").is_none() && address.get("detail").is_none(),
        "street address removed: {address}"
    );
    assert!(address.get("district_code").is_some(), "district kept: {address}");
    assert_eq!(address["kind"], "apartment", "dwelling kind kept: {address}");
    assert_eq!(answers, Some(json!({})), "answers emptied, not NULL, so the booking still reads as a quote");
    assert_eq!(notes, json!({}), "visit notes cleared");
    assert_eq!(household, json!({}), "household cleared");
    let after: serde_json::Value = sqlx::query_scalar("SELECT to_jsonb(b) - 'address' - 'answers' - 'visit_notes' - 'household' FROM bookings b WHERE id = $1")
        .bind(done)
        .fetch_one(&pool)
        .await
        .expect("after columns");
    assert_eq!(after, before, "amounts, dates, service and status unchanged");
}

/// An upcoming booking still needs its address and answers for the visit, so deletion leaves it alone
/// and the hourly purge pseudonymises it once it is completed.
#[sqlx::test(migrations = "../../migrations")]
async fn open_booking_keeps_its_address_until_finished(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0012").await;
    let (open, _) = book(&app, &c, 10, None).await;
    let (st, body) = call(&app, "DELETE", "/v1/me", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let road: Option<String> = sqlx::query_scalar("SELECT address->>'road_address' FROM bookings WHERE id = $1")
        .bind(open)
        .fetch_one(&pool)
        .await
        .expect("open");
    assert!(road.is_some(), "open booking keeps its address for the visit");

    sqlx::query("UPDATE bookings SET status = 3 WHERE id = $1")
        .bind(open)
        .execute(&pool)
        .await
        .expect("finish");
    let report = retention::purge(&pool, &test_config()).await.expect("purge");
    assert_eq!(report.closed_bookings, 1, "the finished booking is pseudonymised by the purge");
    let road: Option<String> = sqlx::query_scalar("SELECT address->>'road_address' FROM bookings WHERE id = $1")
        .bind(open)
        .fetch_one(&pool)
        .await
        .expect("purged");
    assert!(road.is_none());
    let again = retention::purge(&pool, &test_config()).await.expect("second purge");
    assert_eq!(again.closed_bookings, 0, "a second purge changes nothing");
}

/// A completed quote stays a quote for its partner after the customer deletes the account: the partner's
/// job view still reports it as a quote and the quotes list still renders it.
#[sqlx::test(migrations = "../../migrations")]
async fn partner_still_sees_a_completed_quote_after_deletion(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0013").await;
    let (pro_token, pro_id) = common::verified_pro(&app, &pool, "010-8100-0014").await;
    let (done, _) = book(&app, &c, 10, None).await;
    sqlx::query(
        "UPDATE bookings SET status = 3, pro_id = $2, answers = '{\"building\": \"apartment\"}'::jsonb, final_price = 300000, deposit = 100000
         WHERE id = $1",
    )
    .bind(done)
    .bind(pro_id)
    .execute(&pool)
    .await
    .expect("complete quote");
    sqlx::query("INSERT INTO quotes (booking_id, pro_id, amount, state) VALUES ($1, $2, 300000, 'selected')")
        .bind(done)
        .bind(pro_id)
        .execute(&pool)
        .await
        .expect("quote");

    let (st, body) = call(&app, "DELETE", "/v1/me", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");

    let (st, job) = call(&app, "GET", &format!("/v1/partner/jobs/{done}"), Some(pro_token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{job}");
    assert_eq!(job["booking"]["is_quote"], true, "{job}");
    let (st, quotes) = call(&app, "GET", "/v1/partner/quotes", Some(pro_token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{quotes}");
    let row = quotes
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["booking_id"] == json!(done)))
        .expect("quote listed");
    assert!(row["answers"].is_object(), "{row}");
}

/// Deleting a customer sweeps only that customer's finished bookings; another deleted customer's rows
/// wait for the purge.
#[sqlx::test(migrations = "../../migrations")]
async fn account_deletion_pseudonymises_only_that_customers_bookings(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let earlier = customer(&app, &pool, "010-8100-0015").await;
    let (earlier_done, _) = book(&app, &earlier, 10, None).await;
    sqlx::query("UPDATE bookings SET status = 3 WHERE id = $1")
        .bind(earlier_done)
        .execute(&pool)
        .await
        .expect("finish earlier");
    let (st, body) = call(&app, "DELETE", "/v1/me", Some(earlier.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    sqlx::query("UPDATE bookings SET visit_notes = '{\"parking\": \"P1\"}'::jsonb WHERE id = $1")
        .bind(earlier_done)
        .execute(&pool)
        .await
        .expect("re-dirty earlier");

    let later = customer(&app, &pool, "010-8100-0016").await;
    let (later_done, _) = book(&app, &later, 10, None).await;
    sqlx::query("UPDATE bookings SET status = 3, visit_notes = '{\"parking\": \"P2\"}'::jsonb WHERE id = $1")
        .bind(later_done)
        .execute(&pool)
        .await
        .expect("finish later");
    let (st, body) = call(&app, "DELETE", "/v1/me", Some(later.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");

    let earlier_notes: serde_json::Value = sqlx::query_scalar("SELECT visit_notes FROM bookings WHERE id = $1")
        .bind(earlier_done)
        .fetch_one(&pool)
        .await
        .expect("earlier notes");
    assert_eq!(
        earlier_notes,
        json!({"parking": "P1"}),
        "another customer's rows are not swept by this deletion"
    );
    let later_notes: serde_json::Value = sqlx::query_scalar("SELECT visit_notes FROM bookings WHERE id = $1")
        .bind(later_done)
        .fetch_one(&pool)
        .await
        .expect("later notes");
    assert_eq!(later_notes, json!({}), "the deleted customer's own finished booking is cleared");
}

#[sqlx::test(migrations = "../../migrations")]
async fn account_is_not_erased_while_a_saved_card_cannot_be_revoked(pool: PgPool) {
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7100-0300").await;
    sqlx::query("UPDATE payment_methods SET billing_key = 'bk-fake-0000' WHERE id = $1")
        .bind(c.card)
        .execute(&pool)
        .await
        .expect("saved card key");

    // The gateway is unreachable, so the key cannot be revoked: deletion must stop before anything is erased.
    let prod = prod_app(pool.clone(), "http://127.0.0.1:9");
    let (st, body) = call(&prod, "DELETE", "/v1/me", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let phone: String = sqlx::query_scalar("SELECT phone FROM users WHERE id = $1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("user");
    assert!(!phone.starts_with("deleted:"), "nothing anonymised while the card is still billable");
}

#[sqlx::test(migrations = "../../migrations")]
async fn retention_purge_removes_expired_rows_and_unreferenced_uploads(pool: PgPool) {
    let cfg = test_config();
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone) VALUES ('01081000005') RETURNING id")
        .fetch_one(&pool)
        .await
        .expect("user");
    sqlx::query("INSERT INTO otp_codes (phone, code_hash, expires_at, created_at) VALUES ('01081000005', 'h', now(), now() - interval '2 days')")
        .execute(&pool)
        .await
        .expect("old otp");
    sqlx::query("INSERT INTO otp_codes (phone, code_hash, expires_at) VALUES ('01081000005', 'h', now())")
        .execute(&pool)
        .await
        .expect("new otp");
    sqlx::query("INSERT INTO refresh_tokens (token_hash, subject, role, family_id, expires_at) VALUES ('rt-expired', $1, 'customer', gen_random_uuid(), now() - interval '1 day')")
        .bind(user)
        .execute(&pool)
        .await
        .expect("rt");

    let old = Utc::now() - Duration::days(60);
    let orphan = "0123456789abcdef0123456789abcdef.jpg";
    let kept = "fedcba9876543210fedcba9876543210.jpg";
    for file in [orphan, kept] {
        sqlx::query("INSERT INTO uploads (owner_role, owner_id, file, bytes, created_at) VALUES ('customer', $1, $2, 10, $3)")
            .bind(user)
            .bind(file)
            .bind(old)
            .execute(&pool)
            .await
            .expect("upload");
    }
    sqlx::query("UPDATE users SET name = $2 WHERE id = $1")
        .bind(user)
        .bind(format!("/files/{kept}"))
        .execute(&pool)
        .await
        .expect("ref");

    let report = retention::purge(&pool, &cfg).await.expect("purge");
    assert_eq!(report.otp_codes, 1, "only the 2-day-old code");
    assert_eq!(report.refresh_tokens, 1);
    assert_eq!(report.uploads, 1, "only the unreferenced upload");
    let left: Vec<String> = sqlx::query_scalar("SELECT file FROM uploads").fetch_all(&pool).await.expect("left");
    assert_eq!(left, vec![kept.to_string()]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn staff_reads_of_customer_contacts_are_audit_logged(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0006").await;
    let (booking, _) = book(&app, &c, 10, None).await;
    let hash = auth::hash_password("staff-pass-123").expect("hash");
    let staff: Uuid = sqlx::query_scalar("INSERT INTO staff (email, name, password_hash, role) VALUES ('pii@test.local', 'Staff', $1, 'admin') RETURNING id")
        .bind(hash)
        .fetch_one(&pool)
        .await
        .expect("staff");
    let token = auth::issue_access(SECRET, staff, Role::StaffAdmin).expect("token");

    let (st, _) = call(&app, "GET", &format!("/v1/admin/bookings/{booking}"), Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = call(&app, "GET", "/v1/admin/customers", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    let logged: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE actor_id = $1 AND action LIKE 'customer.pii_%'")
        .bind(staff)
        .fetch_one(&pool)
        .await
        .expect("audit");
    assert_eq!(logged, 2, "both contact views recorded");
}

#[sqlx::test(migrations = "../../migrations")]
async fn partner_export_omits_account_number_and_delete_clears_notifications(pool: PgPool) {
    let app = app(pool.clone());
    let (token, pro) = common::verified_pro(&app, &pool, "010-8100-0007").await;
    sqlx::query("INSERT INTO payout_accounts (pro_id, bank, account_enc, account_last4, holder) VALUES ($1, '신한', 'SECRET-CIPHER-TEXT', '1234', '홍길동')")
        .bind(pro)
        .execute(&pool)
        .await
        .expect("payout account");
    sqlx::query("INSERT INTO notifications (owner_role, owner_id, kind, data) VALUES ('pro', $1, 'booking.status', '{}')")
        .bind(pro)
        .execute(&pool)
        .await
        .expect("notification");

    let (st, doc) = call(&app, "GET", "/v1/partner/export", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{doc}");
    assert_eq!(doc["payout_account"]["account_last4"], "1234");
    assert!(
        !doc.to_string().contains("SECRET-CIPHER-TEXT"),
        "encrypted account number never leaves the server"
    );

    let (st, body) = call(&app, "DELETE", "/v1/partner/me", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE owner_id = $1")
        .bind(pro)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(left, 0, "partner notifications removed");
}

#[sqlx::test(migrations = "../../migrations")]
async fn regranting_at_the_new_policy_version_clears_reconsent(pool: PgPool) {
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0006").await;
    // What a version bump leaves behind: the newest grant of each kind names the old text (the log is append-only).
    sqlx::query(
        "INSERT INTO user_consents (user_id, kind, version, granted)
         SELECT $1, kind, '2000-01-01', true FROM unnest(ARRAY['terms', 'privacy', 'overseas_transfer']) AS kind",
    )
    .bind(c.user)
    .execute(&pool)
    .await
    .expect("old-version grants");
    let (_, s) = call(&app, "GET", "/v1/me/consents", Some(c.token.as_str()), None).await;
    assert_eq!(s["reconsent_required"], true, "{s}");
    assert!(!privacy::overseas_allowed(&pool, c.user).await.expect("overseas"));

    // The app sends every item, as the consent screen does after the customer ticks the required ones.
    let mut all = all_required_consents();
    for kind in ["marketing", "location"] {
        all.as_array_mut()
            .expect("list")
            .push(json!({ "kind": kind, "granted": false, "version": CONSENT_VERSION }));
    }
    let (st, body) = call(&app, "PUT", "/v1/me/consents", Some(c.token.as_str()), Some(json!({ "consents": all }))).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["reconsent_required"], false, "{body}");
    let (_, s) = call(&app, "GET", "/v1/me/consents", Some(c.token.as_str()), None).await;
    assert_eq!(s["reconsent_required"], false, "{s}");
    assert!(privacy::overseas_allowed(&pool, c.user).await.expect("overseas"), "push and AI chat are back");
}

#[sqlx::test(migrations = "../../migrations")]
async fn overseas_transfer_consent_can_be_withdrawn_and_stays_off(pool: PgPool) {
    let app = app(pool.clone());
    let c = customer(&app, &pool, "010-8100-0005").await;
    let (st, body) = call(
        &app,
        "PUT",
        "/v1/me/consents",
        Some(c.token.as_str()),
        Some(json!({ "consents": [{ "kind": "overseas_transfer", "granted": false, "version": CONSENT_VERSION }] })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "the notice promises withdrawal in 동의 관리: {body}");
    let (st, s) = call(&app, "GET", "/v1/me/consents", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["reconsent_required"], false, "a withdrawn overseas consent is not asked again");
    let item = s["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["kind"] == "overseas_transfer")
        .expect("item");
    assert_eq!(item["granted"], false);
    assert_eq!(item["required"], false, "the switch is unlocked in the app");
}

#[sqlx::test(migrations = "../../migrations")]
async fn export_covers_every_section_that_holds_the_customers_data(pool: PgPool) {
    let app = app(pool.clone());
    seed_catalog(&pool).await;
    let c = customer(&app, &pool, "010-8100-0006").await;
    sqlx::query(
        "INSERT INTO waitlist_entries (user_id, service_code, visit_date, start_hour, end_hour, body)
         VALUES ($1, 'studio', '2026-12-01', 10, 12, '{\"notes\": \"현관 비밀번호 1234\"}')",
    )
    .bind(c.user)
    .execute(&pool)
    .await
    .expect("waitlist entry");
    let (st, doc) = call(&app, "GET", "/v1/me/export", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    for section in [
        "subscriptions",
        "waitlist",
        "reviews",
        "chat_messages",
        "uploads",
        "points",
        "tips",
        "cash_receipts",
        "tax_invoice_requests",
        "referrals",
    ] {
        assert!(doc.get(section).is_some_and(Value::is_array), "export section `{section}` missing");
    }
    assert_eq!(
        doc["waitlist"][0]["body"]["notes"], "현관 비밀번호 1234",
        "waitlist notes are the customer's own data"
    );
}

/// Payments, refunds, reports, marketing sends and chat replies of one customer, each carrying `tag`.
async fn seed_money_and_contact(app: &axum::Router, db: &PgPool, c: &common::Customer, tag: i64) {
    let (booking, _) = book(app, c, 10, None).await;
    sqlx::query("INSERT INTO payments (booking_id, amount, kind, status, provider, idempotency_key) VALUES ($1, $2, 'full', 'paid', 'test', $3)")
        .bind(booking)
        .bind(60_000 + tag)
        .bind(format!("export-{tag}"))
        .execute(db)
        .await
        .expect("payment");
    let checkout: Uuid = sqlx::query_scalar(
        "INSERT INTO checkouts (user_id, kind, booking_id, method, amount, status, expires_at, refund_reason)
         VALUES ($1, 'booking', $2, 'card', $3, 'refunded', now(), $4) RETURNING id",
    )
    .bind(c.user)
    .bind(booking)
    .bind(50_000 + tag)
    .bind(format!("refund-reason-{tag}"))
    .fetch_one(db)
    .await
    .expect("checkout");
    sqlx::query("INSERT INTO refund_queue (provider, tx_id, amount, checkout_id) VALUES ('test', $1, $2, $3)")
        .bind(format!("tx-{tag}"))
        .bind(40_000 + tag)
        .bind(checkout)
        .execute(db)
        .await
        .expect("refund");
    sqlx::query(
        "INSERT INTO cases (booking_id, kind, source, reporter_role, reporter_id, summary, sla_due_at)
         VALUES ($1, 'complaint', 'manual', 'customer', $2, $3, now())",
    )
    .bind(booking)
    .bind(c.user)
    .bind(format!("report-{tag}"))
    .execute(db)
    .await
    .expect("case");
    let campaign: Uuid = sqlx::query_scalar("INSERT INTO campaigns (name, channel, title, body) VALUES ($1, 'push', 't', 'b') RETURNING id")
        .bind(format!("campaign-{tag}"))
        .fetch_one(db)
        .await
        .expect("campaign");
    sqlx::query("INSERT INTO marketing_sends (dedupe_key, campaign_id, user_id, channel, status) VALUES ($1, $2, $3, 'push', 'sent')")
        .bind(format!("send-{tag}"))
        .bind(campaign)
        .bind(c.user)
        .execute(db)
        .await
        .expect("send");
    let thread: Uuid = sqlx::query_scalar("INSERT INTO chat_threads (audience, user_id, topic) VALUES ('customer', $1, 'general') RETURNING id")
        .bind(c.user)
        .fetch_one(db)
        .await
        .expect("thread");
    for (sender, text) in [("customer", format!("ask-{tag}")), ("staff", format!("reply-{tag}"))] {
        sqlx::query("INSERT INTO chat_messages (thread_id, sender, kind, body) VALUES ($1, $2, 'text', $3)")
            .bind(thread)
            .bind(sender)
            .bind(json!({ "text": text, "ai": sender == "staff" }))
            .execute(db)
            .await
            .expect("message");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn export_has_payments_refunds_reports_marketing_and_replies_of_this_customer_only(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = app(pool.clone());
    let me = customer(&app, &pool, "010-8100-0008").await;
    let other = customer(&app, &pool, "010-8100-0009").await;
    seed_money_and_contact(&app, &pool, &me, 1111).await;
    seed_money_and_contact(&app, &pool, &other, 2222).await;

    let (st, doc) = call(&app, "GET", "/v1/me/export", Some(me.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{doc}");
    let has = |section: &str, needle: &str| doc[section].as_array().is_some_and(|rows| rows.iter().any(|r| r.to_string().contains(needle)));
    let amounts = |section: &str| -> Vec<i64> { doc[section].as_array().into_iter().flatten().filter_map(|r| r["amount"].as_i64()).collect() };
    assert!(
        amounts("payments").contains(&61111) && !amounts("payments").contains(&62222),
        "payments: {}",
        doc["payments"]
    );
    assert!(
        amounts("checkouts").contains(&51111) && !amounts("checkouts").contains(&52222),
        "checkouts: {}",
        doc["checkouts"]
    );
    assert!(has("checkouts", "refund-reason-1111"), "checkouts: {}", doc["checkouts"]);
    assert_eq!(amounts("refunds"), vec![41111], "refunds: {}", doc["refunds"]);
    assert!(has("reports", "report-1111"), "reports: {}", doc["reports"]);
    assert!(has("marketing_sends", "campaign-1111"), "marketing: {}", doc["marketing_sends"]);
    assert!(has("chat_replies", "reply-1111"), "replies: {}", doc["chat_replies"]);
    assert!(!has("chat_replies", "ask-1111"), "the customer's own messages stay in chat_messages");
    let text = doc.to_string();
    for theirs in ["report-2222", "campaign-2222", "reply-2222", "ask-2222", "refund-reason-2222"] {
        assert!(!text.contains(theirs), "another customer's `{theirs}` leaked into the export");
    }
}
