// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Schedule guards: a quote deposit cannot double-book a pro, a pro-assigned reschedule waits for the pro's
//! confirmation, and a stale on-site stamp is refused once the job changed hands or was cancelled.
//! Same DB and fakes as the other flow tests.

mod common;

use api::routes::site::Stamp;
use axum::{Router, http::StatusCode};
use common::{book, call, customer, seed_catalog, verified_pro};
use sqlx::PgPool;
use uuid::Uuid;

async fn accept(app: &Router, id: Uuid, pro: &str) {
    let (st, b) = call(app, "POST", &format!("/v1/partner/jobs/{id}/accept"), Some(pro), None).await;
    assert_eq!(st, StatusCode::OK, "{b}");
}

async fn slot(pool: &PgPool, id: Uuid) -> (chrono::NaiveDate, i16, Option<serde_json::Value>) {
    sqlx::query_as("SELECT visit_date, start_hour, reschedule FROM bookings WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("slot")
}

/// A quoted booking (deposit due, 9:00 on day 10) with `pro`'s quote selected by staff.
async fn selected_quote(app: &Router, pool: &PgPool, c: &common::Customer, pro: Uuid) -> Uuid {
    let (quote, _) = book(app, c, 10, None).await;
    sqlx::query("UPDATE bookings SET status = 0, final_price = 100000, deposit = 10000 WHERE id = $1")
        .bind(quote)
        .execute(pool)
        .await
        .expect("quote");
    sqlx::query("INSERT INTO quotes (booking_id, pro_id, amount, state) VALUES ($1, $2, 100000, 'selected')")
        .bind(quote)
        .bind(pro)
        .execute(pool)
        .await
        .expect("selected quote");
    quote
}

/// A booking the pro holds for today (the visit day, for on-site stamps).
async fn assigned_today(app: &Router, pool: &PgPool, c: &common::Customer, pro: &str) -> Uuid {
    let (id, _) = book(app, c, 10, None).await;
    accept(app, id, pro).await;
    sqlx::query("UPDATE bookings SET visit_date = (now() AT TIME ZONE 'Asia/Seoul')::date WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("today");
    id
}

/// A booking that is already the pro's, on `days` from today at 14:00.
async fn assigned_at(app: &Router, pool: &PgPool, c: &common::Customer, pro: &str, days: i64, hour: i16) -> Uuid {
    let (id, _) = book(app, c, days, None).await;
    accept(app, id, pro).await;
    sqlx::query("UPDATE bookings SET start_hour = $2 WHERE id = $1")
        .bind(id)
        .bind(hour)
        .execute(pool)
        .await
        .expect("hour");
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn quote_deposit_refuses_a_pro_who_is_booked_at_that_time(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0301").await;
    let (pro, pro_id) = verified_pro(&app, &pool, "010-5400-0302").await;
    let _busy = assigned_at(&app, &pool, &c, pro.as_str(), 10, 9).await;

    // The quote booking at the same slot: staff selected this pro's quote, and the deposit is due.
    let quote = selected_quote(&app, &pool, &c, pro_id).await;

    let body = serde_json::json!({ "payment_method_id": c.card });
    let (st, b) = call(&app, "POST", &format!("/v1/bookings/{quote}/deposit"), Some(c.token.as_str()), Some(body)).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("schedule_overlap")), "{b}");
    let (status, attached): (i16, Option<Uuid>) = sqlx::query_as("SELECT status, pro_id FROM bookings WHERE id = $1")
        .bind(quote)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!((status, attached), (0, None), "a refused deposit leaves the quote unconfirmed and unassigned");
}

#[sqlx::test(migrations = "../../migrations")]
async fn quote_deposit_refuses_a_pro_whose_offers_are_paused(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0314").await;
    let (_, pro_id) = verified_pro(&app, &pool, "010-5400-0315").await;
    sqlx::query("INSERT INTO partner_quality (pro_id, score, status, lift_pause, inputs, computed_at) VALUES ($1, 20, 'paused', FALSE, '{}'::jsonb, now())")
        .bind(pro_id)
        .execute(&pool)
        .await
        .expect("paused");
    let quote = selected_quote(&app, &pool, &c, pro_id).await;

    let body = serde_json::json!({ "payment_method_id": c.card });
    let (st, b) = call(&app, "POST", &format!("/v1/bookings/{quote}/deposit"), Some(c.token.as_str()), Some(body)).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("quality_paused")), "{b}");
    let (status, attached): (i16, Option<Uuid>) = sqlx::query_as("SELECT status, pro_id FROM bookings WHERE id = $1")
        .bind(quote)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!((status, attached), (0, None), "a paused pro is never attached by a deposit");
}

