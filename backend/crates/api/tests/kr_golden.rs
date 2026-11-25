// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Korea golden outputs over the real router and a migrated Postgres, frozen before the country-pack refactor
//! (PLAN section 8.1 step 1): the cancel day boundary in Asia/Seoul, the move-quote estimate route, every finance and
//! tax CSV export on a fixed KR fixture database, `/v1/config`, the seller-info fragments, the AlimTalk and SMS payloads
//! of all seven templates, consent transitions, the SEO page footer labels and `i18n::tr`. The Toss and PortOne request
//! bodies are frozen in `tests/toss_payments.rs` (goldens `toss_*_bodies`, `gateway_switch_bodies`).
//! Each test compares its JSON byte for byte with `tests/golden/kr/<name>.json`; a difference is a regression.
//! Regenerate only on purpose, with `UPDATE_GOLDENS=1`, and review the diff. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{
    auth::Role,
    i18n, privacy,
    providers::sms,
    renewals::kst_today,
    seo, sms_outbox,
    util::{KST_TODAY, today_kst, today_kst_at},
};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use common::{call, golden, seed_catalog};
use domain::{
    booking::{BookingStatus, cancel_quote},
    cancel_policy::CancelPolicy,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

/// One GET with an optional bearer token. Returns the status, the headers a client relies on, and the raw body.
async fn get_raw(app: &Router, uri: &str, token: Option<&str>) -> (StatusCode, Value, String) {
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let mut req = req.body(Body::empty()).expect("request");
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let resp = app.clone().oneshot(req).await.expect("router");
    let status = resp.status();
    let headers: serde_json::Map<String, Value> = [header::CONTENT_TYPE, header::CACHE_CONTROL, header::CONTENT_DISPOSITION]
        .iter()
        .filter_map(|h| resp.headers().get(h).map(|v| (h.as_str().to_owned(), json!(v.to_str().expect("ascii header")))))
        .collect();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
    (status, Value::Object(headers), String::from_utf8(bytes.to_vec()).expect("utf-8"))
}

/// A CSV response as status, headers and its `\n`-separated lines (joining them with `\n` restores the body exactly).
async fn csv(app: &Router, uri: &str, token: &str) -> Value {
    let (status, headers, body) = get_raw(app, uri, Some(token)).await;
    json!({ "uri": uri, "status": status.as_u16(), "headers": headers, "lines": body.split('\n').collect::<Vec<_>>() })
}

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).expect("instant").with_timezone(&Utc)
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_refunds_across_the_seoul_day_boundary(pool: PgPool) {
    // The SQL form of "today" evaluated at a fixed instant instead of now().
    let sql_today = format!("SELECT {}", KST_TODAY.replace("now()", "$1::timestamptz"));
    let policies = [
        ("default", CancelPolicy::default()),
        (
            "tiered_min_fee",
            serde_json::from_value(json!({"tiers": [{"min_days": 7, "percent": 0}, {"min_days": 3, "percent": 10}, {"min_days": 1, "percent": 30}, {"min_days": 0, "percent": 50}], "min_fee": 5000, "reschedule_min_days": 2}))
                .expect("policy"),
        ),
    ];
    let visit = NaiveDate::from_ymd_opt(2026, 3, 17).expect("visit");
    let mut rows = Vec::new();
    // 15:00 UTC is midnight in Seoul: one second either side lands on different KST days.
    for instant in [
        "2026-03-14T14:59:59Z",
        "2026-03-14T15:00:00Z",
        "2026-03-15T14:59:59Z",
        "2026-03-15T15:00:00Z",
        "2026-03-16T23:30:00+09:00",
        "2026-03-16T15:00:00Z",
    ] {
        let now = utc(instant);
        let rust_today = kst_today(now);
        let db_today: NaiveDate = sqlx::query_scalar(&sql_today).bind(now).fetch_one(&pool).await.expect("sql today");
        // The cancel-preview route counts days from util::today_kst(): the same date at the same instant.
        assert_eq!(today_kst_at(now), rust_today, "util::today_kst at {instant}");
        let days = (visit - rust_today).num_days();
        let quotes: Vec<Value> = policies
            .iter()
            .map(|(name, p)| json!({ "policy": name, "quote": cancel_quote(p, BookingStatus::Confirmed, days, 52_000), "reschedule_allowed": p.reschedule_allowed(days) }))
            .collect();
        rows.push(json!({ "now": instant, "kst_today": rust_today, "sql_kst_today": db_today, "visit_date": visit, "days_before": days, "quotes": quotes }));
    }
    // The wall-clock helper the route calls agrees with the pinned forms right now (retried once across Seoul midnight).
    let wall = || async {
        let db: NaiveDate = sqlx::query_scalar(&format!("SELECT {KST_TODAY}")).fetch_one(&pool).await.expect("sql today");
        (today_kst(), today_kst_at(Utc::now()), kst_today(Utc::now()), db)
    };
    let (a, b, c, d) = wall().await;
    let (a, b, c, d) = if a == b && b == c && c == d { (a, b, c, d) } else { wall().await };
    assert!(a == b && b == c && c == d, "today_kst {a}, today_kst_at {b}, kst_today {c}, SQL {d}");

    // The customer route over bookings dated relative to today's KST date, before and after the owner changes the policy.
    // This checks the policy mapping on the route; the boundary itself is pinned above through today_kst_at.
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let (token, subject) = common::login(&app, "01070001001", "customer").await;
    let user: Uuid = subject.parse().expect("user id");
    let admin = common::staff_token(&pool, Role::StaffAdmin).await.0;
    let previews = match route_previews(&pool, &app, (&token, user, &admin), &policies, "a").await {
        Some(p) => p,
        None => route_previews(&pool, &app, (&token, user, &admin), &policies, "b")
            .await
            .expect("crossed Seoul midnight twice"),
    };
    golden("cancel_day_boundary", &json!({ "boundary": rows, "route": previews }));
}

