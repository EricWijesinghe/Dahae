// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! E.164 phone copies, step one (migration 0507): every writer stores both columns, the startup backfill fills
//! old rows (idempotent, resumable, safe beside live writers), and the previous release still signs in on the
//! new schema because the old columns keep their values and meaning.

mod common;

use api::{
    build_state, phone,
    sms_outbox::{self, Message},
};
use axum::http::StatusCode;
use common::{app, call, login, test_config};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const PHONE: &str = "01012345678";
const PHONE_E164: &str = "+821012345678";

async fn user_with_phone(db: &PgPool, phone: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ($1, 'u') RETURNING id")
        .bind(phone)
        .fetch_one(db)
        .await
        .expect("user")
}

async fn copy(db: &PgPool, sql: &str, id: Uuid) -> Option<String> {
    sqlx::query_scalar(sql).bind(id).fetch_one(db).await.expect("read copy")
}

async fn user_copy(db: &PgPool, id: Uuid) -> Option<String> {
    copy(db, "SELECT phone_e164 FROM users WHERE id = $1", id).await
}

/// Rows as the previous release writes them: phones only, every copy empty, plus the placeholders it stores.
async fn previous_release_rows(db: &PgPool) -> (Uuid, Uuid) {
    let user = user_with_phone(db, "01011110001").await;
    let friend = user_with_phone(db, "01011110002").await;
    let deleted = user_with_phone(db, "01011110003").await;
    sqlx::query("UPDATE users SET phone = 'deleted:' || id::text, deleted_at = now() WHERE id = $1")
        .bind(deleted)
        .execute(db)
        .await
        .expect("deleted user");
    sqlx::query("INSERT INTO users (name) VALUES ('social, no phone yet')")
        .execute(db)
        .await
        .expect("phoneless user");
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone) VALUES ('01022220001') RETURNING id")
        .fetch_one(db)
        .await
        .expect("pro");
    sqlx::query(
        "INSERT INTO team_members (pro_id, name, phone, role, invite_status)
         VALUES ($1, '', '01022220001', 'owner', 'active'), ($1, '', '', 'member', 'removed')",
    )
    .bind(pro)
    .execute(db)
    .await
    .expect("team");
    sqlx::query("INSERT INTO otp_codes (phone, code_hash, expires_at) VALUES ('01033330001', 'h', now() + interval '3 minutes')")
        .execute(db)
        .await
        .expect("otp");
    sqlx::query("INSERT INTO referrals (referred_user_id, referrer_id, referred_phone) VALUES ($1, $2, '01011110002')")
        .bind(friend)
        .bind(user)
        .execute(db)
        .await
        .expect("referral");
    sqlx::query("INSERT INTO referrals (referred_user_id, referrer_id, referred_phone) VALUES ($1, $2, 'hmac:00ff')")
        .bind(deleted)
        .bind(user)
        .execute(db)
        .await
        .expect("pseudonymised referral");
    sqlx::query(
        "INSERT INTO notification_outbox (kind, title, body, data, channel, phone, next_attempt_at)
         VALUES ('t', 't', 'b', '{}', 'sms', '01044440001', now())",
    )
    .execute(db)
    .await
    .expect("text");
    sqlx::query(
        "INSERT INTO staff (email, name, password_hash, role, phone, alert_phone) VALUES ('p@test.local', 'p', 'x', 'admin', '01055550001', '01055550002')",
    )
    .execute(db)
    .await
    .expect("staff");
    (user, friend)
}

/// Filled copies per column, as `table.column=count`.
async fn filled(db: &PgPool) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for (table, column) in [
        ("users", "phone_e164"),
        ("pros", "phone_e164"),
        ("team_members", "phone_e164"),
        ("otp_codes", "phone_e164"),
        ("referrals", "referred_phone_e164"),
        ("notification_outbox", "phone_e164"),
        ("staff", "phone_e164"),
        ("staff", "alert_phone_e164"),
    ] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count({column}) FROM {table}"))
            .fetch_one(db)
            .await
            .expect("count");
        out.push((format!("{table}.{column}"), n));
    }
    out
}

