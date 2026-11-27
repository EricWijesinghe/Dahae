// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Owner finance over a migrated Postgres: the summary, partner and CSV numbers match hand-computed seed rows.
//! Needs DATABASE_URL (sqlx::test). Rows are inserted directly so every figure is known in advance.

mod common;

use api::auth::{self, Role};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::{SECRET, call, seed_catalog};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const PERIOD: &str = "/v1/admin/finance/summary?period=custom&from=2026-03-01&to=2026-03-31";

async fn insert_booking(db: &PgPool, user: Uuid, number: &str, final_price: i64, discount: i64, status: i16, created: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, cancelled_at, created_at)
         VALUES ($1, $2, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, $3, $4, $4, $5,
                 CASE WHEN $3 = -1 THEN $6::timestamptz + interval '1 day' END, $6::timestamptz)
         RETURNING id",
    )
    .bind(number)
    .bind(user)
    .bind(status)
    .bind(final_price)
    .bind(discount)
    .bind(created)
    .fetch_one(db)
    .await
    .expect("booking")
}

#[allow(clippy::too_many_arguments)]
async fn insert_payment(db: &PgPool, booking: Uuid, amount: i64, refunded: i64, status: &str, key: &str, created: &str) {
    sqlx::query(
        "INSERT INTO payments (booking_id, amount, kind, status, refunded_amount, provider, idempotency_key, created_at)
         VALUES ($1, $2, 'full', $3, $4, 'fake', $5, $6::timestamptz)",
    )
    .bind(booking)
    .bind(amount)
    .bind(status)
    .bind(refunded)
    .bind(key)
    .bind(created)
    .execute(db)
    .await
    .expect("payment");
}

#[allow(clippy::too_many_arguments)]
async fn insert_settlement(db: &PgPool, pro: Uuid, booking: Uuid, gross: i64, fee: i64, state: &str, payout: &str, paid_at: Option<&str>, created: &str) {
    sqlx::query(
        "INSERT INTO settlements (pro_id, booking_id, gross, fee, net, payout_date, state, paid_at, created_at)
         VALUES ($1, $2, $3, $4, $3 - $4, $5::date, $6, $7::timestamptz, $8::timestamptz)",
    )
    .bind(pro)
    .bind(booking)
    .bind(gross)
    .bind(fee)
    .bind(payout)
    .bind(state)
    .bind(paid_at)
    .bind(created)
    .execute(db)
    .await
    .expect("settlement");
}

/// Three March bookings plus one out-of-period booking, with payments, settlements and checkouts.
async fn seed_march(db: &PgPool) -> Uuid {
    seed_catalog(db).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000001', 'fin') RETURNING id")
        .fetch_one(db)
        .await
        .expect("user");
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone, name, team_name, status) VALUES ('01070000101', 'pro', 'team', 'verified') RETURNING id")
        .fetch_one(db)
        .await
        .expect("pro");

    // Completed and paid out on 2026-03-11.
    let b1 = insert_booking(db, user, "BKF-1", 100_000, 10_000, 3, "2026-03-05T10:00:00+09").await;
    insert_payment(db, b1, 90_000, 0, "paid", "fin-1", "2026-03-05T10:00:00+09").await;
    insert_settlement(
        db,
        pro,
        b1,
        100_000,
        20_000,
        "paid",
        "2026-03-11",
        Some("2026-03-11T12:00:00+09"),
        "2026-03-06T09:00:00+09",
    )
    .await;

    // Cancelled after payment: full refund on the cancel day (2026-03-09).
    let b2 = insert_booking(db, user, "BKF-2", 50_000, 0, -1, "2026-03-08T10:00:00+09").await;
    insert_payment(db, b2, 50_000, 50_000, "refunded", "fin-2", "2026-03-08T10:00:00+09").await;

    // Completed, awaiting the 2026-03-25 payout.
    let b3 = insert_booking(db, user, "BKF-3", 200_000, 0, 3, "2026-03-20T10:00:00+09").await;
    insert_payment(db, b3, 200_000, 0, "paid", "fin-3", "2026-03-20T10:00:00+09").await;
    insert_settlement(db, pro, b3, 200_000, 40_000, "pending", "2026-03-25", None, "2026-03-21T09:00:00+09").await;

    // Outside March: must not leak into any March figure.
    let old = insert_booking(db, user, "BKF-0", 999_000, 0, 3, "2026-02-27T10:00:00+09").await;
    insert_payment(db, old, 999_000, 0, "paid", "fin-0", "2026-02-27T10:00:00+09").await;

    for (key, method, amount, status) in [
        ("ck-1", "card", 90_000, "paid"),
        ("ck-2", "kakaopay", 200_000, "paid"),
        ("ck-3", "card", 50_000, "failed"),
    ] {
        sqlx::query(
            "INSERT INTO checkouts (user_id, kind, method, amount, status, expires_at, created_at, provider_payment_id)
             VALUES ($1, 'booking', $2, $3, $4, now() + interval '1 day', '2026-03-10T10:00:00+09', $5)",
        )
        .bind(user)
        .bind(method)
        .bind(amount)
        .bind(status)
        .bind(key)
        .execute(db)
        .await
        .expect("checkout");
    }
    pro
}