/// Cancel previews for bookings 0, 1, 2, 3, 7 and 30 days out, under each policy in turn. The route reads the wall
/// clock, so `None` when Seoul's date changed during the run (the caller redoes it once on the new day).
async fn route_previews(
    pool: &PgPool,
    app: &Router,
    (token, user, admin): (&str, Uuid, &str),
    policies: &[(&str, CancelPolicy)],
    tag: &str,
) -> Option<Vec<Value>> {
    let day = today_kst();
    sqlx::query("DELETE FROM settings WHERE key = 'cancel_policy'")
        .execute(pool)
        .await
        .expect("reset policy");
    let mut bookings = Vec::new();
    for days in [0i64, 1, 2, 3, 7, 30] {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, paid_amount)
             VALUES ($1, $2, 'bathroom', '{}'::jsonb, '11680', $3, 10, 1, 52000, 52000, 0, 52000) RETURNING id",
        )
        .bind(format!("BKG-{tag}-{days}"))
        .bind(user)
        .bind(day + Duration::days(days))
        .fetch_one(pool)
        .await
        .expect("booking");
        bookings.push((days, id));
    }
    let mut previews = Vec::new();
    for (name, policy) in policies {
        if *name != "default" {
            let (st, b) = call(app, "PUT", "/v1/admin/cancel-policy", Some(admin), Some(json!(policy))).await;
            assert_eq!(st, StatusCode::OK, "{b}");
        }
        let (_, public) = call(app, "GET", "/v1/policy/cancellation", None, None).await;
        let mut by_day = Vec::new();
        for (days, id) in &bookings {
            let (st, preview) = call(app, "GET", &format!("/v1/bookings/{id}/cancel-preview"), Some(token), None).await;
            assert_eq!(st, StatusCode::OK, "{preview}");
            by_day.push(json!({ "visit_in_days": days, "preview": preview }));
        }
        previews.push(json!({ "policy": name, "public": public, "previews": by_day }));
    }
    (today_kst() == day).then_some(previews)
}

