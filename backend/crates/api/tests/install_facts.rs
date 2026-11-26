// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Install facts: an existing install becomes KR at first boot (locked if it took a payment), a fresh install
//! stays unset, first boot runs once, the facts cannot be written through the generic settings save, and a
//! newer applied pack only warns in readiness. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, pack, routes::setup::install::first_boot, settings_registry};
use axum::http::StatusCode;
use common::{call, staff_token, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn stored(db: &PgPool, key: &str) -> Option<Value> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(format!("reg.{key}"))
        .fetch_optional(db)
        .await
        .expect("settings read")
}

/// One payment in `status` for booking number `n` (the catalog must be seeded); `demo` lists it as a demo row.
async fn payment(db: &PgPool, n: u8, status: &str, demo: bool) {
    let user: uuid::Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ($1, 'kr') RETURNING id")
        .bind(format!("0107000000{n}"))
        .fetch_one(db)
        .await
        .expect("user");
    let booking: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, created_at)
         VALUES ($1, $2, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, 3, 110000, 110000, 0, now()) RETURNING id",
    )
    .bind(format!("BKI-{n}"))
    .bind(user)
    .fetch_one(db)
    .await
    .expect("booking");
    let payment: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO payments (booking_id, amount, kind, status, refunded_amount, provider, idempotency_key, created_at)
         VALUES ($1, 110000, 'full', $2, CASE WHEN $2 = 'refunded' THEN 110000 ELSE 0 END, 'portone', $3, '2026-03-01T09:00:00Z') RETURNING id",
    )
    .bind(booking)
    .bind(status)
    .bind(format!("idem-install-{n}"))
    .fetch_one(db)
    .await
    .expect("payment");
    if demo {
        sqlx::query("INSERT INTO demo_rows (tbl, row_key) VALUES ('payments', $1)")
            .bind(payment.to_string())
            .execute(db)
            .await
            .expect("demo row");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn fresh_install_stays_unset_and_first_boot_runs_once(pool: PgPool) {
    let kr = pack::load("KR").expect("pack");
    assert!(!first_boot(&pool, &kr).await.expect("first boot"));
    for key in ["install.country", "install.currency", "install.tz", "install.locales", "install.locked_at"] {
        assert_eq!(stored(&pool, key).await, None, "{key} must stay unset");
    }
    let cfg = test_config();
    assert_eq!(
        settings_registry::get_text(&pool, &cfg, "install.pack_version_applied").await.expect("text"),
        ""
    );

    // The owner arrives later: a restart must not switch the install to KR behind the wizard's back.
    staff_token(&pool, Role::StaffAdmin).await;
    assert!(!first_boot(&pool, &kr).await.expect("second boot"));
    assert_eq!(stored(&pool, "install.country").await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn existing_install_becomes_kr_without_a_lock(pool: PgPool) {
    staff_token(&pool, Role::StaffAdmin).await;
    common::seed_catalog(&pool).await;
    payment(&pool, 1, "paid", true).await;
    payment(&pool, 2, "failed", false).await;
    let kr = pack::load("KR").expect("pack");
    assert!(first_boot(&pool, &kr).await.expect("first boot"));
    assert_eq!(stored(&pool, "install.country").await, Some(json!("KR")));
    assert_eq!(stored(&pool, "install.currency").await, Some(json!("KRW")));
    assert_eq!(stored(&pool, "install.tz").await, Some(json!("Asia/Seoul")));
    assert_eq!(stored(&pool, "install.locales").await, Some(json!(["ko", "en"])));
    assert_eq!(stored(&pool, "install.pack_version_applied").await, Some(json!(kr.version)));
    assert_eq!(stored(&pool, "install.locked_at").await, None, "demo and failed payments do not lock");
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'install.auto_country' AND actor_role = 'system'")
        .fetch_one(&pool)
        .await
        .expect("audit");
    assert_eq!(audits, 1);
    assert!(!first_boot(&pool, &kr).await.expect("second boot"), "runs once");
}

#[sqlx::test(migrations = "../../migrations")]
async fn existing_install_with_a_taken_payment_is_locked(pool: PgPool) {
    staff_token(&pool, Role::StaffAdmin).await;
    common::seed_catalog(&pool).await;
    payment(&pool, 1, "refunded", false).await;
    assert!(first_boot(&pool, &pack::load("KR").expect("pack")).await.expect("first boot"));
    assert_eq!(stored(&pool, "install.locked_at").await, Some(json!("2026-03-01T09:00:00Z")));
}

#[sqlx::test(migrations = "../../migrations")]
async fn install_facts_are_read_only_through_settings(pool: PgPool) {
    let app = common::app(pool.clone());
    let (token, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (st, doc) = call(&app, "GET", "/v1/admin/settings/install", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "{doc}");
    let country = doc["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["key"] == "install.country")
        .expect("country");
    assert_eq!(country["value"], Value::Null);
    assert_eq!(country["type"], "choice");
    assert_eq!(country["options"], json!(["KR"]));
    let (st, doc) = call(
        &app,
        "PUT",
        "/v1/admin/settings/install",
        Some(&token),
        Some(json!({ "values": { "install.country": "KR" } })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{doc}");
    assert_eq!(stored(&pool, "install.country").await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_newer_applied_pack_warns_but_stays_ready(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, doc) = call(&app, "GET", "/ready", None, None).await;
    assert_eq!((doc["ready"].clone(), doc["pack_ok"].clone()), (json!(true), json!(true)), "{doc}");
    sqlx::query("INSERT INTO settings (key, value) VALUES ('reg.install.pack_version_applied', '\"2999.1.0\"')")
        .execute(&pool)
        .await
        .expect("newer pack");
    let (st, doc) = call(&app, "GET", "/ready", None, None).await;
    assert_eq!(st, StatusCode::OK, "{doc}");
    assert_eq!((doc["ready"].clone(), doc["pack_ok"].clone()), (json!(true), json!(false)), "{doc}");
}
