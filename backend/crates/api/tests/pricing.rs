//! Peak surcharge on fixed-price and subscription visits. The stored list price of each visit must equal
//! the base price plus the surcharge rule for its slot, rounded like quotes, and the owner's scope switch
//! turns the surcharge off for these visits. Quote estimates are covered in flows.rs.

mod common;

use api::{build_state, routes, routes::subscription_engine, state::AppState};
use axum::{Router, http::StatusCode};
use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use common::{Customer, call, customer, seed_catalog, test_config};
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
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5100-0001").await;
    let visit = next_saturday();
    let id = book_on(&app, &c, visit).await;
    let (list, discount, paid) = stored_list_price(&pool, id).await;
    let (want, bps) = expected_list(visit, &PeakConfig::default());
    assert!(bps > 0, "a Saturday carries the weekend surcharge");
    assert_eq!(list, want.0, "base plus surcharge");
    assert_eq!(list - discount, paid, "the stored breakdown adds up to the amount paid");
}

#[sqlx::test(migrations = "../../migrations")]
async fn fixed_booking_pays_base_price_when_the_owner_turns_peak_off_for_fixed_visits(pool: PgPool) {
    seed_catalog(&pool).await;
    set_peak(
        &pool,
        &PeakConfig {
            apply_to_fixed: false,
            ..PeakConfig::default()
        },
    )
    .await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5100-0002").await;
    let id = book_on(&app, &c, next_saturday()).await;
    let (list, _, _) = stored_list_price(&pool, id).await;
    assert_eq!(list, studio_base().0, "no surcharge on a fixed visit when the owner scope is off");
}

#[sqlx::test(migrations = "../../migrations")]
async fn subscription_visit_carries_the_surcharge_for_its_slot(pool: PgPool) {
    seed_catalog(&pool).await;
    let state: AppState = build_state(pool.clone(), test_config()).expect("state");
    let app = routes::router(state.clone());
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
    let (want, _) = expected_list(visit, &PeakConfig::default());
    assert_eq!(list, want.0, "subscription visit uses the same rule as a fixed booking");
}
