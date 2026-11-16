// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The console's estimate breakdown for a booking carries the same peak surcharge the booking was
//! priced with, so the desk never shows a lower auto-estimate than the booking total (K-05).

mod common;

use api::auth::{self, Role};
use axum::http::StatusCode;
use chrono::{Datelike, Duration, Weekday};
use common::{SECRET, call, customer, seed_catalog};
use serde_json::{Value, json};
use sqlx::PgPool;

const MOVE_ANSWERS: &str = r#"{"cleaning_type":"standard","luggage":"empty","building":"apartment","layout":"two_rooms",
  "duplex":false,"bathrooms":1,"balconies":1,"elevator":true,"floor_3_or_higher":true}"#;

async fn staff_admin_token(pool: &PgPool) -> String {
    let hash = auth::hash_password("staff-pass-12345").expect("hash");
    let id: uuid::Uuid = sqlx::query_scalar("INSERT INTO staff (email, name, password_hash, role) VALUES ($1, 'Desk', $2, 'admin') RETURNING id")
        .bind("desk-admin@test.local")
        .bind(hash)
        .fetch_one(pool)
        .await
        .expect("staff row");
    auth::issue_access(SECRET, id, Role::StaffAdmin).expect("token")
}

/// The next Saturday in KST, so the slot falls in the weekend peak.
fn next_saturday() -> chrono::NaiveDate {
    let mut day = api::util::today_kst() + Duration::days(1);
    while day.weekday() != Weekday::Sat {
        day += Duration::days(1);
    }
    day
}

#[sqlx::test(migrations = "../../migrations")]
async fn breakdown_includes_the_slot_peak_so_it_matches_the_booking_total(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let c = customer(&app, &pool, "010-5400-0042").await;
    let answers: Value = serde_json::from_str(MOVE_ANSWERS).expect("answers");
    let (st, booking) = call(
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
    assert_eq!(st, StatusCode::OK, "quote request: {booking}");
    let id = booking["id"].as_str().expect("booking id").to_string();
    let total = booking["estimate"].as_i64().expect("estimate");

    let token = staff_admin_token(&pool).await;
    let (st, detail) = call(&app, "GET", &format!("/v1/admin/bookings/{id}"), Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "booking detail: {detail}");
    let breakdown = &detail["estimate_breakdown"];
    assert!(breakdown["peak"].as_i64().unwrap_or(0) > 0, "weekend peak line missing: {breakdown}");
    assert_eq!(breakdown["total"].as_i64(), Some(total), "desk total differs from the booking: {breakdown}");
}
