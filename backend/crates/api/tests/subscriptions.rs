// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Subscriptions over the real router: planning, the day-before charge, declines and pause, partner continuity,
//! slot capacity, skip and move, cancel, owner edits, policy and metrics. PortOne billing runs in dev mode;
//! the `fake_decline` billing key is the mock decline.

mod common;

use api::{
    auth::{self, Role},
    build_state,
    providers::FAKE_DECLINE_KEY,
    routes::{self, subscription_engine},
    state::AppState,
};
use axum::{Router, http::StatusCode};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use common::{Customer, SECRET, call, customer, seed_catalog, test_config, verified_pro};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn kst_today() -> NaiveDate {
    (Utc::now() + Duration::hours(9)).date_naive()
}

/// Monday = 0, the convention the apps and partner prefs use.
fn weekday_index(d: NaiveDate) -> u8 {
    d.weekday().num_days_from_monday() as u8
}

fn app_and_state(db: PgPool) -> (Router, AppState) {
    let state = build_state(db, test_config()).expect("state");
    (routes::router(state.clone()), state)
}

fn admin() -> String {
    auth::issue_access(SECRET, Uuid::now_v7(), Role::StaffAdmin).expect("token")
}

async fn subscribe(app: &Router, c: &Customer, weekday: u8, cadence: i64, hour: u8, pro: Option<Uuid>) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/v1/subscriptions",
        Some(c.token.as_str()),
        Some(json!({
            "service_code": "studio",
            "address_id": c.address,
            "payment_method_id": c.card,
            "cadence_weeks": cadence,
            "weekday": weekday,
            "start_hour": hour,
            "preferred_pro_id": pro,
        })),
    )
    .await
}

fn sub_id(body: &Value) -> Uuid {
    body["subscription"]["id"].as_str().expect("sub id").parse().expect("uuid")
}

fn visits(body: &Value) -> Vec<Value> {
    body["visits"].as_array().cloned().unwrap_or_default()
}

