// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Admin booking detail: the estimate breakdown of a weekend booking carries its peak surcharge,
//! so its total equals the estimate the customer was quoted and charged against.

mod common;

use api::auth::Role;
use axum::http::StatusCode;
use chrono::{Datelike, Duration, Weekday};
use common::{call, customer, seed_catalog, staff_token};
use serde_json::{Value, json};
use sqlx::PgPool;

const MOVE_ANSWERS: &str = r#"{"cleaning_type":"standard","luggage":"empty","building":"apartment","layout":"two_rooms",
  "duplex":false,"bathrooms":1,"balconies":1,"elevator":true,"floor_3_or_higher":true}"#;

/// The first Saturday at least three days out, so the slot is bookable and carries the weekend rate.
fn next_saturday() -> chrono::NaiveDate {
    let mut day = api::util::local_today() + Duration::days(3);
    while day.weekday() != Weekday::Sat {
        day += Duration::days(1);
    }
    day
}

#[sqlx::test(migrations = "../../migrations")]
async fn weekend_booking_breakdown_total_equals_its_estimate(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0042").await;
    let answers: Value = serde_json::from_str(MOVE_ANSWERS).expect("answers");
    let (st, booked) = call(
        &app,
        "POST",
        "/v1/bookings",
        Some(c.token.as_str()),
        Some(json!({
            "service_code": "move_out", "address_id": c.address, "visit_date": next_saturday(), "start_hour": 8,
            "answers": answers, "visit_notes": {}
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "quote request: {booked}");
    let id = booked["id"].as_str().expect("booking id");
    let estimate = booked["estimate"].as_i64().expect("estimate");

    let (staff, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (st, detail) = call(&app, "GET", &format!("/v1/admin/bookings/{id}"), Some(staff.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "booking detail: {detail}");
    let breakdown = &detail["estimate_breakdown"];
    assert!(breakdown["peak_bps"].as_i64().unwrap_or_default() > 0, "weekend rate missing: {breakdown}");
    assert!(breakdown["peak"].as_i64().unwrap_or_default() > 0, "weekend peak line missing: {breakdown}");
    assert_eq!(breakdown["total"].as_i64(), Some(estimate), "breakdown must equal the quoted estimate");
}