/// A live admin row with a fresh step-up: the exports check the stored role and the step-up.
async fn admin_token(db: &PgPool) -> String {
    common::staff_token(db, Role::StaffAdmin).await.0
}

#[sqlx::test(migrations = "../../migrations")]
async fn summary_matches_the_seeded_march_numbers(pool: PgPool) {
    seed_march(&pool).await;
    let app = common::app(pool.clone());
    let (status, s) = call(&app, "GET", PERIOD, Some(&admin_token(&pool).await), None).await;
    assert_eq!(status, StatusCode::OK, "{s}");
    assert_eq!(s["bookings"], 3);
    assert_eq!(s["cancelled"], 1);
    assert_eq!(s["cancellation_bps"], 3_333);
    assert_eq!(s["avg_order_value"], 150_000, "(100k + 200k) / 2 active");
    assert_eq!(s["discount"], 10_000);
    assert_eq!(s["gross"], 340_000);
    assert_eq!(s["refunds"], 50_000);
    assert_eq!(s["net_revenue"], 290_000);
    assert_eq!(s["platform_fee"], 60_000);
    assert_eq!(s["vat_estimate"], 6_000);
    assert_eq!(s["payouts"]["paid"], 80_000);
    assert_eq!(s["payouts"]["pending"], 160_000);
    assert_eq!(s["payouts"]["held"], 0);
    assert_eq!(s["vat_basis"], "platform_fee_10pct_estimate");
    let methods = s["payment_methods"].as_array().expect("methods");
    assert_eq!(methods.len(), 2, "failed checkout is excluded: {s}");
    assert_eq!(methods[0]["method"], "kakaopay");
    assert_eq!(methods[0]["amount"], 200_000);
    assert_eq!(methods[1]["method"], "card");
    assert_eq!(methods[1]["amount"], 90_000);
}

