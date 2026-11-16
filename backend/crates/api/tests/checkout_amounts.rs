//! What the customer is told to pay comes from the server: the booking price preview (peak, welcome,
//! coupon), the `expected_total` guard on pay, and the amount on a virtual account, at confirm and
//! when the account is opened again later. Same DB and fakes as flows.rs.

mod common;

use api::points;
use axum::{Router, http::StatusCode};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use common::{Customer, app_with_channels, call, customer, seed_catalog};
use domain::peak::PeakConfig;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn set_setting(pool: &PgPool, key: &str, value: Value) {
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .expect("setting");
}

/// The first Saturday at least two days out (KST), so the booking clears the lead-time check.
fn next_saturday() -> NaiveDate {
    let mut d = api::util::today_kst() + Duration::days(2);
    while d.weekday() != Weekday::Sat {
        d += Duration::days(1);
    }
    d
}

fn weekday_in(days: i64) -> NaiveDate {
    let mut d = api::util::today_kst() + Duration::days(days);
    while matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
        d += Duration::days(1);
    }
    d
}

fn studio(c: &Customer, visit: NaiveDate, coupon: Option<Uuid>) -> Value {
    json!({ "service_code": "studio", "address_id": c.address, "visit_date": visit, "start_hour": 9,
            "coupon_id": coupon, "visit_notes": {} })
}

