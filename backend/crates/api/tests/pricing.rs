//! Peak surcharge on fixed-price and subscription visits. The stored list price of each visit must equal
//! the base price plus the surcharge rule for its slot, rounded like quotes, and the owner's scope switch
//! turns the surcharge off for these visits. Quote estimates are covered in flows.rs.

mod common;

use api::{auth::Role, build_state, routes, routes::subscription_engine, state::AppState};
use axum::{Router, http::StatusCode};
use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use common::{Customer, call, customer, seed_catalog, staff_token, test_config};
use domain::{
    Won,
    catalog::default_catalog,
    peak::{PeakConfig, kst_to_utc, peak_surcharge},
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const VISIT_HOUR: u8 = 9;

fn kst_today() -> NaiveDate {
    (Utc::now() + Duration::hours(9)).date_naive()
}

/// Monday = 0, the convention the apps and partner prefs use.
fn weekday_index(d: NaiveDate) -> u8 {
    d.weekday().num_days_from_monday() as u8
}

/// The first Saturday at least two days out, so the booking clears the lead-time check.
fn next_saturday() -> NaiveDate {
    let mut d = kst_today() + Duration::days(2);
    while d.weekday() != Weekday::Sat {
        d += Duration::days(1);
    }
    d
}

fn studio_base() -> Won {
    let studio = default_catalog().into_iter().find(|s| s.code == "studio").expect("studio");
    studio.list_price(None).expect("fixed price").0
}

/// Expected list price for a studio visit at `visit` and the surcharge rate it should carry.
fn expected_list(visit: NaiveDate, config: &PeakConfig) -> (Won, i64) {
    let bps = peak_surcharge(config, kst_to_utc(visit, u32::from(VISIT_HOUR)).expect("slot")).bps;
    let base = studio_base();
    (base + base.times_bps(bps).round100(), bps)
}

/// The owner has switched the surcharge on for fixed-price and subscription visits.
fn scope_on() -> PeakConfig {
    PeakConfig {
        apply_to_fixed: true,
        ..PeakConfig::default()
    }
}

/// Quote answers for a standard two-room move, with the slot the customer picked.
fn quote_answers(visit: NaiveDate) -> serde_json::Value {
    json!({
        "cleaning_type": "standard", "luggage": "empty", "building": "commercial", "layout": "two_rooms",
        "duplex": false, "bathrooms": 1, "balconies": 0, "elevator": true, "floor_3_or_higher": false,
        "ac_addons": [], "visit_date": visit, "start_hour": VISIT_HOUR
    })
}

async fn set_peak(db: &PgPool, config: &PeakConfig) {
    sqlx::query("INSERT INTO settings (key, value) VALUES ('peak_pricing', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(sqlx::types::Json(config))
        .execute(db)
        .await
        .expect("peak config");
}

/// Books the studio service on a given KST date through the real router.
async fn book_on(app: &Router, c: &Customer, visit: NaiveDate) -> Uuid {
    let (st, b) = call(
        app,
        "POST",
        "/v1/bookings",
        Some(c.token.as_str()),
        Some(json!({
            "service_code": "studio",
            "address_id": c.address,
            "visit_date": visit,
            "start_hour": VISIT_HOUR,
            "coupon_id": None::<Uuid>,
            "visit_notes": {},
            "payment_method_id": c.card
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "booking: {b}");
    b["id"].as_str().expect("booking id").parse().expect("uuid")
}

async fn stored_list_price(db: &PgPool, id: Uuid) -> (i64, i64, i64) {
    sqlx::query_as("SELECT list_price, discount, paid_amount FROM bookings WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
        .expect("booking row")
}

#[sqlx::test(migrations = "../../migrations")]
async fn fixed_booking_on_a_weekend_carries_the_surcharge_in_its_list_price(pool: PgPool) {
    seed_catalog(&pool).await;
    set_peak(&pool, &scope_on()).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5100-0001").await;
    let visit = next_saturday();
    let id = book_on(&app, &c, visit).await;
    let (list, discount, paid) = stored_list_price(&pool, id).await;
    let (want, bps) = expected_list(visit, &scope_on());
    assert!(bps > 0, "a Saturday carries the weekend surcharge");
    assert_eq!(list, want.0, "base plus surcharge");
    assert_eq!(list - discount, paid, "the stored breakdown adds up to the amount paid");
}

#[sqlx::test(migrations = "../../migrations")]
async fn fixed_booking_pays_base_price_by_default(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5100-0002").await;
    let id = book_on(&app, &c, next_saturday()).await;
    let (list, _, _) = stored_list_price(&pool, id).await;
    assert_eq!(list, studio_base().0, "no surcharge on a fixed visit until the owner turns the scope on");
}

#[sqlx::test(migrations = "../../migrations")]
async fn fixed_booking_pays_base_price_when_the_owner_turns_peak_off_for_fixed_visits(pool: PgPool) {
    seed_catalog(&pool).await;
    set_peak(&pool, &PeakConfig::default()).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5100-0004").await;
    let id = book_on(&app, &c, next_saturday()).await;
    let (list, _, _) = stored_list_price(&pool, id).await;
    assert_eq!(list, studio_base().0, "no surcharge on a fixed visit when the owner scope is off");
}

#[sqlx::test(migrations = "../../migrations")]
async fn quote_preview_and_fixed_booking_apply_the_same_slot_rate(pool: PgPool) {
    seed_catalog(&pool).await;
    set_peak(&pool, &scope_on()).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5100-0005").await;
    let visit = next_saturday();
    let (st, quote) = call(&app, "POST", "/v1/quote/estimate", None, Some(quote_answers(visit))).await;
    assert_eq!(st, StatusCode::OK, "{quote}");
    let (want, bps) = expected_list(visit, &scope_on());
    assert_eq!(quote["peak_bps"].as_i64(), Some(bps), "quote shows the slot's rate: {quote}");
    let id = book_on(&app, &c, visit).await;
    let (list, _, _) = stored_list_price(&pool, id).await;
    assert_eq!(list, want.0, "fixed booking applies the same rate, rounded to the hundred won");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_peak_save_must_state_the_fixed_scope(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let (token, _) = staff_token(&pool, Role::StaffAdmin).await;
    let mut body = serde_json::to_value(PeakConfig::default()).expect("json");
    body.as_object_mut().expect("object").remove("apply_to_fixed");
    let (st, b) = call(&app, "PUT", "/v1/admin/peak-pricing", Some(token.as_str()), Some(body.clone())).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["error"]["code"], "apply_to_fixed_required");
    body["apply_to_fixed"] = json!(true);
    let (st, b) = call(&app, "PUT", "/v1/admin/peak-pricing", Some(token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn subscription_visit_carries_the_surcharge_for_its_slot(pool: PgPool) {
    seed_catalog(&pool).await;
    let state: AppState = build_state(pool.clone(), test_config()).expect("state");
    let app = routes::router(state.clone());
    set_peak(&pool, &scope_on()).await;
    let c = customer(&app, &pool, "010-5100-0003").await;
    let tomorrow = kst_today() + Duration::days(1);
    let (st, body) = call(
        &app,
        "POST",
        "/v1/subscriptions",
        Some(c.token.as_str()),
        Some(json!({
            "service_code": "studio",
            "address_id": c.address,
            "payment_method_id": c.card,
            "cadence_weeks": 1,
            "weekday": weekday_index(tomorrow),
            "start_hour": VISIT_HOUR,
            "preferred_pro_id": None::<Uuid>,
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let report = subscription_engine::pass(&state).await.expect("pass");
    assert_eq!(report.booked, 1, "{report:?}");
    let sub_id: Uuid = body["subscription"]["id"].as_str().expect("sub id").parse().expect("uuid");
    let (visit, list): (NaiveDate, i64) = sqlx::query_as("SELECT visit_date, list_price FROM bookings WHERE subscription_id = $1")
        .bind(sub_id)
        .fetch_one(&pool)
        .await
        .expect("visit booking");
    let (want, _) = expected_list(visit, &scope_on());
    assert_eq!(list, want.0, "subscription visit uses the same rule as a fixed booking");
}