#[sqlx::test(migrations = "../../migrations")]
async fn move_quote_estimate_route_on_the_seeded_formula(pool: PgPool) {
    seed_catalog(&pool).await;
    let app = common::app(pool.clone());
    let base = json!({"cleaning_type": "standard", "luggage": "empty", "building": "apartment", "layout": "two_rooms",
                      "duplex": false, "bathrooms": 2, "balconies": 1, "elevator": true, "floor_3_or_higher": true});
    let patches = [
        json!({}),
        json!({"cleaning_type": "after_interior", "layout": "four_plus", "duplex": true}),
        json!({"cleaning_type": "same_day_move", "luggage": "partly_unpacked", "elevator": false}),
        json!({"layout": "studio", "bathrooms": 1, "balconies": 0, "ac_addons": [{"unit": "wall", "count": 1}]}),
        json!({"bathrooms": 9}),
    ];
    let mut out = Vec::new();
    for service in ["move_out", "move_in", "whole_home", "bathroom"] {
        for patch in &patches {
            let mut body = base.clone();
            for (k, v) in patch.as_object().expect("patch") {
                body[k] = v.clone();
            }
            body["service_code"] = json!(service);
            let (st, mut estimate) = call(&app, "POST", "/v1/quote/estimate", None, Some(body.clone())).await;
            // The request id differs on every call; the error code is the contract.
            if let Some(e) = estimate.get_mut("error").and_then(Value::as_object_mut) {
                e.remove("request_id");
            }
            out.push(json!({ "request": body, "status": st.as_u16(), "response": estimate }));
        }
    }
    golden("quote_estimate_route", &json!(out));
}

const PRO_A: Uuid = Uuid::from_u128(0x6b00_0000_0000_0000_0000_0000_0000_00a1);
const PRO_B: Uuid = Uuid::from_u128(0x6b00_0000_0000_0000_0000_0000_0000_00b2);

fn fixed(kind: u128, n: u128) -> Uuid {
    Uuid::from_u128((0x6b00 << 112) | (kind << 64) | n)
}