async fn price(app: &Router, c: &Customer, body: Value) -> Value {
    let (st, p) = call(app, "POST", "/v1/bookings/price", Some(c.token.as_str()), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{p}");
    p
}

fn int(v: &Value, key: &str) -> i64 {
    v[key].as_i64().unwrap_or_else(|| panic!("{key} in {v}"))
}

async fn bookings_of(pool: &PgPool, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM bookings WHERE user_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .expect("count")
}

#[sqlx::test(migrations = "../../migrations")]
async fn virtual_account_carries_the_charge_after_points_at_confirm_and_when_listed(pool: PgPool) {
    seed_catalog(&pool).await;
    set_setting(
        &pool,
        "points_policy",
        json!({ "enabled": true, "earn_bps": 1000, "welcome_points": 0, "referral_points": 0,
                "expiry_days": 365, "notify_days": 30, "max_use_bps": 10000 }),
    )
    .await;
    let app = app_with_channels(pool.clone(), &["card"]);
    let c = customer(&app, &pool, "010-6410-0001").await;
    let mut conn = pool.acquire().await.expect("conn");
    points::grant(&mut conn, c.user, "grant", 5_000, "seed:va".into()).await.expect("grant");
    let token = Some(c.token.as_str());

    let body = json!({ "kind": "booking", "method": "card", "points": 5_000, "booking": studio(&c, weekday_in(10), None) });
    let (st, intent) = call(&app, "POST", "/v1/checkouts", token, Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{intent}");
    let charge = int(&intent, "amount");
    assert_eq!(charge, int(&intent, "total") - 5_000, "{intent}");

    let uri = format!("/v1/checkouts/{}/confirm", intent["id"].as_str().expect("id"));
    let (st, done) = call(&app, "POST", &uri, token, Some(json!({ "payment_id": "dev:va" }))).await;
    assert_eq!((st, done["status"].as_str()), (StatusCode::OK, Some("awaiting_deposit")), "{done}");
    let va = &done["virtual_account"];
    assert_eq!(va["amount"].as_i64(), Some(charge), "deposit the charge, not the total: {done}");
    assert_eq!(va["account_number"], "123-456-789012", "{done}");
    assert_eq!(va["bank"], "KB", "{done}");

    // Opened again later (bookings banner, another device): the same amount.
    let (st, list) = call(&app, "GET", "/v1/checkouts?status=awaiting_deposit", token, None).await;
    assert_eq!(st, StatusCode::OK, "{list}");
    let later = &list[0]["virtual_account"];
    assert_eq!(later["amount"].as_i64(), Some(charge), "{list}");
    assert_eq!(later["account_number"], "123-456-789012", "{list}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn price_preview_shows_the_peak_surcharge_the_booking_charges(pool: PgPool) {
    seed_catalog(&pool).await;
    let scope_on = PeakConfig {
        apply_to_fixed: true,
        ..PeakConfig::default()
    };
    set_setting(&pool, "peak_pricing", serde_json::to_value(&scope_on).expect("peak")).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-6410-0002").await;
    let visit = next_saturday();

    let p = price(&app, &c, studio(&c, visit, None)).await;
    let surcharge = int(&p, "peak_surcharge");
    assert!(surcharge > 0, "a Saturday carries the weekend surcharge: {p}");
    assert_eq!(int(&p, "total"), int(&p, "list_price") - int(&p, "multi_discount") - int(&p, "discount"), "{p}");

    // The base price the customer would have seen without the server: refused, nothing charged.
    let mut stale = studio(&c, visit, None);
    stale["payment_method_id"] = json!(c.card);
    stale["expected_total"] = json!(int(&p, "total") - surcharge);
    let (st, b) = call(&app, "POST", "/v1/bookings", Some(c.token.as_str()), Some(stale.clone())).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("price_changed")), "{b}");
    assert_eq!(bookings_of(&pool, c.user).await, 0, "no booking, no charge");

    stale["expected_total"] = json!(int(&p, "total"));
    let (st, b) = call(&app, "POST", "/v1/bookings", Some(c.token.as_str()), Some(stale)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let paid: i64 = sqlx::query_scalar("SELECT paid_amount FROM bookings WHERE id = $1::uuid")
        .bind(b["id"].as_str().expect("id"))
        .fetch_one(&pool)
        .await
        .expect("paid");
    assert_eq!(paid, int(&p, "total"), "charged exactly what the preview showed");
}

#[sqlx::test(migrations = "../../migrations")]
async fn price_preview_shows_the_welcome_offer_and_a_coupon_replaces_it(pool: PgPool) {
    seed_catalog(&pool).await;
    set_setting(&pool, "welcome_offer", json!({ "enabled": true, "kind": "amount", "value": 5000 })).await;
    let app = app_with_channels(pool.clone(), &["card"]);
    let c = customer(&app, &pool, "010-6410-0003").await;
    let visit = weekday_in(10);

    let p = price(&app, &c, studio(&c, visit, None)).await;
    assert_eq!((int(&p, "discount"), p["welcome"].as_bool()), (5_000, Some(true)), "{p}");
    assert_eq!(int(&p, "total"), int(&p, "list_price") - 5_000, "{p}");

    let coupon: Uuid =
        sqlx::query_scalar("INSERT INTO coupons (user_id, name, amount, expires_at) VALUES ($1, 'manual', 2000, now() + interval '30 days') RETURNING id")
            .bind(c.user)
            .fetch_one(&pool)
            .await
            .expect("coupon");
    let with_coupon = price(&app, &c, studio(&c, visit, Some(coupon))).await;
    assert_eq!(
        (int(&with_coupon, "discount"), with_coupon["welcome"].as_bool()),
        (2_000, Some(false)),
        "{with_coupon}"
    );

    // A checkout for the list price (welcome not shown) is refused before any PG window opens.
    let body = json!({ "kind": "booking", "method": "card", "expected_total": int(&p, "list_price"), "booking": studio(&c, visit, None) });
    let (st, b) = call(&app, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(body)).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("price_changed")), "{b}");
    let body = json!({ "kind": "booking", "method": "card", "expected_total": int(&p, "total"), "booking": studio(&c, visit, None) });
    let (st, intent) = call(&app, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(body)).await;
    assert_eq!((st, intent["amount"].as_i64()), (StatusCode::OK, Some(int(&p, "total"))), "{intent}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn price_preview_refuses_quote_services(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-6410-0004").await;
    let mut body = studio(&c, weekday_in(10), None);
    body["service_code"] = json!("move_in");
    body["start_hour"] = json!(8);
    let (st, b) = call(&app, "POST", "/v1/bookings/price", Some(c.token.as_str()), Some(body)).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("quote_not_payable")), "{b}");
}