async fn set_policy(db: &PgPool, discount: i64) {
    sqlx::query("INSERT INTO settings (key, value) VALUES ('subscription_policy', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(json!({ "discount_percent": discount }))
        .execute(db)
        .await
        .expect("policy");
}

async fn sub_status(db: &PgPool, id: Uuid) -> (String, Option<String>, i32) {
    sqlx::query_as("SELECT status, paused_reason, consecutive_failures FROM subscriptions WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
        .expect("subscription row")
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_plans_visits_on_the_requested_weekday(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, _) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0001").await;
    let (st, body) = subscribe(&app, &c, 2, 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let all = visits(&body);
    let planned: Vec<&Value> = all.iter().filter(|v| v["state"] == "planned").collect();
    // Weekly over the 28-day horizon: at least three Wednesdays.
    assert!(planned.len() >= 3, "{body}");
    for v in planned {
        let d: NaiveDate = v["visit_date"].as_str().unwrap().parse().unwrap();
        assert_eq!(weekday_index(d), 2, "{v}");
        assert!(d > kst_today() && d <= kst_today() + Duration::days(28), "{v}");
        assert_eq!(v["start_hour"], 9);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_cadence_and_weekday_are_refused(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, _) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0002").await;
    let (st, _) = subscribe(&app, &c, 2, 3, 9, None).await;
    assert!(st.is_client_error(), "cadence 3 accepted: {st}");
    let (st, _) = subscribe(&app, &c, 7, 1, 9, None).await;
    assert!(st.is_client_error(), "weekday 7 accepted: {st}");
    let (st, _) = subscribe(&app, &c, 2, 1, 10, None).await;
    assert!(st.is_client_error(), "slot 10 accepted: {st}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn due_visit_is_booked_and_charged_at_the_policy_discount(pool: PgPool) {
    seed_catalog(&pool).await;
    set_policy(&pool, 10).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0003").await;
    let (st, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(1)), 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["subscription"]["discount_percent"], 10);

    let report = subscription_engine::pass(&state).await.expect("pass");
    assert_eq!(report.booked, 1, "{report:?}");

    let (list, discount, paid, linked): (i64, i64, i64, Option<Uuid>) =
        sqlx::query_as("SELECT list_price, discount, paid_amount, subscription_id FROM bookings WHERE subscription_id = $1")
            .bind(sub_id(&body))
            .fetch_one(&pool)
            .await
            .expect("booking");
    assert_eq!(linked, Some(sub_id(&body)));
    assert_eq!(discount, list / 10, "10% of the list price");
    assert_eq!(paid, list - discount);
    let provider: String = sqlx::query_scalar("SELECT provider FROM payments WHERE booking_id = (SELECT id FROM bookings WHERE subscription_id = $1)")
        .bind(sub_id(&body))
        .fetch_one(&pool)
        .await
        .expect("payment");
    assert_eq!(provider, "portone");
    let (st, view) = call(&app, "GET", &format!("/v1/subscriptions/{}", sub_id(&body)), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(visits(&view).iter().any(|v| v["state"] == "booked"), "{view}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn declined_card_retries_then_pauses_until_a_new_card(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0004").await;
    let (sealed, digest) = api::crypto::seal_billing_key(&test_config().data_key, FAKE_DECLINE_KEY);
    sqlx::query("UPDATE payment_methods SET billing_key = $2, billing_key_digest = $3 WHERE id = $1")
        .bind(c.card)
        .bind(sealed)
        .bind(digest)
        .execute(&pool)
        .await
        .expect("decline card");
    let (_, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(1)), 1, 9, None).await;
    let id = sub_id(&body);

    let first = subscription_engine::pass(&state).await.expect("pass");
    assert_eq!((first.booked, first.failed), (0, 1), "{first:?}");
    let booked: i64 = sqlx::query_scalar("SELECT count(*) FROM bookings WHERE subscription_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(booked, 0, "a declined charge leaves no booking");
    assert_eq!(sub_status(&pool, id).await.2, 1);

    // Open the retry window twice more: the third consecutive decline pauses for payment.
    for _ in 0..2 {
        sqlx::query("UPDATE subscription_visits SET retry_at = NULL WHERE subscription_id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        subscription_engine::pass(&state).await.expect("pass");
    }
    let (status, reason, failures) = sub_status(&pool, id).await;
    assert_eq!((status.as_str(), reason.as_deref(), failures), ("paused", Some("payment"), 3));

    // Paused: no charge, even with the window open.
    sqlx::query("UPDATE subscription_visits SET retry_at = NULL WHERE subscription_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(subscription_engine::pass(&state).await.expect("pass").booked, 0);

    // A new card and a resume: the visit is charged on the next pass.
    let (sealed, digest) = api::crypto::seal_billing_key(&test_config().data_key, &format!("bk_test_second_{}", c.user));
    let second: Uuid = sqlx::query_scalar(
        "INSERT INTO payment_methods (user_id, provider, billing_key, billing_key_digest, display_name) VALUES ($1, 'pg', $2, $3, 'card 2') RETURNING id",
    )
    .bind(c.user)
    .bind(sealed)
    .bind(digest)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (st, resumed) = call(
        &app,
        "POST",
        &format!("/v1/subscriptions/{id}/resume"),
        Some(c.token.as_str()),
        Some(json!({ "payment_method_id": second })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{resumed}");
    assert_eq!(sub_status(&pool, id).await.0, "active");
    assert_eq!(subscription_engine::pass(&state).await.expect("pass").booked, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn preferred_partner_gets_the_visit_and_is_the_only_one_who_sees_it(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0005").await;
    let (ptoken, pro) = verified_pro(&app, &pool, "010-7000-0900").await;
    verified_pro(&app, &pool, "010-7000-0901").await;
    let (_, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(1)), 1, 9, Some(pro)).await;
    assert_eq!(subscription_engine::pass(&state).await.expect("pass").booked, 1);

    let (pro_of_booking, status): (Option<Uuid>, i16) = sqlx::query_as("SELECT pro_id, status FROM bookings WHERE subscription_id = $1")
        .bind(sub_id(&body))
        .fetch_one(&pool)
        .await
        .expect("booking");
    assert_eq!((pro_of_booking, status), (Some(pro), 2));

    let (st, mine) = call(&app, "GET", "/v1/partner/subscriptions", Some(ptoken.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{mine}");
    assert_eq!(mine.as_array().map(Vec::len), Some(1), "{mine}");
    assert_eq!(mine[0]["district_code"], common::DISTRICT);
    assert!(
        mine[0].get("user_id").is_none() && mine[0].get("address").is_none(),
        "partner sees contact-free rows only"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn planning_falls_back_when_the_customers_window_is_full(pool: PgPool) {
    seed_catalog(&pool).await;
    sqlx::query("INSERT INTO slot_capacity (start_hour, capacity) VALUES (9, 0) ON CONFLICT (start_hour) DO UPDATE SET capacity = 0")
        .execute(&pool)
        .await
        .unwrap();
    let (app, _) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0006").await;
    let (st, body) = subscribe(&app, &c, 2, 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let planned = visits(&body);
    assert!(!planned.is_empty());
    assert!(planned.iter().all(|v| v["start_hour"] == 8), "{body}");
}

async fn close_slot(db: &PgPool, hour: i16) {
    sqlx::query("INSERT INTO slot_capacity (start_hour, capacity) VALUES ($1, 0) ON CONFLICT (start_hour) DO UPDATE SET capacity = 0")
        .bind(hour)
        .execute(db)
        .await
        .expect("close slot");
}

/// Closes `hour` the moment the engine stores a charge id, i.e. after its seat check and before the charge.
async fn close_slot_once_the_charge_id_is_stored(db: &PgPool, hour: i16) {
    sqlx::raw_sql(&format!(
        "CREATE FUNCTION close_slot_on_charge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
           INSERT INTO slot_capacity (start_hour, capacity) VALUES ({hour}, 0) ON CONFLICT (start_hour) DO UPDATE SET capacity = 0;
           RETURN NEW; END $$;
         CREATE TRIGGER close_slot_on_charge AFTER UPDATE OF payment_ref ON subscription_visits
           FOR EACH ROW WHEN (NEW.payment_ref IS NOT NULL) EXECUTE FUNCTION close_slot_on_charge();"
    ))
    .execute(db)
    .await
    .expect("trigger");
}

async fn count_notices(db: &PgPool, kind: &str, owner: Option<Uuid>) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM notifications WHERE kind = $1 AND ($2::uuid IS NULL OR owner_id = $2)")
        .bind(kind)
        .bind(owner)
        .fetch_one(db)
        .await
        .expect("notice count")
}

#[sqlx::test(migrations = "../../migrations")]
async fn visit_moved_to_another_window_tells_the_customer(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, _) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0010").await;
    close_slot(&pool, 9).await;
    let (st, body) = subscribe(&app, &c, 2, 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let planned = visits(&body).iter().filter(|v| v["state"] == "planned").count() as i64;
    assert!(planned >= 3, "{body}");
    assert_eq!(
        count_notices(&pool, "subscription.visit_moved", Some(c.user)).await,
        planned,
        "one notice per moved visit"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn visit_with_no_free_window_tells_the_customer_and_staff(pool: PgPool) {
    seed_catalog(&pool).await;
    for hour in [8, 9, 13, 14] {
        close_slot(&pool, hour).await;
    }
    let (app, _) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0011").await;
    let (st, body) = subscribe(&app, &c, 2, 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let planned = visits(&body).iter().filter(|v| v["state"] == "planned").count() as i64;
    assert!(planned >= 3, "{body}");
    assert_eq!(count_notices(&pool, "subscription.visit_unplaced", Some(c.user)).await, planned);
    // Staff alerts are live events only (no inbox row), so nothing is stored for them.
    assert_eq!(count_notices(&pool, "subscription.capacity_full", None).await, 0, "staff alerts have no inbox");
}

#[sqlx::test(migrations = "../../migrations")]
async fn booking_that_fails_after_the_charge_is_refunded(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0012").await;
    let (_, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(1)), 1, 9, None).await;
    assert!(!visits(&body).is_empty(), "{body}");
    // The window fills between the seat check and the booking: the charge goes through, then the booking is refused.
    close_slot_once_the_charge_id_is_stored(&pool, 9).await;

    let report = subscription_engine::pass(&state).await.expect("pass");
    assert_eq!((report.booked, report.failed), (0, 1), "{report:?}");
    let (queued, done): (i64, bool) = sqlx::query_as("SELECT count(*), coalesce(bool_and(done_at IS NOT NULL), false) FROM refund_queue")
        .fetch_one(&pool)
        .await
        .expect("refund queue");
    assert_eq!((queued, done), (1, true), "the charge is refunded and the refund is recorded");
    // The card was charged, so the customer is told the money comes back, not only that a retry will follow.
    assert_eq!(count_notices(&pool, "subscription.slot_full_refunded", Some(c.user)).await, 1);
    assert_eq!(count_notices(&pool, "subscription.slot_full", Some(c.user)).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_skips_moves_pauses_and_cancels(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, _) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0007").await;
    let (_, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(2)), 1, 9, None).await;
    let id = sub_id(&body);
    let planned: Vec<Value> = visits(&body).into_iter().filter(|v| v["state"] == "planned").collect();
    assert!(planned.len() >= 2, "{body}");
    let (first, second) = (planned[0]["id"].as_str().unwrap().to_string(), planned[1]["id"].as_str().unwrap().to_string());

    let (st, v) = call(
        &app,
        "POST",
        &format!("/v1/subscriptions/{id}/visits/{first}/skip"),
        Some(c.token.as_str()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert!(visits(&v).iter().any(|x| x["id"] == first.as_str() && x["state"] == "skipped"));

    let moved_to = (kst_today() + Duration::days(20)).to_string();
    let (st, v) = call(
        &app,
        "POST",
        &format!("/v1/subscriptions/{id}/visits/{second}/reschedule"),
        Some(c.token.as_str()),
        Some(json!({ "visit_date": moved_to, "start_hour": 13 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert!(visits(&v).iter().any(|x| x["visit_date"] == moved_to.as_str() && x["start_hour"] == 13), "{v}");

    let (st, _) = call(&app, "POST", &format!("/v1/subscriptions/{id}/pause"), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(sub_status(&pool, id).await, ("paused".into(), Some("customer".into()), 0));
    let (st, _) = call(&app, "POST", &format!("/v1/subscriptions/{id}/pause"), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::CONFLICT, "pausing twice");

    let (st, _) = call(&app, "POST", &format!("/v1/subscriptions/{id}/resume"), Some(c.token.as_str()), Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK);

    let (st, v) = call(&app, "POST", &format!("/v1/subscriptions/{id}/cancel"), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["subscription"]["status"], "cancelled");
    assert_eq!(v["booked_visits"], json!([]));
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM subscription_visits WHERE subscription_id = $1 AND state = 'planned'")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "cancel clears planned visits");
}

#[sqlx::test(migrations = "../../migrations")]
async fn another_customers_subscription_is_not_found(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, _) = app_and_state(pool.clone());
    let owner = customer(&app, &pool, "010-7000-0008").await;
    let other = customer(&app, &pool, "010-7000-0009").await;
    let (_, body) = subscribe(&app, &owner, 2, 1, 9, None).await;
    let id = sub_id(&body);
    let (st, _) = call(&app, "GET", &format!("/v1/subscriptions/{id}"), Some(other.token.as_str()), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = call(&app, "POST", &format!("/v1/subscriptions/{id}/cancel"), Some(other.token.as_str()), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn visit_that_passed_unpaid_becomes_missed(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0010").await;
    let (_, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(1)), 1, 9, None).await;
    let id = sub_id(&body);
    sqlx::query("UPDATE subscription_visits SET visit_date = $2 WHERE subscription_id = $1 AND visit_date = $3")
        .bind(id)
        .bind(kst_today() - Duration::days(1))
        .bind(kst_today() + Duration::days(1))
        .execute(&pool)
        .await
        .unwrap();
    let report = subscription_engine::pass(&state).await.expect("pass");
    assert!(report.missed >= 1, "{report:?}");
    let missed: i64 = sqlx::query_scalar("SELECT count(*) FROM subscription_visits WHERE subscription_id = $1 AND state = 'missed'")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(missed, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_edits_pauses_and_reads_metrics(pool: PgPool) {
    seed_catalog(&pool).await;
    set_policy(&pool, 0).await;
    let (app, _) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0011").await;
    let (_, body) = subscribe(&app, &c, 2, 1, 9, None).await;
    let id = sub_id(&body);
    let token = admin();

    let (st, edited) = call(
        &app,
        "PUT",
        &format!("/v1/admin/subscriptions/{id}"),
        Some(token.as_str()),
        Some(json!({ "weekday": 4, "cadence_weeks": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{edited}");
    assert_eq!(edited["subscription"]["cadence_weeks"], 2);
    assert!(
        // Tomorrow's visit keeps its charge and its old date; everything after it moves to the new weekday.
        visits(&edited).iter().filter(|v| v["state"] == "planned").all(|v| {
            let d: NaiveDate = v["visit_date"].as_str().unwrap().parse().unwrap();
            d <= kst_today() + Duration::days(1) || weekday_index(d) == 4
        }),
        "{edited}"
    );
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'subscription.edit' AND target = $1")
        .bind(id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audited, 1);

    let (st, metrics) = call(&app, "GET", "/v1/admin/subscriptions/metrics", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{metrics}");
    assert_eq!(metrics["active"], 1);
    assert!(metrics["mrr"].as_i64().unwrap() > 0, "{metrics}");

    let (st, _) = call(&app, "POST", &format!("/v1/admin/subscriptions/{id}/pause"), Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, metrics) = call(&app, "GET", "/v1/admin/subscriptions/metrics", Some(token.as_str()), None).await;
    assert_eq!((metrics["active"].as_i64(), metrics["mrr"].as_i64()), (Some(0), Some(0)), "{metrics}");

    let (st, _) = call(&app, "POST", &format!("/v1/subscriptions/{id}/cancel"), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, metrics) = call(&app, "GET", "/v1/admin/subscriptions/metrics", Some(token.as_str()), None).await;
    assert_eq!(metrics["cancelled"], 1, "{metrics}");

    // The owner's finance summary carries the same recurring block.
    let (st, fin) = call(
        &app,
        "GET",
        "/v1/admin/finance/summary?period=custom&from=2026-03-01&to=2026-03-31",
        Some(token.as_str()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{fin}");
    assert!(fin.get("recurring").is_some(), "{fin}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn discount_policy_is_bounded_and_public(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, _) = app_and_state(pool.clone());
    let token = admin();
    let (st, _) = call(
        &app,
        "PUT",
        "/v1/admin/subscriptions/policy",
        Some(token.as_str()),
        Some(json!({ "discount_percent": 31 })),
    )
    .await;
    assert!(st.is_client_error(), "31% accepted: {st}");
    let (st, _) = call(
        &app,
        "PUT",
        "/v1/admin/subscriptions/policy",
        Some(token.as_str()),
        Some(json!({ "discount_percent": 15 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, public) = call(&app, "GET", "/v1/policy/subscription", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(public["discount_percent"], 15);
}

#[sqlx::test(migrations = "../../migrations")]
async fn planned_visits_hold_the_slot_so_a_later_subscription_is_placed_elsewhere(pool: PgPool) {
    seed_catalog(&pool).await;
    sqlx::query("INSERT INTO slot_capacity (start_hour, capacity) VALUES (9, 1) ON CONFLICT (start_hour) DO UPDATE SET capacity = 1")
        .execute(&pool)
        .await
        .expect("capacity");
    let (app, _) = app_and_state(pool.clone());
    let weekday = weekday_index(kst_today() + Duration::days(1));
    let first = customer(&app, &pool, "010-7000-0030").await;
    let second = customer(&app, &pool, "010-7000-0031").await;
    let (st, body) = subscribe(&app, &first, weekday, 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let (st, body) = subscribe(&app, &second, weekday, 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let planned = visits(&body);
    assert!(!planned.is_empty(), "{body}");
    // Every date already has the first subscription's visit at 9, which fills the one seat.
    assert!(planned.iter().all(|v| v["start_hour"] != 9), "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_paused_subscription_releases_its_planned_seats(pool: PgPool) {
    seed_catalog(&pool).await;
    sqlx::query("INSERT INTO slot_capacity (start_hour, capacity) VALUES (9, 1) ON CONFLICT (start_hour) DO UPDATE SET capacity = 1")
        .execute(&pool)
        .await
        .expect("capacity");
    let (app, _) = app_and_state(pool.clone());
    let weekday = weekday_index(kst_today() + Duration::days(1));
    let first = customer(&app, &pool, "010-7000-0034").await;
    let second = customer(&app, &pool, "010-7000-0035").await;
    let (_, body) = subscribe(&app, &first, weekday, 1, 9, None).await;
    let id = sub_id(&body);
    let (st, _) = call(&app, "POST", &format!("/v1/subscriptions/{id}/pause"), Some(first.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    // A paused subscription charges nothing, so its planned visits must not keep the 9 o'clock seat.
    let (st, body) = subscribe(&app, &second, weekday, 1, 9, None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert!(visits(&body).iter().all(|v| v["start_hour"] == 9), "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_full_slot_at_charge_time_is_retried_without_pausing_for_payment(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0032").await;
    let (_, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(1)), 1, 9, None).await;
    let id = sub_id(&body);
    close_slot(&pool, 9).await;

    // Three full-slot refusals in a row: none of them is a card decline, so the subscription stays active.
    for _ in 0..3 {
        sqlx::query("UPDATE subscription_visits SET retry_at = NULL WHERE subscription_id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(subscription_engine::pass(&state).await.expect("pass").failed, 1);
    }
    assert_eq!(sub_status(&pool, id).await, ("active".to_string(), None, 0));
    // The seat is checked before the card: nothing is charged, so nothing is refunded.
    let refunds: i64 = sqlx::query_scalar("SELECT count(*) FROM refund_queue").fetch_one(&pool).await.expect("refunds");
    assert_eq!(refunds, 0, "a full slot never charges the card");
    // The customer hears the real reason once, not a card problem and not on every retry.
    assert_eq!(count_notices(&pool, "subscription.payment_failed", Some(c.user)).await, 0);
    assert_eq!(count_notices(&pool, "subscription.slot_full", Some(c.user)).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_charge_still_open_at_day_end_is_settled_before_the_visit_is_missed(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0033").await;
    let tomorrow = kst_today() + Duration::days(1);
    let (_, body) = subscribe(&app, &c, weekday_index(tomorrow), 1, 9, None).await;
    let id = sub_id(&body);
    // The visit's day has come while its charge id is still stored (no answer from the gateway).
    sqlx::query("UPDATE subscription_visits SET visit_date = $2, payment_ref = 'subvisit-open-1' WHERE subscription_id = $1 AND visit_date = $3")
        .bind(id)
        .bind(kst_today())
        .bind(tomorrow)
        .execute(&pool)
        .await
        .expect("open charge");

    subscription_engine::pass(&state).await.expect("pass");
    let (state_now, payment_ref): (String, Option<String>) =
        sqlx::query_as("SELECT state, payment_ref FROM subscription_visits WHERE subscription_id = $1 AND visit_date = $2")
            .bind(id)
            .bind(kst_today())
            .fetch_one(&pool)
            .await
            .expect("visit");
    // The gateway never captured it, so the id is cleared and the visit is missed with nothing left open.
    assert_eq!((state_now.as_str(), payment_ref), ("missed", None));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_charge_left_open_when_the_visit_is_skipped_is_still_settled(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0036").await;
    let (_, body) = subscribe(&app, &c, weekday_index(kst_today() + Duration::days(2)), 1, 9, None).await;
    let id = sub_id(&body);
    // The customer skipped the visit while its charge id was still stored (the answer never came back).
    sqlx::query("UPDATE subscription_visits SET state = 'skipped', payment_ref = 'subvisit-skipped-1' WHERE subscription_id = $1 AND visit_date = $2")
        .bind(id)
        .bind(kst_today() + Duration::days(2))
        .execute(&pool)
        .await
        .expect("skipped charge");

    subscription_engine::pass(&state).await.expect("pass");
    let payment_ref: Option<String> = sqlx::query_scalar("SELECT payment_ref FROM subscription_visits WHERE subscription_id = $1 AND visit_date = $2")
        .bind(id)
        .bind(kst_today() + Duration::days(2))
        .fetch_one(&pool)
        .await
        .expect("visit");
    // The gateway has no such payment, so the id is cleared and the skipped visit keeps its state.
    assert_eq!(payment_ref, None);
}

async fn use_declining_card(db: &PgPool, c: &Customer) {
    let (sealed, digest) = api::crypto::seal_billing_key(&test_config().data_key, FAKE_DECLINE_KEY);
    sqlx::query("UPDATE payment_methods SET billing_key = $2, billing_key_digest = $3 WHERE id = $1")
        .bind(c.card)
        .bind(sealed)
        .bind(digest)
        .execute(db)
        .await
        .expect("decline card");
}

async fn visit_on(db: &PgPool, sub: Uuid, date: NaiveDate) -> (String, Option<String>, Option<DateTime<Utc>>) {
    sqlx::query_as("SELECT state, payment_ref, retry_at FROM subscription_visits WHERE subscription_id = $1 AND visit_date = $2")
        .bind(sub)
        .bind(date)
        .fetch_one(db)
        .await
        .expect("visit")
}

async fn refunds_of(db: &PgPool, tx_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM refund_queue WHERE provider = 'portone' AND tx_id = $1 AND done_at IS NOT NULL")
        .bind(tx_id)
        .fetch_one(db)
        .await
        .expect("refunds")
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_decline_too_late_for_a_retry_marks_the_visit_missed_and_says_so(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0037").await;
    use_declining_card(&pool, &c).await;
    let tomorrow = kst_today() + Duration::days(1);
    let (_, body) = subscribe(&app, &c, weekday_index(tomorrow), 1, 9, None).await;
    let id = sub_id(&body);

    // Declined two minutes before KST midnight: no later pass can still charge tomorrow's visit.
    let window_close = tomorrow.and_time(NaiveTime::MIN).and_utc() - Duration::hours(9);
    let report = subscription_engine::pass_at(&state, window_close - Duration::minutes(2)).await.expect("pass");
    assert_eq!(report.failed, 1, "{report:?}");
    let (visit_state, payment_ref, retry_at) = visit_on(&pool, id, tomorrow).await;
    assert_eq!((visit_state.as_str(), payment_ref, retry_at), ("missed", None, None));
    assert_eq!(count_notices(&pool, "subscription.payment_failed", Some(c.user)).await, 1);
    assert_eq!(count_notices(&pool, "subscription.visit_missed", Some(c.user)).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_visit_blocked_by_a_removed_card_says_why(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0038").await;
    subscribe(&app, &c, weekday_index(kst_today() + Duration::days(1)), 1, 9, None).await;
    sqlx::query("UPDATE payment_methods SET deleted_at = now() WHERE id = $1")
        .bind(c.card)
        .execute(&pool)
        .await
        .expect("remove card");

    assert_eq!(subscription_engine::pass(&state).await.expect("pass").failed, 1);
    assert_eq!(count_notices(&pool, "subscription.payment_failed", Some(c.user)).await, 0);
    assert_eq!(count_notices(&pool, "subscription.visit_blocked", Some(c.user)).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_charge_captured_for_a_visit_whose_day_came_is_refunded(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0039").await;
    let tomorrow = kst_today() + Duration::days(1);
    let (_, body) = subscribe(&app, &c, weekday_index(tomorrow), 1, 9, None).await;
    let id = sub_id(&body);
    // The gateway captured the charge but the answer never reached us, and the visit's day has come.
    sqlx::query("UPDATE subscription_visits SET visit_date = $2, payment_ref = 'dev:paid' WHERE subscription_id = $1 AND visit_date = $3")
        .bind(id)
        .bind(kst_today())
        .bind(tomorrow)
        .execute(&pool)
        .await
        .expect("captured charge");

    subscription_engine::pass(&state).await.expect("pass");
    let (visit_state, payment_ref, _) = visit_on(&pool, id, kst_today()).await;
    assert_eq!((visit_state.as_str(), payment_ref), ("missed", None));
    assert_eq!(refunds_of(&pool, "dev:paid").await, 1, "the captured money goes back");
    assert_eq!(count_notices(&pool, "subscription.visit_missed", Some(c.user)).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_charge_still_open_at_the_gateway_waits_before_the_visit_is_missed(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0040").await;
    let tomorrow = kst_today() + Duration::days(1);
    let (_, body) = subscribe(&app, &c, weekday_index(tomorrow), 1, 9, None).await;
    let id = sub_id(&body);
    // The gateway still reports the payment as open (not paid, not failed) when the day comes.
    sqlx::query("UPDATE subscription_visits SET visit_date = $2, payment_ref = 'dev:virtual_account' WHERE subscription_id = $1 AND visit_date = $3")
        .bind(id)
        .bind(kst_today())
        .bind(tomorrow)
        .execute(&pool)
        .await
        .expect("open charge");

    assert_eq!(subscription_engine::pass(&state).await.expect("pass").missed, 0);
    let (visit_state, payment_ref, _) = visit_on(&pool, id, kst_today()).await;
    // Nothing is decided until the gateway answers: the id is kept for the next pass and no refund is sent.
    assert_eq!((visit_state.as_str(), payment_ref.as_deref()), ("planned", Some("dev:virtual_account")));
    assert_eq!(refunds_of(&pool, "dev:virtual_account").await, 0);
    assert_eq!(count_notices(&pool, "subscription.visit_missed", Some(c.user)).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_charge_the_engine_refunded_itself_is_not_a_card_decline(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0041").await;
    let tomorrow = kst_today() + Duration::days(1);
    let (_, body) = subscribe(&app, &c, weekday_index(tomorrow), 1, 9, None).await;
    let id = sub_id(&body);
    // An earlier pass charged, the booking step then failed (a deadlock, say) and the engine refunded its own
    // charge; the gateway now reports that payment as CANCELLED.
    sqlx::query("UPDATE subscription_visits SET payment_ref = 'dev:cancelled' WHERE subscription_id = $1 AND visit_date = $2")
        .bind(id)
        .bind(tomorrow)
        .execute(&pool)
        .await
        .expect("stored charge");
    sqlx::query("INSERT INTO refund_queue (provider, tx_id, amount, cancellable, next_at, done_at) VALUES ('portone', 'dev:cancelled', 1, 1, now(), now())")
        .execute(&pool)
        .await
        .expect("own refund");

    assert_eq!(subscription_engine::pass(&state).await.expect("pass").failed, 1);
    assert_eq!(
        sub_status(&pool, id).await,
        ("active".to_string(), None, 0),
        "the engine's own refund is not a card decline"
    );
    assert_eq!(count_notices(&pool, "subscription.payment_failed", Some(c.user)).await, 0);
    let (visit_state, payment_ref, retry_at) = visit_on(&pool, id, tomorrow).await;
    assert_eq!((visit_state.as_str(), payment_ref), ("planned", None), "retried under a new charge id");
    assert!(retry_at.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_cancelled_charge_the_engine_never_refunded_still_counts_as_a_decline(pool: PgPool) {
    seed_catalog(&pool).await;
    let (app, state) = app_and_state(pool.clone());
    let c = customer(&app, &pool, "010-7000-0042").await;
    let tomorrow = kst_today() + Duration::days(1);
    let (_, body) = subscribe(&app, &c, weekday_index(tomorrow), 1, 9, None).await;
    let id = sub_id(&body);
    sqlx::query("UPDATE subscription_visits SET payment_ref = 'dev:cancelled' WHERE subscription_id = $1 AND visit_date = $2")
        .bind(id)
        .bind(tomorrow)
        .execute(&pool)
        .await
        .expect("stored charge");

    assert_eq!(subscription_engine::pass(&state).await.expect("pass").failed, 1);
    assert_eq!(sub_status(&pool, id).await.2, 1, "a payment cancelled elsewhere is still a failed card attempt");
    assert_eq!(count_notices(&pool, "subscription.payment_failed", Some(c.user)).await, 1);
}
