// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! An install in Europe/London: "today", visit slots, day windows, quiet hours and the cancel window follow
//! `install.tz` in Rust and in SQL. Its own test binary, because the install zone is process-wide.
//! Needs DATABASE_URL (sqlx::test).

mod common;

use api::{
    auth::Role,
    db, marketing,
    routes::checkout_recon,
    util::{self, LOCAL_TODAY},
};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use common::call;
use domain::{peak, zone};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

fn london_tz() -> zone::Tz {
    zone::get("Europe/London").expect("zone")
}

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).expect("instant").with_timezone(&Utc)
}

fn day(s: &str) -> NaiveDate {
    s.parse().expect("date")
}

/// The test database with `install.tz` stored as London, a pool whose connections carry it (as `db::connect` does
/// at boot), and the process zone set to it.
async fn london(pool: &PgPool) -> PgPool {
    sqlx::query("INSERT INTO settings (key, value) VALUES ('reg.install.tz', '\"Europe/London\"')")
        .execute(pool)
        .await
        .expect("store install.tz");
    let tz = util::load_install_tz(pool).await.expect("read install.tz");
    assert_eq!(zone::name(&tz), "Europe/London");
    let options = db::with_install_tz((*pool.connect_options()).clone(), &tz);
    util::set_install_tz(tz);
    PgPoolOptions::new().max_connections(4).connect_with(options).await.expect("london pool")
}

#[sqlx::test(migrations = "../../migrations")]
async fn today_and_day_windows_follow_the_install_zone(pool: PgPool) {
    let uk = london(&pool).await;
    let at = format!("SELECT {}", LOCAL_TODAY.replace("now()", "$1::timestamptz"));
    // (instant, London date, default-zone date): the evening gap in winter, and British Summer Time.
    for (instant, london_day, default_day) in [
        ("2026-11-15T15:30:00Z", "2026-11-15", "2026-11-16"),
        ("2026-11-15T23:59:59Z", "2026-11-15", "2026-11-16"),
        ("2026-11-16T00:00:00Z", "2026-11-16", "2026-11-16"),
        ("2026-07-15T23:30:00Z", "2026-07-16", "2026-07-16"),
        ("2026-07-15T22:59:59Z", "2026-07-15", "2026-07-16"),
    ] {
        let now = utc(instant);
        let sql_uk: NaiveDate = sqlx::query_scalar(&at).bind(now).fetch_one(&uk).await.expect("sql today");
        let sql_default: NaiveDate = sqlx::query_scalar(&at).bind(now).fetch_one(&pool).await.expect("sql today");
        assert_eq!((util::local_today_at(now), sql_uk), (day(london_day), day(london_day)), "London at {instant}");
        assert_eq!(
            sql_default,
            day(default_day),
            "a connection without dahae.tz keeps the default zone at {instant}"
        );
    }
    let before = util::local_today();
    let sql: NaiveDate = sqlx::query_scalar(&format!("SELECT {LOCAL_TODAY}")).fetch_one(&uk).await.expect("sql today");
    let after = util::local_today();
    assert!(sql == before || sql == after, "wall-clock today: SQL {sql}, Rust {before}..{after}");

    // A slot is a London wall-clock hour, in Rust and in SQL (the no-show and check-in queries' form).
    let summer = day("2026-07-01");
    assert_eq!(peak::slot_to_utc(&util::install_tz(), summer, 10), Some(utc("2026-07-01T09:00:00Z")));
    let slot: DateTime<Utc> = sqlx::query_scalar("SELECT ($1::date + make_interval(hours => 10)) AT TIME ZONE dahae_tz()")
        .bind(summer)
        .fetch_one(&uk)
        .await
        .expect("sql slot");
    assert_eq!(slot, utc("2026-07-01T09:00:00Z"));

    // Report days run London midnight to midnight, in Rust and in the SQL range filters.
    assert_eq!(
        checkout_recon::local_day_bounds(day("2026-11-16")),
        (utc("2026-11-16T00:00:00Z"), utc("2026-11-17T00:00:00Z"))
    );
    let start: DateTime<Utc> = sqlx::query_scalar("SELECT $1::date::timestamp AT TIME ZONE dahae_tz()")
        .bind(day("2026-07-16"))
        .fetch_one(&uk)
        .await
        .expect("sql day start");
    assert_eq!(start, utc("2026-07-15T23:00:00Z"));

    // Advertising quiet hours (21:00-08:00) are London hours.
    assert!(!marketing::quiet_hours(utc("2026-11-16T20:59:00Z")));
    assert!(marketing::quiet_hours(utc("2026-11-16T21:00:00Z")));
    assert_eq!(marketing::next_send_time(utc("2026-11-16T22:00:00Z")), utc("2026-11-17T08:00:00Z"));
    assert_eq!(marketing::next_send_time(utc("2026-07-16T22:00:00Z")), utc("2026-07-17T07:00:00Z"), "08:00 BST");
}

/// Previews for visits 0..=3 days out from London's today. `None` when London's date changed during the run.
async fn previews(pool: &PgPool, app: &axum::Router, token: &str, user: Uuid, tag: &str) -> Option<Vec<(i64, Value)>> {
    let today = zone::today(&london_tz(), Utc::now());
    let mut out = Vec::new();
    for days in 0..=3i64 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, paid_amount)
             VALUES ($1, $2, 'bathroom', '{}'::jsonb, '11680', $3, 10, 1, 52000, 52000, 0, 52000) RETURNING id",
        )
        .bind(format!("BKG-{tag}-{days}"))
        .bind(user)
        .bind(today + Duration::days(days))
        .fetch_one(pool)
        .await
        .expect("booking");
        let (st, preview) = call(app, "GET", &format!("/v1/bookings/{id}/cancel-preview"), Some(token), None).await;
        assert_eq!(st, StatusCode::OK, "{preview}");
        out.push((days, preview));
    }
    (zone::today(&london_tz(), Utc::now()) == today).then_some(out)
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_cancel_window_counts_london_days(pool: PgPool) {
    let uk = london(&pool).await;
    common::seed_catalog(&uk).await;
    let app = common::app(uk.clone());
    let (token, subject) = common::login(&app, "01070001002", "customer").await;
    let user: Uuid = subject.parse().expect("user id");
    common::staff_token(&uk, Role::StaffAdmin).await;
    let rows = match previews(&uk, &app, &token, user, "a").await {
        Some(rows) => rows,
        None => previews(&uk, &app, &token, user, "b").await.expect("crossed London midnight twice"),
    };
    // Default policy: free from 2 days before the visit, staff only after. Counted from London's date, so in the
    // hours when the default zone is already on the next day a visit 2 London days out is still free.
    for (days, preview) in rows {
        assert_eq!(preview["days_before"], json!(days), "{preview}");
        assert_eq!(preview["can_cancel"], json!(days >= 2), "visit in {days} London days: {preview}");
    }
}