#[sqlx::test(migrations = "../../migrations")]
async fn pro_assigned_reschedule_moves_the_job_only_when_the_pro_confirms(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0303").await;
    let (pro, _) = verified_pro(&app, &pool, "010-5400-0304").await;
    let (id, _) = book(&app, &c, 10, None).await;
    accept(&app, id, pro.as_str()).await;
    let (day, hour, _) = slot(&pool, id).await;

    let body = serde_json::json!({ "visit_date": day + chrono::Duration::days(1), "start_hour": 14 });
    let (st, b) = call(&app, "POST", &format!("/v1/bookings/{id}/reschedule"), Some(c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (now_day, now_hour, pending) = slot(&pool, id).await;
    assert_eq!((now_day, now_hour), (day, hour), "the pro's schedule keeps the job until the pro confirms");
    assert!(pending.is_some(), "the request is pending");

    let (st, b) = call(
        &app,
        "POST",
        &format!("/v1/partner/jobs/{id}/reschedule"),
        Some(pro.as_str()),
        Some(serde_json::json!({ "accept": true })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (confirmed_day, confirmed_hour, pending) = slot(&pool, id).await;
    assert_eq!(
        (confirmed_day, confirmed_hour),
        (day + chrono::Duration::days(1), 14),
        "confirmation moves the job"
    );
    assert!(pending.is_none(), "confirmation clears the request");
}

#[sqlx::test(migrations = "../../migrations")]
async fn declined_reschedule_keeps_the_original_slot(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0305").await;
    let (pro, _) = verified_pro(&app, &pool, "010-5400-0306").await;
    let (id, _) = book(&app, &c, 10, None).await;
    accept(&app, id, pro.as_str()).await;
    let before = slot(&pool, id).await;

    let body = serde_json::json!({ "visit_date": before.0 + chrono::Duration::days(1), "start_hour": 14 });
    let (st, b) = call(&app, "POST", &format!("/v1/bookings/{id}/reschedule"), Some(c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = call(
        &app,
        "POST",
        &format!("/v1/partner/jobs/{id}/reschedule"),
        Some(pro.as_str()),
        Some(serde_json::json!({ "accept": false })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");

    let after = slot(&pool, id).await;
    assert_eq!((after.0, after.1), (before.0, before.1), "a declined change never moves the job");
    assert!(after.2.is_none(), "a declined request is cleared");
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirming_a_reschedule_into_another_job_is_refused(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0307").await;
    let (pro, _) = verified_pro(&app, &pool, "010-5400-0308").await;
    let (id, _) = book(&app, &c, 10, None).await;
    accept(&app, id, pro.as_str()).await;
    let other = assigned_at(&app, &pool, &c, pro.as_str(), 11, 9).await;
    let before = slot(&pool, id).await;

    let body = serde_json::json!({ "visit_date": before.0 + chrono::Duration::days(1), "start_hour": 14 });
    let (st, b) = call(&app, "POST", &format!("/v1/bookings/{id}/reschedule"), Some(c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    // Another job lands in that window before the pro answers.
    sqlx::query("UPDATE bookings SET start_hour = 14 WHERE id = $1")
        .bind(other)
        .execute(&pool)
        .await
        .expect("other job");
    let (st, b) = call(
        &app,
        "POST",
        &format!("/v1/partner/jobs/{id}/reschedule"),
        Some(pro.as_str()),
        Some(serde_json::json!({ "accept": true })),
    )
    .await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("schedule_overlap")), "{b}");

    let after = slot(&pool, id).await;
    assert_eq!((after.0, after.1), (before.0, before.1), "a refused confirmation leaves the job where it was");
    assert!(after.2.is_some(), "the request stays open for the pro to decline");
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_visit_stamp_is_refused_after_a_reassignment(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0309").await;
    let (pro, pro_id) = verified_pro(&app, &pool, "010-5400-0310").await;
    let (_, new_pro) = verified_pro(&app, &pool, "010-5400-0311").await;
    let id = assigned_today(&app, &pool, &c, pro.as_str()).await;
    // Staff reassigns the job after the old pro's snapshot was read.
    sqlx::query("UPDATE bookings SET pro_id = $2 WHERE id = $1")
        .bind(id)
        .bind(new_pro)
        .execute(&pool)
        .await
        .expect("reassign");

    let res = api::routes::site::stamp_visit(&pool, id, pro_id, Stamp::Arrival).await;
    assert!(matches!(res, Err(api::error::ApiError::Conflict("booking_changed"))), "{res:?}");
    let stamped: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT checked_in_at FROM bookings WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert!(stamped.is_none(), "the new pro's job is not stamped for the old pro");
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_visit_stamp_is_refused_after_a_cancellation(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0312").await;
    let (pro, pro_id) = verified_pro(&app, &pool, "010-5400-0313").await;
    let id = assigned_today(&app, &pool, &c, pro.as_str()).await;
    sqlx::query("UPDATE bookings SET status = -1 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("cancel");

    let res = api::routes::site::stamp_visit(&pool, id, pro_id, Stamp::Departure).await;
    assert!(matches!(res, Err(api::error::ApiError::Conflict("booking_changed"))), "{res:?}");
    let stamped: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT checked_out_at FROM bookings WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert!(stamped.is_none(), "a cancelled job takes no departure stamp");
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirming_a_reschedule_is_refused_once_the_booking_is_closed(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0316").await;
    let (pro, _) = verified_pro(&app, &pool, "010-5400-0317").await;
    for (n, closed) in [(10, -1_i16), (12, 3)] {
        let (id, _) = book(&app, &c, n, None).await;
        accept(&app, id, pro.as_str()).await;
        let before = slot(&pool, id).await;
        let body = serde_json::json!({ "visit_date": before.0 + chrono::Duration::days(1), "start_hour": 14 });
        let (st, b) = call(&app, "POST", &format!("/v1/bookings/{id}/reschedule"), Some(c.token.as_str()), Some(body)).await;
        assert_eq!(st, StatusCode::OK, "{b}");
        // Cancelled or completed while the request was still open.
        sqlx::query("UPDATE bookings SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(closed)
            .execute(&pool)
            .await
            .expect("close");

        let (st, b) = call(
            &app,
            "POST",
            &format!("/v1/partner/jobs/{id}/reschedule"),
            Some(pro.as_str()),
            Some(serde_json::json!({ "accept": true })),
        )
        .await;
        assert_eq!(
            (st, b["error"]["code"].as_str()),
            (StatusCode::CONFLICT, Some("booking_closed")),
            "status {closed}: {b}"
        );
        let after = slot(&pool, id).await;
        assert_eq!((after.0, after.1), (before.0, before.1), "status {closed}: a closed job is not moved");
        let told: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM notifications WHERE owner_id = $1 AND kind = $2 AND data->>'booking_id' = $3)
                  + (SELECT count(*) FROM notification_outbox WHERE owner_id = $1 AND kind = $2 AND data->>'booking_id' = $3)",
        )
        .bind(c.user)
        .bind("booking.reschedule_confirmed")
        .bind(id.to_string())
        .fetch_one(&pool)
        .await
        .expect("notices");
        assert_eq!(told, 0, "status {closed}: the customer is not told a closed booking moved");
    }
}