#[sqlx::test(migrations = "../../migrations")]
async fn backfill_fills_every_phone_column_once(db: PgPool) {
    let (user, friend) = previous_release_rows(&db).await;
    assert!(filled(&db).await.iter().all(|(_, n)| *n == 0), "the previous release writes no copies");

    // Batch of 1: every column needs several pages.
    let first = phone::backfill(&db, "KR", 1).await.expect("backfill");
    assert_eq!(first, 9, "2 users, 1 pro, 1 team member, 1 code, 1 referral, 1 text, 2 staff numbers");
    let want: Vec<(String, i64)> = [
        ("users.phone_e164", 2),
        ("pros.phone_e164", 1),
        ("team_members.phone_e164", 1),
        ("otp_codes.phone_e164", 1),
        ("referrals.referred_phone_e164", 1),
        ("notification_outbox.phone_e164", 1),
        ("staff.phone_e164", 1),
        ("staff.alert_phone_e164", 1),
    ]
    .iter()
    .map(|(k, n)| (k.to_string(), *n))
    .collect();
    assert_eq!(filled(&db).await, want, "placeholders (deleted:, hmac:, empty, NULL) get no copy");
    assert_eq!(user_copy(&db, user).await.as_deref(), Some("+821011110001"));
    assert_eq!(user_copy(&db, friend).await.as_deref(), Some("+821011110002"));
    let alert: Option<String> = sqlx::query_scalar("SELECT alert_phone_e164 FROM staff").fetch_one(&db).await.expect("alert");
    assert_eq!(alert.as_deref(), Some("+821055550002"));

    assert_eq!(phone::backfill(&db, "KR", 1).await.expect("rerun"), 0, "a rerun changes nothing");
    assert_eq!(filled(&db).await, want);
}