/// KR fixture: March 2026 bookings, payments (one refunded, one partly), settlements in every state, Toss and manual
/// payouts for a freelancer, an unclassified and a business partner, checkouts by method, and 세금계산서 requests.
/// Every id is fixed and every time is KST, some of them a second either side of a day or month boundary.
#[allow(clippy::type_complexity)] // the fixture rows read as tables
async fn seed_kr_finance(db: &PgPool) {
    seed_catalog(db).await;
    let user = fixed(1, 1);
    sqlx::query("INSERT INTO users (id, phone, name) VALUES ($1, '01070002001', '김고객')")
        .bind(user)
        .execute(db)
        .await
        .expect("user");
    for (id, phone, name, team, tax_type, employment) in [
        (PRO_A, "01070002101", "김파트너", "깨끗팀", "freelancer", Some("freelance")),
        (PRO_B, "01070002102", "박사업", "반짝(주)", "business", Some("contract")),
        (fixed(2, 3), "01070002103", "이미등록", "새팀", "unset", None),
    ] {
        sqlx::query("INSERT INTO pros (id, phone, name, team_name, status, tax_type) VALUES ($1, $2, $3, $4, 'verified', $5)")
            .bind(id)
            .bind(phone)
            .bind(name)
            .bind(team)
            .bind(tax_type)
            .execute(db)
            .await
            .expect("pro");
        sqlx::query("INSERT INTO pro_profiles (pro_id, employment_type) VALUES ($1, $2)")
            .bind(id)
            .bind(employment)
            .execute(db)
            .await
            .expect("profile");
    }
    // (n, service, final, discount, status, created KST, payment amount, refunded, payment status, method)
    let bookings: [(u128, &str, i64, i64, i16, &str, i64, i64, &str, &str); 7] = [
        (1, "studio", 99_000, 0, 3, "2026-03-01T00:00:00+09", 99_000, 0, "paid", "card"),
        (2, "bathroom", 47_000, 5_000, 3, "2026-03-05T10:00:00+09", 47_000, 0, "paid", "kakaopay"),
        (3, "aircon", 375_300, 41_700, 3, "2026-03-10T23:59:59+09", 375_300, 0, "paid", "card"),
        (4, "bathroom", 52_000, 0, -1, "2026-03-12T09:00:00+09", 52_000, 52_000, "refunded", "card"),
        (
            5,
            "mattress",
            100_300,
            17_700,
            3,
            "2026-03-20T14:00:00+09",
            100_300,
            30_000,
            "partially_refunded",
            "virtual_account",
        ),
        (6, "studio", 99_000, 0, 3, "2026-03-31T23:59:59+09", 99_000, 0, "paid", "card"),
        (7, "studio", 99_000, 0, 3, "2026-04-01T00:00:00+09", 99_000, 0, "paid", "card"),
    ];
    for (n, service, final_price, discount, status, created, amount, refunded, pay_status, method) in bookings {
        let booking = fixed(3, n);
        sqlx::query(
            "INSERT INTO bookings (id, number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, paid_amount, cancelled_at, created_at)
             VALUES ($1, $2, $3, $4, '{}'::jsonb, '11680', ($5::timestamptz AT TIME ZONE 'Asia/Seoul')::date + 3, 10, $6, $7, $7 + $8, $8, $9,
                     CASE WHEN $6 = -1 THEN $5::timestamptz + interval '1 day' END, $5::timestamptz)",
        )
        .bind(booking)
        .bind(format!("BK2603-{n:05}"))
        .bind(user)
        .bind(service)
        .bind(created)
        .bind(status)
        .bind(final_price)
        .bind(discount)
        .bind(amount)
        .execute(db)
        .await
        .expect("booking");
        sqlx::query(
            "INSERT INTO payments (id, booking_id, amount, kind, status, refunded_amount, provider, provider_tx_id, idempotency_key, created_at)
             VALUES ($1, $2, $3, 'full', $4, $5, 'portone', $6, $7, $8::timestamptz)",
        )
        .bind(fixed(4, n))
        .bind(booking)
        .bind(amount)
        .bind(pay_status)
        .bind(refunded)
        .bind(format!("pg-tx-{n}"))
        .bind(format!("kr-golden-{n}"))
        .bind(created)
        .execute(db)
        .await
        .expect("payment");
        sqlx::query(
            "INSERT INTO checkouts (id, user_id, kind, booking_id, method, amount, status, expires_at, created_at)
             VALUES ($1, $2, 'booking', $3, $4, $5, 'paid', $6::timestamptz + interval '1 day', $6::timestamptz)",
        )
        .bind(fixed(5, n))
        .bind(user)
        .bind(booking)
        .bind(method)
        .bind(amount)
        .bind(created)
        .execute(db)
        .await
        .expect("checkout");
    }
    // A failed checkout never counts.
    sqlx::query(
        "INSERT INTO checkouts (id, user_id, kind, method, amount, status, expires_at, created_at)
         VALUES ($1, $2, 'booking', 'card', 77000, 'failed', '2026-03-16T00:00:00+09', '2026-03-15T10:00:00+09')",
    )
    .bind(fixed(5, 99))
    .bind(user)
    .execute(db)
    .await
    .expect("failed checkout");

    // Payouts: (n, pro, amount, items, provider, state, tax_type, paid_at KST)
    let payouts: [(u128, Uuid, i64, i32, &str, &str, &str, Option<&str>); 5] = [
        (1, PRO_A, 79_200, 1, "toss", "paid", "freelancer", Some("2026-03-04T12:00:00+09")),
        (2, PRO_A, 337_840, 2, "manual", "paid", "freelancer", Some("2026-03-11T00:00:00+09")),
        (3, PRO_B, 80_240, 1, "manual", "paid", "business", Some("2026-03-18T15:30:00+09")),
        (4, fixed(2, 3), 33_333, 1, "manual", "paid", "unset", Some("2026-03-31T23:59:59+09")),
        (5, PRO_A, 79_200, 1, "toss", "sent", "freelancer", None),
    ];
    for (n, pro, amount, items, provider, state, tax_type, paid_at) in payouts {
        sqlx::query(
            "INSERT INTO payouts (id, pro_id, payout_date, amount, items, provider, state, bank, holder, account_enc, account_last4, tax_type, paid_at, created_at)
             VALUES ($1, $2, '2026-03-04'::date + ($3::int - 1) * 7, $4, $5, $6, $7, '088', '예금주', 'x', '1234', $8, $9::timestamptz, '2026-03-02T09:00:00+09')",
        )
        .bind(fixed(6, n))
        .bind(pro)
        .bind(i32::try_from(n).expect("small"))
        .bind(amount)
        .bind(items)
        .bind(provider)
        .bind(state)
        .bind(tax_type)
        .bind(paid_at)
        .execute(db)
        .await
        .expect("payout");
    }
    // Settlements: (booking n, pro, gross, fee, payout date, state, paid_at, created KST, payout n)
    let settlements: [(u128, Uuid, i64, i64, &str, &str, Option<&str>, &str, Option<u128>); 5] = [
        (
            1,
            PRO_A,
            99_000,
            19_800,
            "2026-03-04",
            "paid",
            Some("2026-03-04T12:00:00+09"),
            "2026-03-02T09:00:00+09",
            Some(1),
        ),
        (
            2,
            PRO_A,
            47_000,
            9_400,
            "2026-03-11",
            "paid",
            Some("2026-03-11T00:00:00+09"),
            "2026-03-08T18:00:00+09",
            Some(2),
        ),
        (3, PRO_A, 375_300, 75_060, "2026-03-18", "pending", None, "2026-03-13T23:59:59+09", None),
        (5, PRO_B, 100_300, 20_060, "2026-03-25", "held", None, "2026-03-23T10:00:00+09", None),
        (6, PRO_B, 99_000, 19_800, "2026-04-08", "waiting_balance", None, "2026-04-01T00:00:00+09", None),
    ];
    for (n, pro, gross, fee, payout_date, state, paid_at, created, payout) in settlements {
        sqlx::query(
            "INSERT INTO settlements (id, pro_id, booking_id, gross, fee, net, payout_date, state, paid_at, created_at, payout_id)
             VALUES ($1, $2, $3, $4, $5, $4 - $5, $6::date, $7, $8::timestamptz, $9::timestamptz, $10)",
        )
        .bind(fixed(7, n))
        .bind(pro)
        .bind(fixed(3, n))
        .bind(gross)
        .bind(fee)
        .bind(payout_date)
        .bind(state)
        .bind(paid_at)
        .bind(created)
        .bind(payout.map(|p| fixed(6, p)))
        .execute(db)
        .await
        .expect("settlement");
    }
    // 세금계산서 requests on bookings 1, 3 and 6 (6 is the last second of March); one cancelled never exports.
    for (n, booking, company, rep, state, created) in [
        (1u128, 1u128, "에이스(주)", "홍길동", "requested", "2026-03-01T00:00:00+09"),
        (2, 3, "=HYPERLINK(\"x\")", "", "requested", "2026-03-10T23:59:59+09"),
        (3, 6, "주식회사 \"반짝\", 본점", "김대표", "requested", "2026-03-31T23:59:59+09"),
        (4, 2, "취소된회사", "", "cancelled", "2026-03-05T10:00:00+09"),
    ] {
        sqlx::query(
            "INSERT INTO tax_invoice_requests (id, checkout_id, payment_id, business_number, company_name, representative, business_type, business_item, address, email, state, created_at)
             VALUES ($1, $2, $3, '2208162517', $4, $5, '서비스', '청소', '서울특별시 강남구 테헤란로 1', 'tax@example.test', $6, $7::timestamptz)",
        )
        .bind(fixed(8, n))
        .bind(fixed(5, booking))
        .bind(fixed(4, booking))
        .bind(company)
        .bind(rep)
        .bind(state)
        .bind(created)
        .execute(db)
        .await
        .expect("tax invoice request");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn settlement_and_tax_csv_exports_on_the_kr_fixture(pool: PgPool) {
    seed_kr_finance(&pool).await;
    let app = common::app(pool.clone());
    let token = common::staff_token(&pool, Role::StaffAdmin).await.0;
    let march = "period=custom&from=2026-03-01&to=2026-03-31";
    let mut exports = Vec::new();
    for kind in ["ledger", "settlements", "monthly", "withholding", "payroll"] {
        exports.push(csv(&app, &format!("/v1/admin/finance/export/{kind}?{march}"), &token).await);
    }
    exports.push(csv(&app, "/v1/admin/finance/export/monthly?period=custom&from=2026-02-01&to=2026-04-30", &token).await);
    exports.push(csv(&app, "/v1/admin/finance/export/vat?year=2026&quarter=1", &token).await);
    exports.push(csv(&app, "/v1/admin/finance/export/vat?year=2026&quarter=2", &token).await);
    exports.push(csv(&app, &format!("/v1/admin/finance/export/tax-invoices?{march}"), &token).await);
    // A second export hands out nothing new.
    exports.push(csv(&app, &format!("/v1/admin/finance/export/tax-invoices?{march}"), &token).await);
    let mut summaries = Vec::new();
    for view in ["summary", "partners", "services"] {
        let (st, body) = call(&app, "GET", &format!("/v1/admin/finance/{view}?{march}"), Some(&token), None).await;
        summaries.push(json!({ "view": view, "status": st.as_u16(), "body": body }));
    }
    golden("finance_exports", &json!({ "exports": exports, "summaries": summaries }));
}

/// The business profile a Korean owner types in the console.
fn kr_business() -> Value {
    json!({
        "name": "다해클린",
        "name_en": "Dahae Clean",
        "short_name": "다해",
        "tagline": "집 청소, 예약부터 결제까지",
        "tagline_en": "Home cleaning, booked and paid",
        "primary_color": "#3b5bfd",
        "support_phone": "1588-0000",
        "support_email": "help@example.co.kr",
        "company": {
            "legal_name": "주식회사 다해클린",
            "ceo": "홍길동",
            "registration_no": "220-81-62517",
            "mail_order_no": "제2026-서울강남-0001호",
            "address": "서울특별시 강남구 테헤란로 1, 10층",
            "privacy_officer": "김보호 (privacy@example.co.kr)",
            "hosting_provider": "예시호스팅(주)",
            "privacy_email": "privacy@example.co.kr"
        }
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn public_config_document(pool: PgPool) {
    let app = common::app(pool.clone());
    let (st, headers, fresh) = get_raw(&app, "/v1/config", None).await;
    assert_eq!(st, StatusCode::OK, "{fresh}");
    let token = common::staff_token(&pool, Role::StaffAdmin).await.0;
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(kr_business())).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = call(
        &app,
        "PUT",
        "/v1/admin/config",
        Some(&token),
        Some(json!({"flags": {"chat": false}, "maintenance": {"on": false, "message": ""}})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, _, configured) = get_raw(&app, "/v1/config", None).await;
    let parse = |s: &str| serde_json::from_str::<Value>(s).expect("config JSON");
    golden(
        "public_config",
        &json!({ "headers": headers, "fresh_install": parse(&fresh), "kr_business": parse(&configured) }),
    );
}

/// Every `?field=` the website includes, plus the footer block (no field). Built from the endpoint's own list, so a new
/// field gets a golden entry.
fn seller_fields() -> Vec<&'static str> {
    let fields: Vec<&str> = std::iter::once("").chain(api::routes::seller_info_fields()).collect();
    assert_eq!(fields.len(), 38, "a seller-info field was added or removed: review the seller_info golden");
    fields
}

async fn seller_fragments(app: &Router) -> Vec<Value> {
    let mut out = Vec::new();
    for field in seller_fields() {
        let uri = if field.is_empty() {
            "/v1/public/seller-info".to_owned()
        } else {
            format!("/v1/public/seller-info?field={field}")
        };
        let (st, headers, html) = get_raw(app, &uri, None).await;
        out.push(json!({ "field": field, "status": st.as_u16(), "headers": headers, "html": html }));
    }
    out
}

#[sqlx::test(migrations = "../../migrations")]
async fn seller_info_fragments(pool: PgPool) {
    let app = common::app(pool.clone());
    let fresh = seller_fragments(&app).await;
    let token = common::staff_token(&pool, Role::StaffAdmin).await.0;
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(kr_business())).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let configured = seller_fragments(&app).await;
    golden("seller_info", &json!({ "fresh_install": fresh, "kr_business": configured }));
}

/// What each of the seven AlimTalk templates sends: the alert kind that picks it, the outbox row with and without an
/// approved template, the Solapi AlimTalk body, its SMS fallback, and the SMS a non-Korean customer gets instead.
#[test]
fn alimtalk_and_sms_payloads_for_every_template() {
    let booking = i18n::SAMPLE_BOOKING_NO;
    let event = |kind: &str| i18n::event(kind).map(|e| (e.title, e.body)).expect("event text");
    // (template, alert kind, alert data, Korean title, Korean body), each as its call site sends it.
    let cases = [
        (sms::TPL_CONFIRMED, "booking.status", json!({ "status": 1 }), "예약 상태가 바뀌었어요", booking),
        (sms::TPL_ASSIGNED, "booking.status", json!({ "status": 2 }), "청소팀이 배정됐어요", booking),
        (sms::TPL_REMINDER, "booking.reminder", json!({}), "내일 청소 예정이에요", booking),
        (sms::TPL_ARRIVAL, "booking.arrival", json!({}), event("booking.arrival").0, booking),
        (
            sms::TPL_COMPLETED,
            "booking.completed",
            json!({}),
            event("booking.completed").0,
            event("booking.completed").1,
        ),
        (
            sms::TPL_RECEIPT,
            "payment.receipt",
            json!({}),
            event("payment.receipt").0,
            event("payment.receipt").1,
        ),
        (sms::TPL_CANCELLED, "booking.cancelled", json!({}), event("booking.cancelled").0, booking),
    ];
    let keys: Vec<&str> = cases.iter().map(|c| c.0).collect();
    assert_eq!(keys, sms::TEMPLATE_KEYS, "one case per template key, in order");
    let (to, from, brand) = ("01070003001", "15880000", "다해클린");
    let out: Vec<Value> = cases
        .iter()
        .map(|&(template, kind, ref data, title, body)| {
            let row = |ready| {
                let msg = sms_outbox::Message::Alimtalk { template, brand, title, body };
                let (channel, title, body, data) = sms_outbox::stored(msg, ready);
                json!({ "channel": channel, "title": title, "body": body, "data": data })
            };
            let text = format!("{title}\n{body}");
            let english = format!("{}\n{}", i18n::tr("en", title), i18n::tr("en", body));
            json!({
                "template": template,
                "alert_kind": kind,
                "alert_data": data,
                "alert_key": sms::alert_key(kind, data),
                "outbox_with_template": row(true),
                "outbox_without_template": row(false),
                "alimtalk": sms::alimtalk_message("PF_KR", "TPL_KR", to, from, &text, sms::alimtalk_variables(brand, title, body)),
                "sms_fallback": sms::sms_message(to, from, &text),
                "sms_english": sms::sms_message(to, from, &english),
            })
        })
        .collect();
    // The SMS/LMS cut-over, in characters.
    let lengths: Vec<Value> = [44, 45, 46, 90, 91]
        .iter()
        .map(|&n| json!({ "chars": n, "type": sms::sms_message(to, from, &"가".repeat(n))["type"] }))
        .collect();
    golden("messages", &json!({ "templates": out, "sms_length": lengths }));
}

/// Every server text in Korean and English, the status-code keys, and text that passes through untranslated.
#[test]
fn server_text_translations() {
    let sources: Vec<&str> = i18n::TABLE.iter().map(|(ko, _)| *ko).chain(["B-20261111-0042", "52,000원", ""]).collect();
    let out: Vec<Value> = sources
        .iter()
        .map(|s| json!({ "source": s, "ko": i18n::tr("ko", s), "en": i18n::tr("en", s), "ja": i18n::tr("ja", s) }))
        .collect();
    let events: Vec<Value> = i18n::EVENTS
        .iter()
        .map(|e| json!({ "kind": e.kind, "label": e.label, "vars": e.vars, "ko": i18n::default_text(e, "ko"), "en": i18n::default_text(e, "en") }))
        .collect();
    let shipped: Vec<&str> = ["ko", "en", "ja", "zh"].iter().map(|l| i18n::shipped(l)).collect();
    golden("i18n", &json!({ "shipped": shipped, "tr": out, "events": events }));
}

/// The consent state as the app reads it, with `granted_at` reduced to whether it is set.
async fn consent_snapshot(app: &Router, pool: &PgPool, token: &str, user: Uuid) -> Value {
    let (st, mut state) = call(app, "GET", "/v1/me/consents", Some(token), None).await;
    assert_eq!(st, StatusCode::OK, "{state}");
    for item in state["items"].as_array_mut().expect("items") {
        let set = !item["granted_at"].is_null();
        item["granted_at"] = json!(set);
    }
    json!({
        "state": state,
        "marketing_allowed": privacy::marketing_allowed(pool, user).await.expect("marketing"),
        "overseas_allowed": privacy::overseas_allowed(pool, user).await.expect("overseas"),
    })
}

/// The customer's consent state after each change, from sign-up through a policy-version bump.
#[sqlx::test(migrations = "../../migrations")]
async fn consent_transitions(pool: PgPool) {
    let app = common::app(pool.clone());
    let c = common::customer(&app, &pool, "010-7000-4001").await;
    let v = privacy::CONSENT_VERSION;
    let grant = |kind: &str, granted: bool| json!({ "kind": kind, "granted": granted, "version": v });
    let mut steps = vec![json!({ "step": "signed_up", "after": consent_snapshot(&app, &pool, &c.token, c.user).await })];
    let puts = [
        ("withdraw_terms", json!([grant("terms", false)])),
        ("stale_version", json!([{ "kind": "terms", "granted": true, "version": "2020-01-01" }])),
        ("unknown_kind", json!([grant("newsletter", true)])),
        ("grant_marketing", json!([grant("marketing", true)])),
        ("withdraw_marketing", json!([grant("marketing", false)])),
        ("grant_location", json!([grant("location", true)])),
        ("withdraw_overseas", json!([grant("overseas_transfer", false)])),
        ("regrant_overseas", json!([grant("overseas_transfer", true)])),
        ("policy_bumped", Value::Null),
        ("regranted", common::required_consents()),
    ];
    for (step, consents) in puts {
        if consents.is_null() {
            // A policy-version bump: the newest grants name the old text, so the app asks again.
            sqlx::query(
                "INSERT INTO user_consents (user_id, kind, version, granted)
                 SELECT $1, kind, '2000-01-01', true FROM unnest(ARRAY['terms', 'privacy', 'overseas_transfer']) AS kind",
            )
            .bind(c.user)
            .execute(&pool)
            .await
            .expect("old-version grants");
            steps.push(json!({ "step": step, "after": consent_snapshot(&app, &pool, &c.token, c.user).await }));
            continue;
        }
        let body = json!({ "consents": consents });
        let (st, resp) = call(&app, "PUT", "/v1/me/consents", Some(c.token.as_str()), Some(body.clone())).await;
        let error = resp.get("error").map(|e| e["code"].clone());
        steps.push(json!({ "step": step, "put": body, "status": st.as_u16(), "error": error, "after": consent_snapshot(&app, &pool, &c.token, c.user).await }));
    }
    golden("consents", &json!({ "version": v, "kinds": privacy::CONSENTS, "steps": steps }));
}

/// The public service page a search engine indexes, with the KR business's footer labels.
#[test]
fn seo_service_page_and_footer_labels() {
    let base = "https://dahae.example.co.kr";
    let brand = seo::Brand::from_value(base, &kr_business()).expect("brand");
    let company = brand.company.clone();
    let page = seo::ServicePage {
        base: base.to_owned(),
        brand,
        service: "move_out".to_owned(),
        keyword: "이사청소",
        english: "Move-out cleaning",
        district_code: "11680".to_owned(),
        district: "강남구",
        district_en: "Gangnam-gu",
        price: Some((150_000, 380_000)),
        faqs: vec![("주말에도 하나요?".to_owned(), "네, 토요일과 일요일에도 해요.".to_owned())],
        review_count: 1,
        review_avg: 5.0,
        reviews: vec![seo::Review {
            rating: 5,
            body: "꼼꼼했어요".to_owned(),
            name: "김*객".to_owned(),
        }],
        verify: vec![("naver-site-verification", "abc123".to_owned())],
    };
    let html = seo::render_service_page(&page);
    let footer = &html[html.find("<footer>").expect("footer")..];
    let krw: Vec<String> = [0, 1000, 52_000, 1_234_567].iter().map(|&n| seo::krw(n)).collect();
    golden(
        "seo_page",
        &json!({ "company": company, "footer": footer.lines().collect::<Vec<_>>(), "page": html.lines().collect::<Vec<_>>(), "krw": krw }),
    );
}
