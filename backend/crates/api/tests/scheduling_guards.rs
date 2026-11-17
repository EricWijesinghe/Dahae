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

/// The request as the pro's job screen shows it (`booking.reschedule`).
async fn shown_request(app: &Router, id: Uuid, pro: &str) -> serde_json::Value {
    let (st, job) = call(app, "GET", &format!("/v1/partner/jobs/{id}"), Some(pro), None).await;
    assert_eq!(st, StatusCode::OK, "{job}");
    job["booking"]["reschedule"].clone()
}

/// The pro answers `seen`, the request its screen showed.
async fn answer(app: &Router, id: Uuid, pro: &str, accept: bool, seen: &serde_json::Value) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({ "accept": accept, "requested_at": seen["requested_at"] });
    call(app, "POST", &format!("/v1/partner/jobs/{id}/reschedule"), Some(pro), Some(body)).await
}

/// The pro answers the request currently on its screen.
async fn reply(app: &Router, id: Uuid, pro: &str, accept: bool) -> (StatusCode, serde_json::Value) {
    let seen = shown_request(app, id, pro).await;
    answer(app, id, pro, accept, &seen).await
}

async fn ask_reschedule(app: &Router, c: &common::Customer, id: Uuid, day: chrono::NaiveDate, hour: i16) -> serde_json::Value {
    let body = serde_json::json!({ "visit_date": day, "start_hour": hour });
    let (st, b) = call(app, "POST", &format!("/v1/bookings/{id}/reschedule"), Some(c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    b
}

/// System message codes in the booking's customer chat, oldest first.
async fn customer_notices(pool: &PgPool, thread: Uuid) -> Vec<(String, serde_json::Value)> {
    sqlx::query_as("SELECT body->>'code', coalesce(body->'params', 'null') FROM chat_messages WHERE thread_id = $1 AND kind = 'system' ORDER BY created_at, id")
        .bind(thread)
        .fetch_all(pool)
        .await
        .expect("messages")
}

async fn customer_thread(pool: &PgPool, c: &common::Customer, id: Uuid) -> Uuid {
    sqlx::query_scalar("INSERT INTO chat_threads (audience, user_id, booking_id, topic) VALUES ('customer', $1, $2, 'studio') RETURNING id")
        .bind(c.user)
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("thread")
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

    let (st, b) = reply(&app, id, pro.as_str(), true).await;
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
    let (st, b) = reply(&app, id, pro.as_str(), false).await;
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
    let (st, b) = reply(&app, id, pro.as_str(), true).await;
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

        let (st, b) = reply(&app, id, pro.as_str(), true).await;
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

#[sqlx::test(migrations = "../../migrations")]
async fn confirming_is_refused_when_the_customer_changed_the_request_since_the_pro_saw_it(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0318").await;
    let (pro, _) = verified_pro(&app, &pool, "010-5400-0319").await;
    let (id, _) = book(&app, &c, 10, None).await;
    accept(&app, id, pro.as_str()).await;
    let before = slot(&pool, id).await;
    ask_reschedule(&app, &c, id, before.0 + chrono::Duration::days(1), 14).await;
    let seen = shown_request(&app, id, pro.as_str()).await;
    // The customer asks again for another day while the pro's screen still shows the first request.
    let other_day = before.0 + chrono::Duration::days(2);
    ask_reschedule(&app, &c, id, other_day, 14).await;

    for accepting in [true, false] {
        let (st, b) = answer(&app, id, pro.as_str(), accepting, &seen).await;
        assert_eq!(
            (st, b["error"]["code"].as_str()),
            (StatusCode::CONFLICT, Some("reschedule_changed")),
            "accept={accepting}: {b}"
        );
        let after = slot(&pool, id).await;
        assert_eq!((after.0, after.1), (before.0, before.1), "accept={accepting}: the job stays where it was");
        assert_eq!(
            after.2.expect("pending")["to_date"],
            serde_json::json!(other_day),
            "accept={accepting}: the new request stays open"
        );
    }

    let (st, b) = reply(&app, id, pro.as_str(), true).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let moved = slot(&pool, id).await;
    assert_eq!((moved.0, moved.1), (other_day, 14), "the request the pro saw is the one applied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirming_a_reschedule_whose_day_is_no_longer_bookable_is_refused(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0320").await;
    let (pro, _) = verified_pro(&app, &pool, "010-5400-0321").await;
    let (id, _) = book(&app, &c, 10, None).await;
    accept(&app, id, pro.as_str()).await;
    let before = slot(&pool, id).await;
    ask_reschedule(&app, &c, id, before.0 + chrono::Duration::days(1), 14).await;
    // Days pass before the pro answers: the requested day is now today.
    sqlx::query("UPDATE bookings SET reschedule = jsonb_set(reschedule, '{to_date}', to_jsonb(((now() AT TIME ZONE 'Asia/Seoul')::date)::text)) WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("age request");

    let (st, b) = reply(&app, id, pro.as_str(), true).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("reschedule_expired")), "{b}");
    let after = slot(&pool, id).await;
    assert_eq!((after.0, after.1), (before.0, before.1), "a past or same-day slot is never applied");
    assert!(after.2.is_some(), "the request stays open for the pro to decline");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_customer_chat_says_the_visit_moved_only_once_the_pro_confirms(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0322").await;
    let (pro, _) = verified_pro(&app, &pool, "010-5400-0323").await;
    let (id, _) = book(&app, &c, 10, None).await;
    accept(&app, id, pro.as_str()).await;
    let thread = customer_thread(&pool, &c, id).await;
    let before = slot(&pool, id).await;
    let to = before.0 + chrono::Duration::days(1);

    let answered = ask_reschedule(&app, &c, id, to, 14).await;
    // The answer is the pending request, not a moved visit.
    assert_eq!(answered["visit_date"], serde_json::json!(before.0), "{answered}");
    assert_eq!(answered["reschedule"]["to_date"], serde_json::json!(to), "{answered}");
    assert!(
        customer_notices(&pool, thread).await.is_empty(),
        "nothing says the visit moved while the pro has not confirmed"
    );

    let (st, b) = reply(&app, id, pro.as_str(), true).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let said = customer_notices(&pool, thread).await;
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].0, "rescheduled");
    assert_eq!(said[0].1["visit_date"], serde_json::json!(to));

    // A declined request tells the customer too.
    ask_reschedule(&app, &c, id, to + chrono::Duration::days(1), 14).await;
    let (st, b) = reply(&app, id, pro.as_str(), false).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let said = customer_notices(&pool, thread).await;
    assert_eq!(said.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["rescheduled", "reschedule_declined"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_reschedule_without_a_pro_still_moves_and_says_so_at_once(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0324").await;
    let (id, _) = book(&app, &c, 10, None).await;
    let thread = customer_thread(&pool, &c, id).await;
    let to = slot(&pool, id).await.0 + chrono::Duration::days(1);

    let answered = ask_reschedule(&app, &c, id, to, 14).await;
    assert_eq!(answered["visit_date"], serde_json::json!(to), "{answered}");
    assert!(answered["reschedule"].is_null(), "{answered}");
    let said = customer_notices(&pool, thread).await;
    assert_eq!(said.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["rescheduled"]);
}

/// A PG deposit checkout for the quote booking (the window the customer pays in).
async fn deposit_checkout(app: &Router, c: &common::Customer, quote: Uuid) -> (StatusCode, serde_json::Value) {
    let ck = serde_json::json!({ "kind": "deposit", "method": "card", "booking_id": quote });
    call(app, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(ck)).await
}

async fn checkouts_for(pool: &PgPool, booking: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM checkouts WHERE booking_id = $1")
        .bind(booking)
        .fetch_one(pool)
        .await
        .expect("checkouts")
}

#[sqlx::test(migrations = "../../migrations")]
async fn deposit_checkout_refuses_a_pro_who_is_booked_at_that_time_before_any_payment(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app_with_channels(pool.clone(), &["card"]);
    let c = customer(&app, &pool, "010-5400-0325").await;
    let (pro, pro_id) = verified_pro(&app, &pool, "010-5400-0326").await;
    let _busy = assigned_at(&app, &pool, &c, pro.as_str(), 10, 9).await;
    let quote = selected_quote(&app, &pool, &c, pro_id).await;

    let (st, b) = deposit_checkout(&app, &c, quote).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("schedule_overlap")), "{b}");
    assert_eq!(checkouts_for(&pool, quote).await, 0, "no PG window opens for a pro who cannot take the visit");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deposit_checkout_refuses_a_paused_pro_before_any_payment(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app_with_channels(pool.clone(), &["card"]);
    let c = customer(&app, &pool, "010-5400-0327").await;
    let (_, pro_id) = verified_pro(&app, &pool, "010-5400-0328").await;
    sqlx::query("INSERT INTO partner_quality (pro_id, score, status, lift_pause, inputs, computed_at) VALUES ($1, 20, 'paused', FALSE, '{}'::jsonb, now())")
        .bind(pro_id)
        .execute(&pool)
        .await
        .expect("paused");
    let quote = selected_quote(&app, &pool, &c, pro_id).await;

    let (st, b) = deposit_checkout(&app, &c, quote).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("quality_paused")), "{b}");
    assert_eq!(checkouts_for(&pool, quote).await, 0, "no PG window opens for a paused pro");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deposit_checkout_opens_for_a_free_pro(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app_with_channels(pool.clone(), &["card"]);
    let c = customer(&app, &pool, "010-5400-0329").await;
    let (_, pro_id) = verified_pro(&app, &pool, "010-5400-0330").await;
    let quote = selected_quote(&app, &pool, &c, pro_id).await;

    let (st, b) = deposit_checkout(&app, &c, quote).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(checkouts_for(&pool, quote).await, 1);
}

/// True while some session of this test's database waits on an advisory lock.
async fn advisory_waiter(pool: &PgPool) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype = 'advisory' AND NOT granted
           AND database = (SELECT oid FROM pg_database WHERE datname = current_database()))",
    )
    .fetch_one(pool)
    .await
    .expect("pg_locks")
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirming_a_reschedule_takes_the_slot_lock_before_the_pro_schedule_lock(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0331").await;
    let (pro, pro_id) = verified_pro(&app, &pool, "010-5400-0332").await;
    let (id, _) = book(&app, &c, 10, None).await;
    accept(&app, id, pro.as_str()).await;
    let to = slot(&pool, id).await.0 + chrono::Duration::days(1);
    ask_reschedule(&app, &c, id, to, 14).await;
    let seen = shown_request(&app, id, pro.as_str()).await;

    // A subscription booking holds the requested slot (it takes the slot lock first, then the pro's schedule).
    let mut holder = pool.begin().await.expect("tx");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("slot:{to}:14"))
        .execute(&mut *holder)
        .await
        .expect("slot lock");
    let confirm = {
        let (app, pro, seen) = (app.clone(), pro.clone(), seen.clone());
        tokio::spawn(async move { answer(&app, id, pro.as_str(), true, &seen).await })
    };
    for _ in 0..200 {
        if advisory_waiter(&pool).await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(advisory_waiter(&pool).await, "the confirmation waits on a lock");
    // While it waits on the slot, the pro's schedule must still be free, or the two would deadlock.
    let schedule_free: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(pro_id.to_string())
        .fetch_one(&mut *holder)
        .await
        .expect("schedule lock");
    holder.rollback().await.expect("release");
    let (st, b) = confirm.await.expect("confirm task");
    assert!(
        schedule_free,
        "the confirmation must not hold the pro's schedule lock while it waits for the slot"
    );
    assert_eq!(st, StatusCode::OK, "{b}");
}
