// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Readiness `adapters_ok`: a live row that names an adapter this binary does not know (a newer release's, seen
//! after a rollback) fails `/ready`, so the installer's automatic rollback fires. Needs DATABASE_URL (sqlx::test).

mod common;

use axum::http::StatusCode;
use common::call;
use serde_json::json;
use sqlx::PgPool;

/// A booking with one payment on `provider` in `status`.
async fn payment(db: &PgPool, provider: &str, status: &str) {
    common::seed_catalog(db).await;
    let user: uuid::Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000031', 'ready') RETURNING id")
        .fetch_one(db)
        .await
        .expect("user");
    let booking: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, created_at)
         VALUES ('BKR-1', $1, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, 1, 110000, 110000, 0, now()) RETURNING id",
    )
    .bind(user)
    .fetch_one(db)
    .await
    .expect("booking");
    sqlx::query(
        "INSERT INTO payments (booking_id, amount, kind, status, refunded_amount, provider, provider_tx_id, idempotency_key)
         VALUES ($1, 110000, 'full', $2, CASE WHEN $2 = 'refunded' THEN 110000 ELSE 0 END, $3, 'tx-ready', 'idem-ready')",
    )
    .bind(booking)
    .bind(status)
    .bind(provider)
    .execute(db)
    .await
    .expect("payment");
}

#[sqlx::test(migrations = "../../migrations")]
async fn todays_adapters_are_ready(pool: PgPool) {
    payment(&pool, "tosspayments_billing", "paid").await;
    let (st, doc) = call(&common::app(pool), "GET", "/ready", None, None).await;
    assert_eq!(st, StatusCode::OK, "{doc}");
    assert_eq!((doc["ready"].clone(), doc["adapters_ok"].clone()), (json!(true), json!(true)), "{doc}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_legacy_pg_payment_stays_ready(pool: PgPool) {
    payment(&pool, "pg", "paid").await;
    let (st, doc) = call(&common::app(pool), "GET", "/ready", None, None).await;
    assert_eq!((st, doc["adapters_ok"].clone()), (StatusCode::OK, json!(true)), "{doc}");
}

/// Every code any release wrote to payments.provider, payment_methods.gateway, refund_queue.provider or
/// payouts.provider (git history of their insert sites). Removing one would fail /ready on an install holding it.
#[test]
fn the_known_adapters_cover_every_code_ever_written() {
    let mut known = api::lifecycle::known_adapters();
    known.sort_unstable();
    assert_eq!(known, ["manual", "pg", "portone", "stripe", "toss", "tosspayments", "tosspayments_billing"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refundable_payment_on_an_unknown_adapter_fails_readiness(pool: PgPool) {
    payment(&pool, "mollie", "paid").await;
    let (st, doc) = call(&common::app(pool), "GET", "/ready", None, None).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{doc}");
    assert_eq!((doc["ready"].clone(), doc["adapters_ok"].clone()), (json!(false), json!(false)), "{doc}");
    assert_eq!(doc["db"], json!(true));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_adapter_with_nothing_left_to_refund_stays_ready(pool: PgPool) {
    payment(&pool, "mollie", "refunded").await;
    let (st, doc) = call(&common::app(pool), "GET", "/ready", None, None).await;
    assert_eq!((st, doc["adapters_ok"].clone()), (StatusCode::OK, json!(true)), "{doc}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_live_saved_card_on_an_unknown_adapter_fails_readiness(pool: PgPool) {
    let app = common::app(pool.clone());
    let user: uuid::Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000032', 'card') RETURNING id")
        .fetch_one(&pool)
        .await
        .expect("user");
    sqlx::query("INSERT INTO payment_methods (user_id, provider, billing_key, display_name, gateway) VALUES ($1, 'card', 'bk', 'card', 'mollie')")
        .bind(user)
        .execute(&pool)
        .await
        .expect("card");
    let (st, doc) = call(&app, "GET", "/ready", None, None).await;
    assert_eq!((st, doc["adapters_ok"].clone()), (StatusCode::SERVICE_UNAVAILABLE, json!(false)), "{doc}");
    sqlx::query("UPDATE payment_methods SET deleted_at = now()")
        .execute(&pool)
        .await
        .expect("deleted");
    let (st, doc) = call(&app, "GET", "/ready", None, None).await;
    assert_eq!(
        (st, doc["adapters_ok"].clone()),
        (StatusCode::OK, json!(true)),
        "a deleted card needs no adapter: {doc}"
    );
}