#[sqlx::test(migrations = "../../migrations")]
async fn backfill_resumes_a_run_that_was_cut_off(db: PgPool) {
    let mut users = Vec::new();
    for i in 0..5 {
        users.push(user_with_phone(&db, &format!("0106666000{i}")).await);
    }
    // A run stopped after its first batch: two rows hold copies, three do not.
    for id in &users[..2] {
        sqlx::query("UPDATE users SET phone_e164 = '+82' || substr(phone, 2) WHERE id = $1")
            .bind(id)
            .execute(&db)
            .await
            .expect("partial run");
    }
    assert_eq!(phone::backfill(&db, "KR", 2).await.expect("resume"), 3, "only the rest is written");
    for (i, id) in users.iter().enumerate() {
        assert_eq!(user_copy(&db, *id).await, Some(format!("+82106666000{i}")));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_backfills_agree(db: PgPool) {
    for i in 0..20 {
        user_with_phone(&db, &format!("010777700{i:02}")).await;
    }
    let (a, b) = tokio::join!(phone::backfill(&db, "KR", 3), phone::backfill(&db, "KR", 3));
    assert_eq!(
        a.expect("first replica") + b.expect("second replica"),
        20,
        "each row is written by exactly one run"
    );
    let wrong: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE phone_e164 IS DISTINCT FROM '+82' || substr(phone, 2)")
        .fetch_one(&db)
        .await
        .expect("check");
    assert_eq!(wrong, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_phone_changed_by_the_previous_release_loses_its_stale_copy(db: PgPool) {
    let user = user_with_phone(&db, PHONE).await;
    phone::backfill(&db, "KR", 10).await.expect("backfill");
    assert_eq!(user_copy(&db, user).await.as_deref(), Some(PHONE_E164));

    // The previous release's /v1/me/phone: phone only.
    sqlx::query("UPDATE users SET phone = $2 WHERE id = $1 AND deleted_at IS NULL")
        .bind(user)
        .bind("01098765432")
        .execute(&db)
        .await
        .expect("old bind");
    assert_eq!(user_copy(&db, user).await, None, "never left naming the old number");
    // An unrelated update keeps the copy.
    phone::backfill(&db, "KR", 10).await.expect("backfill");
    sqlx::query("UPDATE users SET name = 'renamed', phone = phone WHERE id = $1")
        .bind(user)
        .execute(&db)
        .await
        .expect("rename");
    assert_eq!(user_copy(&db, user).await.as_deref(), Some("+821098765432"));

    // Same for the staff alert number and a referral's phone.
    sqlx::query("INSERT INTO staff (email, name, password_hash, role, alert_phone, alert_phone_e164) VALUES ('o@test.local', 'o', 'x', 'admin', $1, $2)")
        .bind(PHONE)
        .bind(PHONE_E164)
        .execute(&db)
        .await
        .expect("staff");
    sqlx::query("UPDATE staff SET alert_phone = '01011112222' WHERE email = 'o@test.local'")
        .execute(&db)
        .await
        .expect("old alert");
    let alert: Option<String> = sqlx::query_scalar("SELECT alert_phone_e164 FROM staff WHERE email = 'o@test.local'")
        .fetch_one(&db)
        .await
        .expect("alert");
    assert_eq!(alert, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_colliding_copy_is_skipped_without_stopping_the_backfill(db: PgPool) {
    let holder = user_with_phone(&db, "01012340000").await;
    // A stale copy that names another row's number (only possible with hand-edited data).
    sqlx::query("UPDATE users SET phone_e164 = $2 WHERE id = $1")
        .bind(holder)
        .bind(PHONE_E164)
        .execute(&db)
        .await
        .expect("stale");
    let blocked = user_with_phone(&db, PHONE).await;
    let other = user_with_phone(&db, "01012349999").await;
    assert_eq!(phone::backfill(&db, "KR", 10).await.expect("backfill"), 1);
    assert_eq!(user_copy(&db, blocked).await, None, "left empty and logged");
    assert_eq!(
        user_copy(&db, other).await.as_deref(),
        Some("+821012349999"),
        "the rest of the batch is written"
    );
}

/// The previous release's sign-in statements, verbatim: code stored and burnt by phone, account upserted by phone.
const N1_OTP_INSERT: &str = "INSERT INTO otp_codes (phone, code_hash, expires_at) VALUES ($1, $2, now() + interval '3 minutes')";
const N1_OTP_BURN: &str = "UPDATE otp_codes SET attempts = attempts + 1
     WHERE id = (SELECT id FROM otp_codes WHERE phone = $1 AND expires_at > now() ORDER BY created_at DESC LIMIT 1)
       AND attempts < $2
     RETURNING code_hash";
const N1_OTP_CONSUME: &str = "DELETE FROM otp_codes WHERE phone = $1 AND code_hash = $2";
const N1_USER_UPSERT: &str =
    "INSERT INTO users (phone, acquisition) VALUES ($1, $2) ON CONFLICT (phone) DO UPDATE SET phone = EXCLUDED.phone RETURNING id, (xmax = 0)";
const N1_PRO_UPSERT: &str = "INSERT INTO pros (phone) VALUES ($1) ON CONFLICT (phone) DO UPDATE SET phone = EXCLUDED.phone RETURNING id, (xmax = 0)";

async fn previous_release_login(db: &PgPool, upsert: &str, phone: &str) -> (Uuid, bool) {
    sqlx::query(N1_OTP_INSERT).bind(phone).bind("hash").execute(db).await.expect("old code insert");
    let burnt: Option<String> = sqlx::query_scalar(N1_OTP_BURN).bind(phone).bind(5).fetch_optional(db).await.expect("old burn");
    assert_eq!(burnt.as_deref(), Some("hash"), "the old code lookup by phone still works");
    sqlx::query(N1_OTP_CONSUME).bind(phone).bind("hash").execute(db).await.expect("old consume");
    let mut q = sqlx::query_as(upsert).bind(phone);
    if upsert == N1_USER_UPSERT {
        q = q.bind(None::<sqlx::types::Json<serde_json::Value>>);
    }
    q.fetch_one(db).await.expect("old upsert")
}

#[sqlx::test(migrations = "../../migrations")]
async fn previous_release_still_signs_in_on_the_new_schema(db: PgPool) {
    let app = app(db.clone());
    // Accounts made by this release are found by the previous one: the old columns hold the same values.
    let (_, customer) = login(&app, PHONE, "customer").await;
    let (_, pro) = login(&app, "01087654321", "partner").await;
    let (found, created) = previous_release_login(&db, N1_USER_UPSERT, PHONE).await;
    assert_eq!((found.to_string(), created), (customer, false));
    let (found, created) = previous_release_login(&db, N1_PRO_UPSERT, "01087654321").await;
    assert_eq!((found.to_string(), created), (pro, false));
    let stored: (String, Option<String>) = sqlx::query_as("SELECT phone, phone_e164 FROM users WHERE id = $1")
        .bind(found_user(&db, PHONE).await)
        .fetch_one(&db)
        .await
        .expect("row");
    assert_eq!(
        stored,
        (PHONE.to_string(), Some(PHONE_E164.to_string())),
        "old column untouched; the copy survives an old login"
    );

    // An account the previous release makes after a rollback is found by this release, which fills the copy.
    let (old_made, created) = previous_release_login(&db, N1_USER_UPSERT, "01055556666").await;
    assert!(created);
    assert_eq!(user_copy(&db, old_made).await, None);
    let (_, subject) = login(&app, "01055556666", "customer").await;
    assert_eq!(subject, old_made.to_string());
    assert_eq!(user_copy(&db, old_made).await.as_deref(), Some("+821055556666"), "a login fills the copy");
}

async fn found_user(db: &PgPool, phone: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE phone = $1")
        .bind(phone)
        .fetch_one(db)
        .await
        .expect("user")
}

async fn dev_code(app: &axum::Router, phone: &str) -> String {
    let (st, sent) = call(app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": phone }))).await;
    assert_eq!(st, StatusCode::OK, "{sent}");
    sent["dev_code"].as_str().expect("dev code").to_string()
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_writers_store_both_columns(db: PgPool) {
    let app = app(db.clone());
    let (token, subject) = login(&app, PHONE, "customer").await;
    let id: Uuid = subject.parse().expect("id");
    assert_eq!(user_copy(&db, id).await.as_deref(), Some(PHONE_E164), "sign-up");

    let code = dev_code(&app, "01099998888").await;
    let code_copy: Option<String> = sqlx::query_scalar("SELECT phone_e164 FROM otp_codes WHERE phone = '01099998888'")
        .fetch_one(&db)
        .await
        .expect("code");
    assert_eq!(code_copy.as_deref(), Some("+821099998888"), "login code");
    let (st, body) = call(
        &app,
        "POST",
        "/v1/me/phone",
        Some(&token),
        Some(json!({ "phone": "010-9999-8888", "code": code })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(user_copy(&db, id).await.as_deref(), Some("+821099998888"), "phone change");

    let state = build_state(db.clone(), test_config()).expect("state");
    let text = sms_outbox::enqueue(&state, Some(id), "01099998888", "booking.status", Message::Sms("hi"))
        .await
        .expect("enqueue");
    let text_copy: Option<String> = sqlx::query_scalar("SELECT phone_e164 FROM notification_outbox WHERE id = $1")
        .bind(text)
        .fetch_one(&db)
        .await
        .expect("text");
    assert_eq!(text_copy.as_deref(), Some("+821099998888"), "queued text");

    let (st, body) = call(&app, "DELETE", "/v1/me", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(user_copy(&db, id).await, None, "account deletion clears the copy with the phone");
}

#[sqlx::test(migrations = "../../migrations")]
async fn partner_writers_store_both_columns(db: PgPool) {
    let app = app(db.clone());
    let (token, subject) = login(&app, PHONE, "partner").await;
    let pro: Uuid = subject.parse().expect("id");
    let pro_copy = || copy(&db, "SELECT phone_e164 FROM pros WHERE id = $1", pro);
    let owner_copy = || copy(&db, "SELECT phone_e164 FROM team_members WHERE pro_id = $1 AND role = 'owner'", pro);
    assert_eq!(pro_copy().await.as_deref(), Some(PHONE_E164), "sign-up");
    assert_eq!(owner_copy().await.as_deref(), Some(PHONE_E164), "owner team row");

    let code = dev_code(&app, "01077778888").await;
    let (st, body) = call(
        &app,
        "POST",
        "/v1/partner/me/phone",
        Some(&token),
        Some(json!({ "phone": "01077778888", "code": code })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(pro_copy().await.as_deref(), Some("+821077778888"), "phone change");
    assert_eq!(owner_copy().await.as_deref(), Some("+821077778888"), "owner row follows");

    let (st, body) = call(
        &app,
        "POST",
        "/v1/partner/team",
        Some(&token),
        Some(json!({ "phone": "010 5555 4444", "name": "Kim" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let member: Option<String> = sqlx::query_scalar("SELECT phone_e164 FROM team_members WHERE pro_id = $1 AND role = 'member'")
        .bind(pro)
        .fetch_one(&db)
        .await
        .expect("member");
    assert_eq!(member.as_deref(), Some("+821055554444"), "invite");

    // A second partner without members: deleting one with members trips UNIQUE (pro_id, phone) on the shared ''.
    let (token, subject) = login(&app, "01033334444", "partner").await;
    let gone: Uuid = subject.parse().expect("id");
    let (st, body) = call(&app, "DELETE", "/v1/partner/me", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(
        copy(&db, "SELECT phone_e164 FROM pros WHERE id = $1", gone).await,
        None,
        "deletion clears the copy"
    );
    let left: i64 = sqlx::query_scalar("SELECT count(phone_e164) FROM team_members WHERE pro_id = $1")
        .bind(gone)
        .fetch_one(&db)
        .await
        .expect("team");
    assert_eq!(left, 0, "removed team rows lose their copies");
}

/// Columns that hold a phone, each with its E.164 copy.
const PHONE_COLUMNS: &[(&str, &str)] = &[
    ("phone", "phone_e164"),
    ("alert_phone", "alert_phone_e164"),
    ("referred_phone", "referred_phone_e164"),
];
const PHONE_TABLES: &[&str] = &["users", "pros", "team_members", "otp_codes", "referrals", "notification_outbox", "staff"];

/// Every SQL statement in `src` that writes a phone column also writes its copy. A statement is a string literal
/// starting at `INSERT INTO <table>` or `UPDATE <table>`; columns after its WHERE or RETURNING are only read.
#[test]
fn every_phone_writer_in_the_source_writes_the_copy() {
    let mut files = Vec::new();
    let mut dirs = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).expect("src dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    let mut writers = 0;
    let mut missing = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("read source");
        for verb in ["INSERT INTO ", "UPDATE "] {
            for (at, _) in text.match_indices(verb) {
                let rest = &text[at + verb.len()..];
                let table: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                if !PHONE_TABLES.contains(&table.as_str()) {
                    continue;
                }
                let statement = &rest[..rest.find('"').unwrap_or(rest.len())];
                let words: Vec<&str> = statement
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .filter(|w| !w.is_empty())
                    .collect();
                let written = &words[..words.iter().position(|w| matches!(*w, "WHERE" | "RETURNING")).unwrap_or(words.len())];
                for (column, copy) in PHONE_COLUMNS {
                    if written.contains(column) {
                        writers += 1;
                        if !written.contains(copy) {
                            missing.push(format!("{}: {verb}{table} writes {column} without {copy}", file.display()));
                        }
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
    assert!(writers >= 20, "the scan found the known writers ({writers})");
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_backfill_fills_bigint_keys_whose_text_order_differs(db: PgPool) {
    // Ids 1..12 sort "1" < "10" < "11" < "12" < "2" as text: paging must follow the number order.
    for i in 1..=12 {
        sqlx::query(
            "INSERT INTO notification_outbox (kind, title, body, data, channel, phone, next_attempt_at)
             VALUES ('t', 't', 'b', '{}', 'sms', $1, now())",
        )
        .bind(format!("010444400{i:02}"))
        .execute(&db)
        .await
        .expect("text");
    }
    assert_eq!(phone::backfill(&db, "KR", 3).await.expect("backfill"), 12, "one pass fills every page");
    let empty: Vec<i64> = sqlx::query_scalar("SELECT id FROM notification_outbox WHERE phone_e164 IS NULL ORDER BY id")
        .fetch_all(&db)
        .await
        .expect("left");
    assert!(empty.is_empty(), "rows left without a copy: {empty:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn backfill_pages_use_the_primary_key_in_a_generic_plan(db: PgPool) {
    let mut tx = db.begin().await.expect("tx");
    // Generic plans are what a cached statement switches to; seq scans and sorts off show whether the index can
    // serve both the range and the order at all.
    for set in [
        "SET LOCAL plan_cache_mode = force_generic_plan",
        "SET LOCAL enable_seqscan = off",
        "SET LOCAL enable_sort = off",
    ] {
        sqlx::query(set).execute(&mut *tx).await.expect("set");
    }
    for (table, column, key, last) in [
        ("notification_outbox", "phone", "id", "5"),
        ("users", "phone", "id", "00000000-0000-0000-0000-000000000005"),
        ("referrals", "referred_phone", "referred_user_id", "00000000-0000-0000-0000-000000000005"),
    ] {
        let sql = phone::backfill_page_sql(table, column, true).expect("known column");
        sqlx::query(&format!("PREPARE page AS {sql}")).execute(&mut *tx).await.expect("prepare");
        let plan: Vec<String> = sqlx::query_scalar(&format!("EXPLAIN EXECUTE page(1000, '{last}')"))
            .fetch_all(&mut *tx)
            .await
            .expect("explain");
        let plan = plan.join("\n");
        assert!(plan.contains(&format!("Index Scan using {table}_pkey")), "{table}: {plan}");
        assert!(plan.contains(&format!("Index Cond: ({key} > ")), "{table}: {plan}");
        assert!(!plan.contains("Sort"), "{table}: {plan}");
        sqlx::query("DEALLOCATE page").execute(&mut *tx).await.expect("deallocate");
    }
}

/// The router over a state whose read switch was decided like a start does: backfill, then `reads_ready`.
async fn switched_app(db: &PgPool) -> (axum::Router, bool) {
    phone::backfill(db, "KR", phone::BACKFILL_BATCH).await.expect("backfill");
    let ready = phone::reads_ready(db, "KR").await.expect("switch check");
    let state = build_state(db.clone(), test_config()).expect("state");
    state.phone_e164_reads.store(ready, std::sync::atomic::Ordering::Relaxed);
    (api::routes::router(state), ready)
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_read_switch_waits_for_a_complete_backfill(db: PgPool) {
    previous_release_rows(&db).await;
    assert!(!phone::reads_ready(&db, "KR").await.expect("check"), "old rows without copies");
    phone::backfill(&db, "KR", 2).await.expect("backfill");
    assert!(
        phone::reads_ready(&db, "KR").await.expect("check"),
        "every parseable phone has its copy; placeholders need none"
    );

    // A phone the previous release writes after a rollback holds the switch until the next backfill.
    previous_release_login(&db, N1_USER_UPSERT, "01077778888").await;
    assert!(!phone::reads_ready(&db, "KR").await.expect("check"));
}

/// A phone whose copy the backfill cannot write: another row already holds that copy.
async fn blocked_copy(db: &PgPool) {
    sqlx::query("INSERT INTO users (phone, name, phone_e164) VALUES ('01099990002', 'holder', $1)")
        .bind(PHONE_E164)
        .execute(db)
        .await
        .expect("holder");
    user_with_phone(db, PHONE).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_read_switch_waits_while_a_copy_cannot_be_written(db: PgPool) {
    blocked_copy(&db).await;
    phone::backfill(&db, "KR", 10).await.expect("backfill");
    assert!(!phone::reads_ready(&db, "KR").await.expect("check"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_read_switch_waits_while_the_previous_release_could_not_sign_a_row_in(db: PgPool) {
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone, phone_e164) VALUES ('01022220009', '+821022220009') RETURNING id")
        .fetch_one(&db)
        .await
        .expect("pro");
    assert!(phone::reads_ready(&db, "KR").await.expect("check"));
    // A copy with no phone beside it: the previous release reads only the phone, so it could not find this member.
    sqlx::query("INSERT INTO team_members (pro_id, name, phone, phone_e164, role, invite_status) VALUES ($1, '', '', '+821022220010', 'member', 'active')")
        .bind(pro)
        .execute(&db)
        .await
        .expect("member");
    assert!(!phone::reads_ready(&db, "KR").await.expect("check"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn switched_logins_read_the_copies_and_the_previous_release_still_signs_in(db: PgPool) {
    let customer = user_with_phone(&db, "01011112222").await;
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone) VALUES ('01022223333') RETURNING id")
        .fetch_one(&db)
        .await
        .expect("pro");
    // A probe row whose copy names another number than its phone: only a read of the copy finds it by that number.
    let member: Uuid = sqlx::query_scalar(
        "INSERT INTO team_members (pro_id, name, phone, phone_e164, role, invite_status)
         VALUES ($1, 'm', '01099990000', '+821044445555', 'member', 'active') RETURNING id",
    )
    .bind(pro)
    .fetch_one(&db)
    .await
    .expect("member");
    let (app, ready) = switched_app(&db).await;
    assert!(ready);

    let (_, subject) = login(&app, "01011112222", "customer").await;
    assert_eq!(subject, customer.to_string(), "the existing customer, found by its copy");
    let code = dev_code(&app, "01044445555").await;
    let (st, tokens) = call(
        &app,
        "POST",
        "/v1/auth/verify",
        None,
        Some(json!({ "phone": "01044445555", "code": code, "audience": "partner" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{tokens}");
    assert_eq!(
        (tokens["role"].as_str(), tokens["subject"].as_str()),
        (Some("member"), Some(member.to_string().as_str()))
    );
    let (_, subject) = login(&app, "01022223333", "partner").await;
    assert_eq!(subject, pro.to_string());

    // Accounts this release makes with the switch on are found by the previous release's statements.
    let (_, made) = login(&app, "01066667777", "customer").await;
    let (found, created) = previous_release_login(&db, N1_USER_UPSERT, "01066667777").await;
    assert_eq!((found.to_string(), created), (made, false));
    let (_, made) = login(&app, "01088889999", "partner").await;
    let (found, created) = previous_release_login(&db, N1_PRO_UPSERT, "01088889999").await;
    assert_eq!((found.to_string(), created), (made, false));
}

#[sqlx::test(migrations = "../../migrations")]
async fn before_the_switch_logins_read_the_old_columns(db: PgPool) {
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone) VALUES ('01022223334') RETURNING id")
        .fetch_one(&db)
        .await
        .expect("pro");
    sqlx::query(
        "INSERT INTO team_members (pro_id, name, phone, phone_e164, role, invite_status)
         VALUES ($1, 'm', '01099990001', '+821044445556', 'member', 'active')",
    )
    .bind(pro)
    .execute(&db)
    .await
    .expect("member");
    // A user without a copy keeps the switch off.
    blocked_copy(&db).await;
    let (app, ready) = switched_app(&db).await;
    assert!(!ready);
    let code = dev_code(&app, "01044445556").await;
    let (st, tokens) = call(
        &app,
        "POST",
        "/v1/auth/verify",
        None,
        Some(json!({ "phone": "01044445556", "code": code, "audience": "partner" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{tokens}");
    assert_eq!(tokens["role"], "pro", "the old column holds another number: a new partner account");
}
