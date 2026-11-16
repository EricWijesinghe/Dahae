//! Tax-invoice CSV export is idempotent: a second export returns no rows, and a confirmed re-export sends the same
//! rows again without clearing exported_at. Needs DATABASE_URL (sqlx::test).

mod common;

use api::auth::{self, Role};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use common::{SECRET, seed_catalog};
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

const EXPORT: &str = "/v1/admin/finance/export/tax-invoices?period=custom&from=2026-03-01&to=2026-03-31";
const RE_EXPORT: &str = "/v1/admin/finance/export/tax-invoices/re-export?period=custom&from=2026-03-01&to=2026-03-31";

/// The admin that exports in these tests.
const EXPORTER: Uuid = Uuid::from_u128(0x7a11_0000_0000_0000_0000_0000_0000_0001);

/// Sends one request and returns the status and the raw body (the export is CSV, not JSON).
async fn send(app: &Router, method: &str, uri: &str) -> (StatusCode, String) {
    let token = auth::issue_access(SECRET, EXPORTER, Role::StaffAdmin).expect("token");
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let resp = app.clone().oneshot(request).await.expect("router is infallible");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("response body");
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Two queued (state 'requested') invoices in March, each with a paid payment.
async fn seed_requests(db: &PgPool) {
    seed_catalog(db).await;
    // The exporting admin is a live staff row with a fresh step-up (exports check both).
    sqlx::query("INSERT INTO staff (id, email, name, password_hash, role, stepup_at) VALUES ($1, 'exporter@test.local', 'tester', 'x', 'admin', now()) ON CONFLICT DO NOTHING")
        .bind(EXPORTER)
        .execute(db)
        .await
        .expect("exporter");
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01070000201', 'tax') RETURNING id")
        .fetch_one(db)
        .await
        .expect("user");
    for (i, company) in ["ACME-ONE", "ACME-TWO"].iter().enumerate() {
        let booking: Uuid = sqlx::query_scalar(
            "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, created_at)
             VALUES ($1, $2, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, 3, 110000, 110000, 0, '2026-03-05T10:00:00+09'::timestamptz)
             RETURNING id",
        )
        .bind(format!("BKT-{i}"))
        .bind(user)
        .fetch_one(db)
        .await
        .expect("booking");
        let payment: Uuid = sqlx::query_scalar(
            "INSERT INTO payments (booking_id, amount, kind, status, provider, idempotency_key, created_at)
             VALUES ($1, 110000, 'full', 'paid', 'fake', $2, '2026-03-05T10:00:00+09'::timestamptz) RETURNING id",
        )
        .bind(booking)
        .bind(format!("tax-{i}"))
        .fetch_one(db)
        .await
        .expect("payment");
        let checkout: Uuid = sqlx::query_scalar(
            "INSERT INTO checkouts (user_id, kind, booking_id, method, amount, status, expires_at, created_at)
             VALUES ($1, 'booking', $2, 'card', 110000, 'paid', now() + interval '1 day', '2026-03-05T10:00:00+09'::timestamptz)
             RETURNING id",
        )
        .bind(user)
        .bind(booking)
        .fetch_one(db)
        .await
        .expect("checkout");
        sqlx::query(
            "INSERT INTO tax_invoice_requests (checkout_id, payment_id, business_number, company_name, email, state, created_at)
             VALUES ($1, $2, '2208162517', $3, 'tax@example.test', 'requested', '2026-03-05T10:00:00+09'::timestamptz)",
        )
        .bind(checkout)
        .bind(payment)
        .bind(*company)
        .execute(db)
        .await
        .expect("tax invoice request");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn second_export_returns_no_rows(pool: PgPool) {
    seed_requests(&pool).await;
    let app = common::app(pool.clone());

    let (status, first) = send(&app, "GET", EXPORT).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert!(first.contains("ACME-ONE") && first.contains("ACME-TWO"), "first export: {first}");

    let (status, second) = send(&app, "GET", EXPORT).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert!(!second.contains("ACME-"), "second export must be empty: {second}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn re_export_requires_confirm_and_returns_the_same_rows(pool: PgPool) {
    seed_requests(&pool).await;
    let app = common::app(pool.clone());
    let (status, _) = send(&app, "GET", EXPORT).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send(&app, "POST", RE_EXPORT).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "without confirm: {body}");

    let (status, body) = send(&app, "POST", &format!("{RE_EXPORT}&confirm=true")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("ACME-ONE") && body.contains("ACME-TWO"), "re-export: {body}");

    let exported: i64 = sqlx::query_scalar("SELECT count(*) FROM tax_invoice_requests WHERE exported_at IS NOT NULL")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(exported, 2, "re-export must not clear or change exported_at");
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'tax_invoice.re_export'")
        .fetch_one(&pool)
        .await
        .expect("audit count");
    assert_eq!(audited, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn re_export_skips_rows_never_exported(pool: PgPool) {
    seed_requests(&pool).await;
    let app = common::app(pool.clone());

    let (status, body) = send(&app, "POST", &format!("{RE_EXPORT}&confirm=true")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("ACME-"), "re-export before any export must hand out nothing: {body}");

    let exported: i64 = sqlx::query_scalar("SELECT count(*) FROM tax_invoice_requests WHERE exported_at IS NOT NULL")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(exported, 0, "re-export must not mark rows exported");
}

/// A paid booking with its checkout and one PortOne payment; returns (booking, payment, checkout).
async fn paid_booking(db: &PgPool, phone: &str) -> (Uuid, Uuid, Uuid) {
    seed_catalog(db).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ($1, 'refund') RETURNING id")
        .bind(phone)
        .fetch_one(db)
        .await
        .expect("user");
    let booking: Uuid = sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, created_at)
         VALUES ($2, $1, 'studio', '{}'::jsonb, '11680', '2026-03-15', 10, 1, 110000, 110000, 0, now()) RETURNING id",
    )
    .bind(user)
    .bind(format!("BKR-{phone}"))
    .fetch_one(db)
    .await
    .expect("booking");
    let checkout: Uuid = sqlx::query_scalar(
        "INSERT INTO checkouts (user_id, kind, booking_id, method, amount, status, expires_at)
         VALUES ($1, 'booking', $2, 'card', 110000, 'paid', now() + interval '1 day') RETURNING id",
    )
    .bind(user)
    .bind(booking)
    .fetch_one(db)
    .await
    .expect("checkout");
    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (booking_id, amount, kind, status, refunded_amount, provider, provider_tx_id, idempotency_key)
         VALUES ($1, 110000, 'full', 'paid', 0, 'portone', $2, $2) RETURNING id",
    )
    .bind(booking)
    .bind(format!("tx-{phone}"))
    .fetch_one(db)
    .await
    .expect("payment");
    (booking, payment, checkout)
}

/// A queued request for the payment; `exported` marks it as already handed out by the 홈택스 export.
async fn queued_request(db: &PgPool, checkout: Uuid, payment: Uuid, exported: bool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tax_invoice_requests (checkout_id, payment_id, business_number, company_name, email, state, exported_at)
         VALUES ($1, $2, '2208162517', 'REFUND-CO', 'tax@example.test', 'requested', CASE WHEN $3 THEN now() END) RETURNING id",
    )
    .bind(checkout)
    .bind(payment)
    .bind(exported)
    .fetch_one(db)
    .await
    .expect("request")
}

async fn request_state(db: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM tax_invoice_requests WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
        .expect("state")
}

/// Staff cancels the whole booking, which refunds its payment in full.
async fn full_refund(db: &PgPool, booking: Uuid) {
    let dev = common::app(db.clone());
    let (admin, _) = common::staff_token(db, Role::StaffAdmin).await;
    let (st, body) = common::call(
        &dev,
        "POST",
        &format!("/v1/admin/bookings/{booking}/cancel"),
        Some(admin.as_str()),
        Some(json!({})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn full_refund_cancels_the_queued_tax_invoice_request(pool: PgPool) {
    let (booking, payment, checkout) = paid_booking(&pool, "01070000301").await;
    let request = queued_request(&pool, checkout, payment, false).await;
    full_refund(&pool, booking).await;
    assert_eq!(request_state(&pool, request).await, "cancelled");
}

#[sqlx::test(migrations = "../../migrations")]
async fn exported_request_is_not_cancelled_and_raises_an_ops_note(pool: PgPool) {
    let (booking, payment, checkout) = paid_booking(&pool, "01070000302").await;
    let request = queued_request(&pool, checkout, payment, true).await;
    full_refund(&pool, booking).await;
    assert_eq!(request_state(&pool, request).await, "requested", "the owner already has this one");
    let notes: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'tax_invoice.refund_needs_cancel' AND data->>'payment' = $1")
        .bind(payment.to_string())
        .fetch_one(&pool)
        .await
        .expect("notes");
    assert_eq!(notes, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn issued_request_stays_issued_and_raises_an_ops_note_on_full_refund(pool: PgPool) {
    let (booking, payment, checkout) = paid_booking(&pool, "01070000303").await;
    let request = queued_request(&pool, checkout, payment, false).await;
    sqlx::query("UPDATE tax_invoice_requests SET state = 'issued', approval_number = 'APR-1', issued_on = now()::date WHERE id = $1")
        .bind(request)
        .execute(&pool)
        .await
        .expect("issue");
    full_refund(&pool, booking).await;
    assert_eq!(
        request_state(&pool, request).await,
        "issued",
        "an issued invoice is only cancelled by the owner on 홈택스"
    );
    let notes: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'tax_invoice.refund_needs_cancel' AND data->>'payment' = $1")
        .bind(payment.to_string())
        .fetch_one(&pool)
        .await
        .expect("notes");
    assert_eq!(notes, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn full_refund_before_settlement_link_cancels_the_pending_request(pool: PgPool) {
    let (booking, _payment, checkout) = paid_booking(&pool, "01070000304").await;
    sqlx::query("UPDATE checkouts SET provider_payment_id = 'tx-01070000304' WHERE id = $1")
        .bind(checkout)
        .execute(&pool)
        .await
        .expect("checkout");
    let request: Uuid = sqlx::query_scalar(
        "INSERT INTO tax_invoice_requests (checkout_id, business_number, company_name, email, state)
         VALUES ($1, '2208162517', 'LATE-CO', 'tax@example.test', 'pending') RETURNING id",
    )
    .bind(checkout)
    .fetch_one(&pool)
    .await
    .expect("request");
    full_refund(&pool, booking).await;
    api::routes::tax_invoices::on_settled(&pool, checkout, "tx-01070000304").await;
    assert_eq!(request_state(&pool, request).await, "cancelled", "the refund ran before the link");
}

#[sqlx::test(migrations = "../../migrations")]
async fn settlement_link_does_not_queue_a_payment_already_refunded(pool: PgPool) {
    let (_booking, payment, checkout) = paid_booking(&pool, "01070000305").await;
    sqlx::query("UPDATE payments SET status = 'refunded', refunded_amount = amount WHERE id = $1")
        .bind(payment)
        .execute(&pool)
        .await
        .expect("refund");
    let request: Uuid = sqlx::query_scalar(
        "INSERT INTO tax_invoice_requests (checkout_id, business_number, company_name, email, state)
         VALUES ($1, '2208162517', 'LINK-CO', 'tax@example.test', 'pending') RETURNING id",
    )
    .bind(checkout)
    .fetch_one(&pool)
    .await
    .expect("request");
    api::routes::tax_invoices::on_settled(&pool, checkout, "tx-01070000305").await;
    assert_eq!(request_state(&pool, request).await, "cancelled", "never queued for the owner");
}