#[sqlx::test(migrations = "../../migrations")]
async fn partner_table_and_service_table_agree_with_the_summary(pool: PgPool) {
    seed_march(&pool).await;
    let app = common::app(pool.clone());
    let token = admin_token(&pool).await;
    let (_, partners) = call(
        &app,
        "GET",
        "/v1/admin/finance/partners?period=custom&from=2026-03-01&to=2026-03-31",
        Some(&token),
        None,
    )
    .await;
    let p = &partners[0];
    assert_eq!(p["name"], "pro");
    assert_eq!(p["jobs"], 2);
    assert_eq!(p["settled_gross"], 300_000);
    assert_eq!(p["fee"], 60_000);
    assert_eq!(p["net"], 240_000);
    assert_eq!(p["paid"], 80_000);
    assert_eq!(p["pending"], 160_000);

    let (_, services) = call(
        &app,
        "GET",
        "/v1/admin/finance/services?period=custom&from=2026-03-01&to=2026-03-31",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(services[0]["service_code"], "studio");
    assert_eq!(services[0]["gross"], 340_000);
    assert_eq!(services[0]["net_revenue"], 290_000);
}

#[sqlx::test(migrations = "../../migrations")]
async fn series_shapes_and_grain_validation(pool: PgPool) {
    seed_march(&pool).await;
    let app = common::app(pool.clone());
    let token = admin_token(&pool).await;
    let (status, days) = call(&app, "GET", "/v1/admin/finance/series?grain=day&days=30", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "{days}");
    assert_eq!(days.as_array().map(Vec::len), Some(30));
    let (_, weeks) = call(&app, "GET", "/v1/admin/finance/series?grain=week", Some(&token), None).await;
    assert_eq!(weeks.as_array().map(Vec::len), Some(12));
    let (_, months) = call(&app, "GET", "/v1/admin/finance/series?grain=month", Some(&token), None).await;
    assert_eq!(months.as_array().map(Vec::len), Some(12));
    let (status, _) = call(&app, "GET", "/v1/admin/finance/series?grain=year", Some(&token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn exports_are_bom_csv_with_guarded_cells_and_admin_only(pool: PgPool) {
    seed_march(&pool).await;
    let app = common::app(pool.clone());
    let token = admin_token(&pool).await;
    let uri = "/v1/admin/finance/export/monthly?period=custom&from=2026-03-01&to=2026-03-31";
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request");
    let resp = app.clone().oneshot(req).await.expect("router");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
    let text = String::from_utf8(bytes.to_vec()).expect("utf8");
    assert!(text.starts_with('\u{feff}'), "BOM first");
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines[0].split(',').count(), 11);
    assert_eq!(lines[1], "2026-03,3,1,340000,50000,290000,60000,6000,80000,160000,0");
    assert!(lines.last().expect("total").starts_with("합계,"));

    // A consultant (staff, not owner) may not export, and the summary carries payouts so it is admin-only too.
    let consultant = auth::issue_access(SECRET, Uuid::now_v7(), Role::StaffConsultant).expect("token");
    let (s, _) = call(&app, "GET", uri, Some(&consultant), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "GET", PERIOD, Some(&consultant), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn consultant_cannot_read_payout_series(pool: PgPool) {
    seed_march(&pool).await;
    let app = common::app(pool.clone());
    let consultant = auth::issue_access(SECRET, Uuid::now_v7(), Role::StaffConsultant).expect("token");
    for uri in [
        "/v1/admin/finance/series?grain=week",
        "/v1/admin/finance/partners?period=custom&from=2026-03-01&to=2026-03-31",
        "/v1/admin/finance/services?period=custom&from=2026-03-01&to=2026-03-31",
        "/v1/admin/settlements",
    ] {
        let (s, _) = call(&app, "GET", uri, Some(&consultant), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn held_settlements_stay_out_of_fee_vat_and_summary(pool: PgPool) {
    let pro = seed_march(&pool).await;
    let user: Uuid = sqlx::query_scalar("SELECT user_id FROM bookings WHERE number = 'BKF-1'")
        .fetch_one(&pool)
        .await
        .expect("user");
    let held = insert_booking(&pool, user, "BKF-4", 100_000, 0, 3, "2026-03-22T10:00:00+09").await;
    insert_settlement(&pool, pro, held, 100_000, 30_000, "held", "2026-03-25", None, "2026-03-22T12:00:00+09").await;
    let app = common::app(pool.clone());
    let token = admin_token(&pool).await;
    let (_, s) = call(&app, "GET", PERIOD, Some(&token), None).await;
    assert_eq!(s["platform_fee"], 60_000, "held fee is not earned yet");
    assert_eq!(s["vat_estimate"], 6_000);
    assert_eq!(s["payouts"]["held"], 70_000);
    let csv = fetch_csv(&app, "/v1/admin/finance/export/vat?year=2026&quarter=1", &token).await;
    assert!(csv.lines().any(|l| l == "플랫폼수수료(과세표준 추정),0,60000"), "{csv}");
    assert!(csv.lines().any(|l| l == "부가세 추정(수수료의 10%),0,6000"), "{csv}");
    assert!(csv.lines().any(|l| l == "환불액,0,50000"), "{csv}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn dashboard_revenue_matches_the_finance_summary(pool: PgPool) {
    seed_catalog(&pool).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000002', 'recent') RETURNING id")
        .fetch_one(&pool)
        .await
        .expect("user");
    let recent = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    let b = insert_booking(&pool, user, "BKD-1", 80_000, 0, 3, &recent).await;
    insert_payment(&pool, b, 80_000, 10_000, "partially_refunded", "dash-1", &recent).await;
    let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
    let from = today - chrono::Duration::days(29);
    let app = common::app(pool.clone());
    let token = admin_token(&pool).await;
    let (_, dash) = call(&app, "GET", "/v1/admin/dashboard", Some(&token), None).await;
    let (_, s) = call(
        &app,
        "GET",
        &format!("/v1/admin/finance/summary?period=custom&from={from}&to={today}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(dash["revenue_30d"], 70_000);
    assert_eq!(dash["revenue_30d"], s["net_revenue"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn dashboard_refund_dated_by_cancel_day_counts_in_the_window(pool: PgPool) {
    seed_catalog(&pool).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000003', 'refund') RETURNING id")
        .fetch_one(&pool)
        .await
        .expect("user");
    // Paid 40 days ago (outside the window), cancelled and refunded yesterday (inside it).
    let paid = (chrono::Utc::now() - chrono::Duration::days(40)).to_rfc3339();
    let cancelled = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    let b = insert_booking(&pool, user, "BKD-2", 80_000, 0, -1, &paid).await;
    insert_payment(&pool, b, 80_000, 80_000, "refunded", "dash-2", &paid).await;
    sqlx::query("UPDATE bookings SET cancelled_at = $2::timestamptz WHERE id = $1")
        .bind(b)
        .bind(&cancelled)
        .execute(&pool)
        .await
        .expect("cancel");
    let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
    let from = today - chrono::Duration::days(29);
    let app = common::app(pool.clone());
    let token = admin_token(&pool).await;
    let (_, dash) = call(&app, "GET", "/v1/admin/dashboard", Some(&token), None).await;
    let (_, s) = call(
        &app,
        "GET",
        &format!("/v1/admin/finance/summary?period=custom&from={from}&to={today}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(s["net_revenue"], -80_000);
    assert_eq!(dash["revenue_30d"], -80_000);
    assert_eq!(dash["revenue_30d"], s["net_revenue"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn partner_earnings_show_the_next_payout_and_hide_from_members(pool: PgPool) {
    let pro = seed_march(&pool).await;
    let app = common::app(pool.clone());
    let token = auth::issue_access(SECRET, pro, Role::Pro).expect("token");
    let (status, e) = call(&app, "GET", "/v1/partner/earnings?grain=month", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    assert_eq!(e["next_payout"], json!({ "date": "2026-03-25", "amount": 160_000, "count": 1 }));
    assert_eq!(e["pending_total"], 160_000);
    assert_eq!(e["paid_total"], 80_000);
    assert_eq!(e["series"].as_array().map(Vec::len), Some(12));

    let member = auth::issue_access(SECRET, Uuid::now_v7(), Role::Member).expect("token");
    let (s, _) = call(&app, "GET", "/v1/partner/earnings", Some(&member), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "GET", "/v1/partner/earnings", Some(&token), None).await;
    assert_eq!(s, StatusCode::OK);
}

async fn fetch_csv(app: &axum::Router, uri: &str, token: &str) -> String {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request");
    let resp = app.clone().oneshot(req).await.expect("router");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf8")
}

#[sqlx::test(migrations = "../../migrations")]
async fn settlements_export_uses_kst_month_bounds_and_totals_match_rows(pool: PgPool) {
    seed_catalog(&pool).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000002', 'kst') RETURNING id")
        .fetch_one(&pool)
        .await
        .expect("user");
    // A name starting with "=" must reach the sheet as a guarded text cell, not a formula.
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone, name, team_name, status) VALUES ('01070000202', '=1+1', 'team', 'verified') RETURNING id")
        .fetch_one(&pool)
        .await
        .expect("pro");
    // (number, created_at, gross, fee): the March KST edges are in; the instants just outside are out.
    let cases = [
        ("BKB-1", "2026-02-28T23:59:59+09", 1_000, 100),
        ("BKB-2", "2026-03-01T00:00:00+09", 10_000, 2_000),
        ("BKB-4", "2026-03-31T14:59:58Z", 30_000, 6_000),
        ("BKB-3", "2026-03-31T23:59:59+09", 20_000, 4_000),
        ("BKB-5", "2026-03-31T15:00:00Z", 1_000, 100),
        ("BKB-6", "2026-04-01T00:00:00+09", 1_000, 100),
    ];
    for (number, created, gross, fee) in cases {
        let booking = insert_booking(&pool, user, number, gross, 0, 3, created).await;
        insert_settlement(&pool, pro, booking, gross, fee, "pending", "2026-03-25", None, created).await;
    }
    let token = admin_token(&pool).await;
    let app = common::app(pool.clone());
    let csv = fetch_csv(&app, "/v1/admin/finance/export/settlements?period=custom&from=2026-03-01&to=2026-03-31", &token).await;
    let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines.len(), 5, "header, three March rows, total");
    assert!(lines[1].contains("BKB-2"), "earliest March row first");
    assert!(lines[3].contains("BKB-3"), "23:59:59 KST row is still March");
    assert!(lines[1].contains(",'=1+1,team,"), "formula text is guarded");
    for outside in ["BKB-1,", "BKB-5,", "BKB-6,"] {
        assert!(!csv.contains(outside), "{outside} is outside March KST");
    }
    assert_eq!(lines[4], "합계,,,,,,,60000,12000,48000,,,");
    let (gross, fee): (i64, i64) = sqlx::query_as(
        "SELECT SUM(st.gross)::bigint, SUM(st.fee)::bigint FROM settlements st
         WHERE (st.created_at AT TIME ZONE 'Asia/Seoul')::date BETWEEN '2026-03-01' AND '2026-03-31'",
    )
    .fetch_one(&pool)
    .await
    .expect("sums");
    assert_eq!((gross, fee), (60_000, 12_000), "database sums agree with the sheet total");
}

/// The KR pack with VAT at 10% until 2026-03-15 and 20% from then: one March holds both rates.
fn two_rate_app(pool: PgPool) -> axum::Router {
    common::app_with_pack(pool, |kr| {
        kr["tax"]["rates"] = json!([
            {"category": "service", "rate_bps": 1000, "from": "1977-07-01", "until": "2026-03-15"},
            {"category": "service", "rate_bps": 2000, "from": "2026-03-15"}
        ]);
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn vat_follows_the_rate_on_each_fee_day_and_totals_equal_their_parts(pool: PgPool) {
    seed_march(&pool).await;
    let app = two_rate_app(pool.clone());
    let token = admin_token(&pool).await;
    // Fees: 20,000 on 03-06 at 10% and 40,000 on 03-21 at 20%.
    let want = 2_000 + 8_000;
    let (status, s) = call(&app, "GET", PERIOD, Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "{s}");
    assert_eq!(s["vat_estimate"], want);
    assert_eq!(s["vat_basis"], "platform_fee_10_20pct_estimate");

    let (_, svc) = call(
        &app,
        "GET",
        "/v1/admin/finance/services?period=custom&from=2026-03-01&to=2026-03-31",
        Some(&token),
        None,
    )
    .await;
    let by_service: i64 = svc.as_array().expect("rows").iter().map(|r| r["vat_estimate"].as_i64().expect("vat")).sum();
    assert_eq!(by_service, want, "{svc}");

    let (_, series) = call(&app, "GET", "/v1/admin/finance/series?grain=month", Some(&token), None).await;
    let march = series
        .as_array()
        .expect("points")
        .iter()
        .find(|p| p["key"] == "2026-03-01")
        .expect("march point");
    assert_eq!(march["vat_estimate"], want, "the month's series point agrees with its summary");

    let monthly = fetch_csv(&app, "/v1/admin/finance/export/monthly?period=custom&from=2026-03-01&to=2026-03-31", &token).await;
    let lines: Vec<&str> = monthly.trim_start_matches('\u{feff}').lines().collect();
    assert!(lines[0].contains("부가세추정(수수료의10/20%)"), "{}", lines[0]);
    assert_eq!(lines[1], "2026-03,3,1,340000,50000,290000,60000,10000,80000,160000,0");
    assert_eq!(lines[2], "합계,3,1,340000,50000,290000,60000,10000,80000,160000,0");

    let vat = fetch_csv(&app, "/v1/admin/finance/export/vat?year=2026&quarter=1", &token).await;
    assert!(vat.contains("부가세 추정(수수료의 10/20%),0,10000"), "{vat}");
}
