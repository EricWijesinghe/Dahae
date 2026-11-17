//! Payout reconciliation and money safety over a migrated Postgres: background sync (status only,
//! never a second send), owner alerts, batch amount checks, booking cancel vs batches, future-date
//! refusal, and no-store on account reveal. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, crypto, providers::payout as adapter};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::Duration;
use common::{app, call, prod_app, seed_catalog, test_config};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const DATE: &str = "2026-03-12";
const KEY: [u8; 32] = [7u8; 32];

async fn admin_token(db: &PgPool) -> String {
    common::staff_token(db, Role::StaffAdmin).await.0
}

/// Verified pro with a bank account and one pending settlement due by `DATE`. Returns (settlement, pro).
async fn seed(db: &PgPool) -> (Uuid, Uuid) {
    seed_catalog(db).await;
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ('01072000001', 'pay') RETURNING id")
        .fetch_one(db)
        .await
        .expect("user");
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone, name, team_name, status) VALUES ('01072000101', 'pro', 'team', 'verified') RETURNING id")
        .fetch_one(db)
        .await
        .expect("pro");
    let enc = crypto::encrypt(&KEY, crypto::aad::PAYOUT_ACCOUNT, "110-123-456789");
    sqlx::query(
        "INSERT INTO payout_accounts (pro_id, bank, account_enc, account_last4, holder, verified_at) VALUES ($1, '국민은행', $2, '6789', '홍길동', now())",
    )
    .bind(pro)
    .bind(enc)
    .execute(db)
    .await
    .expect("account");
    let booking = insert_booking(db, user, "BKP-1", "2026-03-05").await;
    let settlement: Uuid = sqlx::query_scalar(
        "INSERT INTO settlements (pro_id, booking_id, gross, fee, net, payout_date, state)
         VALUES ($1, $2, 100000, 20000, 80000, '2026-03-10', 'pending') RETURNING id",
    )
    .bind(pro)
    .bind(booking)
    .fetch_one(db)
    .await
    .expect("settlement");
    (settlement, pro)
}

async fn insert_booking(db: &PgPool, user: Uuid, number: &str, visit: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount)
         VALUES ($1, $2, 'studio', '{}'::jsonb, '11680', $3::date, 10, 3, 100000, 100000, 0) RETURNING id",
    )
    .bind(number)
    .bind(user)
    .bind(visit)
    .fetch_one(db)
    .await
    .expect("booking")
}

/// A second pending settlement for the same pro and payout date, on its own booking.
async fn second_settlement(db: &PgPool, pro: Uuid) -> Uuid {
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE phone = '01072000001'")
        .fetch_one(db)
        .await
        .expect("user");
    let booking = insert_booking(db, user, "BKP-2", "2026-03-06").await;
    sqlx::query_scalar(
        "INSERT INTO settlements (pro_id, booking_id, gross, fee, net, payout_date, state)
         VALUES ($1, $2, 100000, 20000, 80000, '2026-03-10', 'pending') RETURNING id",
    )
    .bind(pro)
    .bind(booking)
    .fetch_one(db)
    .await
    .expect("settlement")
}

async fn settlement_state(db: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM settlements WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
        .expect("state")
}

async fn request_payouts(app: &axum::Router, token: &str) -> (StatusCode, serde_json::Value) {
    call(app, "POST", "/v1/admin/settlements/run-payouts", Some(token), Some(json!({ "date": DATE }))).await
}

/// Turns a requested payout into a Toss transfer in the given state, `days` old.
async fn make_toss(db: &PgPool, id: &str, state: &str, transfer: Option<&str>, failure: Option<&str>, days: i32) {
    sqlx::query(
        "UPDATE payouts SET provider = 'toss', state = $2, provider_transfer_id = $3, failure_reason = $4, attempts = 1,
           updated_at = now() - make_interval(days => $5) WHERE id = $1::uuid",
    )
    .bind(id)
    .bind(state)
    .bind(transfer)
    .bind(failure)
    .bind(days)
    .execute(db)
    .await
    .expect("toss row");
}

async fn first_payout_id(app: &axum::Router, token: &str) -> String {
    let (_, rows) = request_payouts(app, token).await;
    rows[0]["id"].as_str().expect("id").to_string()
}

#[sqlx::test(migrations = "../../migrations")]
async fn background_sync_settles_open_transfers_and_never_resends(pool: PgPool) {
    let (settlement, _pro) = seed(&pool).await;
    let app = app(pool.clone());
    let id = first_payout_id(&app, &admin_token(&pool).await).await;
    make_toss(&pool, &id, "sent", Some("tp_1"), None, 2).await;

    let state = api::build_state(pool.clone(), test_config()).expect("state");
    assert_eq!(api::payout_sync::sync_pass(&state).await.expect("pass"), 1);

    let (st, attempts, transfer): (String, i32, Option<String>) =
        sqlx::query_as("SELECT state, attempts, provider_transfer_id FROM payouts WHERE id = $1::uuid")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .expect("row");
    assert_eq!(
        (st.as_str(), attempts, transfer.as_deref()),
        ("paid", 1, Some("tp_1")),
        "paid by status only, no second send"
    );
    assert_eq!(settlement_state(&pool, settlement).await, "paid");
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'payout.sync' AND actor_role = 'system'")
        .fetch_one(&pool)
        .await
        .expect("audit");
    assert_eq!(audited, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn background_sync_leaves_an_ambiguous_send_for_a_same_key_resend(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool.clone());
    let id = first_payout_id(&app, &admin_token(&pool).await).await;
    make_toss(&pool, &id, "requested", None, Some("timeout"), 0).await;
    let key_before: String = sqlx::query_scalar("SELECT idempotency_key FROM payouts WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("key");

    // Toss has no record yet of a send that just failed: it stays open for a same-key resend.
    let state = api::build_state(pool.clone(), test_config()).expect("state");
    api::payout_sync::sync_pass(&state).await.expect("pass");
    let (st, key): (String, String) = sqlx::query_as("SELECT state, idempotency_key FROM payouts WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(st, "requested", "never marked paid without the provider confirming");
    assert_eq!(key, key_before, "the resend reuses the key, so it cannot pay twice");
}

#[sqlx::test(migrations = "../../migrations")]
async fn background_sync_ignores_manual_requested_payouts(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool.clone());
    request_payouts(&app, &admin_token(&pool).await).await;
    let state = api::build_state(pool.clone(), test_config()).expect("state");
    assert_eq!(api::payout_sync::sync_pass(&state).await.expect("pass"), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_alerts_count_stuck_and_failed_transfers(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool.clone());
    let id = first_payout_id(&app, &admin_token(&pool).await).await;
    assert_eq!(api::payout_sync::alert_counts(&pool).await.expect("counts"), (0, 0));

    make_toss(&pool, &id, "sent", Some("tp_2"), None, 1).await;
    assert_eq!(api::payout_sync::alert_counts(&pool).await.expect("counts"), (1, 0), "open for over 24 h");

    make_toss(&pool, &id, "failed", None, Some("잔액 부족"), 0).await;
    assert_eq!(api::payout_sync::alert_counts(&pool).await.expect("counts"), (0, 1), "failed today");
}

#[sqlx::test(migrations = "../../migrations")]
async fn amount_that_no_longer_matches_its_settlements_is_not_paid(pool: PgPool) {
    let (settlement, _pro) = seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    sqlx::query("UPDATE settlements SET fee = fee + 1000, net = net - 1000 WHERE id = $1")
        .bind(settlement)
        .execute(&pool)
        .await
        .expect("change");

    let (code, body) = call(
        &app,
        "POST",
        &format!("/v1/admin/settlements/payouts/{id}/confirm"),
        Some(&token),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("payout_amount_changed")),
        "{body}"
    );
    assert_eq!(settlement_state(&pool, settlement).await, "pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelling_a_booking_takes_it_out_of_an_unsent_batch(pool: PgPool) {
    let (settlement, pro) = seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    let other = second_settlement(&pool, pro).await;
    let batch = Uuid::parse_str(&id).expect("uuid");
    sqlx::query("UPDATE settlements SET payout_id = $2 WHERE id = $1")
        .bind(other)
        .bind(batch)
        .execute(&pool)
        .await
        .expect("join batch");
    sqlx::query("UPDATE payouts SET amount = 160000, items = 2 WHERE id = $1")
        .bind(batch)
        .execute(&pool)
        .await
        .expect("batch total");

    let booking: Uuid = sqlx::query_scalar("SELECT booking_id FROM settlements WHERE id = $1")
        .bind(settlement)
        .fetch_one(&pool)
        .await
        .expect("booking");
    sqlx::query("UPDATE bookings SET status = 1 WHERE id = $1")
        .bind(booking)
        .execute(&pool)
        .await
        .expect("status");
    let (code, body) = call(&app, "POST", &format!("/v1/admin/bookings/{booking}/cancel"), Some(&token), Some(json!({}))).await;
    assert_eq!(code, StatusCode::OK, "{body}");

    assert_eq!(settlement_state(&pool, settlement).await, "held");
    let (amount, items): (i64, i32) = sqlx::query_as("SELECT amount, items FROM payouts WHERE id = $1")
        .bind(batch)
        .fetch_one(&pool)
        .await
        .expect("batch");
    assert_eq!((amount, items), (80_000, 1), "batch shrinks to the remaining job");
    let linked: Option<Uuid> = sqlx::query_scalar("SELECT payout_id FROM settlements WHERE id = $1")
        .bind(settlement)
        .fetch_one(&pool)
        .await
        .expect("link");
    assert_eq!(linked, None, "cancelled job is detached");
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_is_refused_while_the_transfer_is_at_the_provider(pool: PgPool) {
    let (settlement, _pro) = seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    make_toss(&pool, &id, "sent", Some("tp_3"), None, 0).await;
    let booking: Uuid = sqlx::query_scalar("SELECT booking_id FROM settlements WHERE id = $1")
        .bind(settlement)
        .fetch_one(&pool)
        .await
        .expect("booking");
    sqlx::query("UPDATE bookings SET status = 1 WHERE id = $1")
        .bind(booking)
        .execute(&pool)
        .await
        .expect("status");

    let (code, body) = call(&app, "POST", &format!("/v1/admin/bookings/{booking}/cancel"), Some(&token), Some(json!({}))).await;
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("settlement_in_transfer")),
        "{body}"
    );
    assert_eq!(settlement_state(&pool, settlement).await, "pending", "nothing changed");
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_refuses_future_dates_and_skips_pros_already_in_a_batch(pool: PgPool) {
    let (_settlement, pro) = seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let future = (api::util::today_kst() + Duration::days(2)).to_string();
    let (code, _) = call(&app, "POST", "/v1/admin/settlements/run-payouts", Some(&token), Some(json!({ "date": future }))).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "future payout dates are refused");

    request_payouts(&app, &token).await;
    second_settlement(&pool, pro).await;
    let (code, rows) = request_payouts(&app, &token).await;
    assert_eq!(code, StatusCode::OK, "{rows}");
    assert_eq!(rows.as_array().map(Vec::len), Some(0), "the pro already has a batch for that date");
    let batches: i64 = sqlx::query_scalar("SELECT count(*) FROM payouts WHERE pro_id = $1")
        .bind(pro)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(batches, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn revealed_account_is_never_cached(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/admin/settlements/payouts/{id}/account"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request");
    let res = app.clone().oneshot(req).await.expect("router is infallible");
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()), Some("no-store"));
}

async fn booking_of(db: &PgPool, settlement: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT booking_id FROM settlements WHERE id = $1")
        .bind(settlement)
        .fetch_one(db)
        .await
        .expect("booking")
}

async fn set_active(db: &PgPool, booking: Uuid) {
    sqlx::query("UPDATE bookings SET status = 1 WHERE id = $1")
        .bind(booking)
        .execute(db)
        .await
        .expect("status");
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_toss_transfer_does_not_block_a_cancel(pool: PgPool) {
    let (settlement, _pro) = seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    // Toss declined the transfer: no money moved, so the job can still be cancelled.
    make_toss(&pool, &id, "failed", None, Some("계좌 오류"), 0).await;
    let booking = booking_of(&pool, settlement).await;
    set_active(&pool, booking).await;

    let (code, body) = call(&app, "POST", &format!("/v1/admin/bookings/{booking}/cancel"), Some(&token), Some(json!({}))).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(settlement_state(&pool, settlement).await, "held");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_cannot_mark_an_ambiguous_toss_send_failed(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    // A send that timed out may already exist at Toss: only a sync can settle it, never a manual fail.
    make_toss(&pool, &id, "requested", None, Some("timeout"), 0).await;

    let (code, body) = call(
        &app,
        "POST",
        &format!("/v1/admin/settlements/payouts/{id}/fail"),
        Some(&token),
        Some(json!({ "reason": "직접 확인" })),
    )
    .await;
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("payout_in_transfer")),
        "{body}"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM payouts WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("state");
    assert_eq!(state, "requested");
}

#[sqlx::test(migrations = "../../migrations")]
async fn refund_waits_for_a_send_in_flight_and_refuses_without_moving_money(pool: PgPool) {
    let (settlement, _pro) = seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    let booking = booking_of(&pool, settlement).await;
    set_active(&pool, booking).await;
    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (booking_id, amount, kind, status, provider, provider_tx_id, idempotency_key)
         VALUES ($1, 100000, 'full', 'paid', 'fake', 'tx-race', 'pay-race') RETURNING id",
    )
    .bind(booking)
    .fetch_one(&pool)
    .await
    .expect("payment");

    // A send holds the batch row while it calls Toss; the cancel must wait for it before any refund.
    let batch = Uuid::parse_str(&id).expect("uuid");
    let mut sending = pool.begin().await.expect("tx");
    sqlx::query("SELECT id FROM payouts WHERE id = $1 FOR UPDATE")
        .bind(batch)
        .execute(&mut *sending)
        .await
        .expect("lock");
    let cancel_app = app.clone();
    let cancel_token = token.clone();
    let cancel = tokio::spawn(async move {
        call(
            &cancel_app,
            "POST",
            &format!("/v1/admin/bookings/{booking}/cancel"),
            Some(&cancel_token),
            Some(json!({})),
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    sqlx::query("UPDATE payouts SET state = 'sent', attempts = 1, provider_transfer_id = 'tp_race' WHERE id = $1")
        .bind(batch)
        .execute(&mut *sending)
        .await
        .expect("sent");
    sending.commit().await.expect("commit");

    let (code, body) = cancel.await.expect("join");
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("settlement_in_transfer")),
        "{body}"
    );
    let (refunded, status): (i64, String) = sqlx::query_as("SELECT refunded_amount, status FROM payments WHERE id = $1")
        .bind(payment)
        .fetch_one(&pool)
        .await
        .expect("payment");
    assert_eq!((refunded, status.as_str()), (0, "paid"), "nothing refunded for a job that stays active");
    let booking_status: i16 = sqlx::query_scalar("SELECT status FROM bookings WHERE id = $1")
        .bind(booking)
        .fetch_one(&pool)
        .await
        .expect("status");
    assert_eq!(booking_status, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_open_toss_transfer_blocks_clearing_the_toss_card(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool.clone());
    let id = first_payout_id(&app, &admin_token(&pool).await).await;
    make_toss(&pool, &id, "sent", Some("tp_open"), None, 0).await;
    assert!(
        api::payout_sync::toss_transfers_open(&pool).await.expect("query"),
        "a sent transfer needs the Toss keys to settle"
    );
    make_toss(&pool, &id, "paid", Some("tp_open"), None, 0).await;
    assert!(
        !api::payout_sync::toss_transfers_open(&pool).await.expect("query"),
        "settled transfers do not block"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_can_fail_a_toss_payout_that_was_declined_and_retried(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool.clone());
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app, &token).await;
    // Declined once (no money moved), then retried: attempts is 1 but nothing may exist at Toss.
    make_toss(&pool, &id, "requested", None, None, 0).await;

    let (code, body) = call(
        &app,
        "POST",
        &format!("/v1/admin/settlements/payouts/{id}/fail"),
        Some(&token),
        Some(json!({ "reason": "직접 확인" })),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let state: String = sqlx::query_scalar("SELECT state FROM payouts WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("state");
    assert_eq!(state, "failed");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_send_refused_for_missing_keys_leaves_the_batch_as_it_was(pool: PgPool) {
    seed(&pool).await;
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app(pool.clone()), &token).await;
    sqlx::query("UPDATE payouts SET provider = 'toss', attempts = 0 WHERE id = $1::uuid")
        .bind(&id)
        .execute(&pool)
        .await
        .expect("toss");
    let fingerprint = adapter::account_fingerprint(adapter::bank_code("국민은행").expect("bank"), "110-123-456789", "홍길동");
    sqlx::query("UPDATE pros SET toss_seller_id = 'seller-1', toss_seller_fp = $1")
        .bind(fingerprint)
        .execute(&pool)
        .await
        .expect("seller");

    // Production router with no Toss keys: refused before any request leaves, so nothing may be recorded as sent.
    let prod = prod_app(pool.clone(), "http://127.0.0.1:9");
    let (_, body) = call(
        &prod,
        "POST",
        &format!("/v1/admin/settlements/payouts/{id}/send"),
        Some(&token),
        Some(json!({})),
    )
    .await;
    assert_eq!(body["error"]["code"].as_str(), Some("provider_not_configured"), "{body}");
    let row: (String, i32, Option<String>) = sqlx::query_as("SELECT state, attempts, failure_reason FROM payouts WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(row, ("requested".to_string(), 0, None), "no ambiguous-send mark, so the booking is not blocked");
}

const SECURITY_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OWNER_SEED: &[u8] = b"payout-owner-seed-0001";

/// Toss mock: `GET /v2/payouts` answers `items` as one complete page.
async fn start_toss(items: serde_json::Value) -> String {
    let body = json!({ "entityType": "payout-list", "entityBody": { "hasMore": false, "items": items } });
    let router = axum::Router::new().route("/v2/payouts", axum::routing::get(move || async move { axum::Json(body) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind toss mock");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, router).await.expect("mock serves") });
    format!("http://{addr}")
}

/// Production-mode state with Toss payout keys, its Toss calls sent to `toss_base`.
fn toss_state(pool: &PgPool, toss_base: &str) -> api::state::AppState {
    let mut cfg = test_config();
    cfg.dev_mode = false;
    let state = api::build_state(pool.clone(), cfg.clone()).expect("state");
    let providers = api::providers::Providers::new(cfg.clone(), pool.clone())
        .expect("providers")
        .with_toss_api(toss_base);
    let mut integ = api::integrations::Integrations::from_config(&cfg);
    integ.payout = Some(api::integrations::PayoutSettings {
        mode: api::integrations::PayoutMode::Toss,
        secret_key: Some("test_sk_payout".into()),
        security_key: Some(SECURITY_HEX.into()),
    });
    providers.set_integrations(integ);
    api::state::AppState {
        providers: std::sync::Arc::new(providers),
        ..state
    }
}

/// A requested batch whose Toss send ended ambiguously `days` ago. Returns (payout id, settlement).
async fn ambiguous_batch(pool: &PgPool, days: i32) -> (String, Uuid) {
    let (settlement, _pro) = seed(pool).await;
    let id = first_payout_id(&app(pool.clone()), &admin_token(pool).await).await;
    make_toss(pool, &id, "requested", None, Some("status 502 Bad Gateway"), days).await;
    (id, settlement)
}

async fn payout_row(pool: &PgPool, id: &str) -> (String, Option<String>, String) {
    sqlx::query_as("SELECT state, provider_transfer_id, idempotency_key FROM payouts WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("row")
}

/// An owner-admin token (stepped up, TOTP enrolled) and a code for now.
async fn owner(pool: &PgPool) -> (String, String) {
    let (token, staff) = common::staff_token(pool, Role::StaffAdmin).await;
    common::enrol_owner(pool, staff, OWNER_SEED).await;
    let step = (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64;
    (token, format!("{:06}", api::totp::code_at(OWNER_SEED, step)))
}

async fn not_sent(app: &axum::Router, id: &str, token: &str, code: Option<&str>) -> (StatusCode, serde_json::Value) {
    common::call_owner(app, "POST", &format!("/v1/admin/settlements/payouts/{id}/not-sent"), token, code, json!({})).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn status_check_finds_an_ambiguous_send_at_toss_by_its_ref_id(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 0).await;
    let other = json!({ "id": "FPA_other", "refPayoutId": Uuid::nil().to_string(), "status": "COMPLETED" });
    let ours = json!({ "id": "FPA_ours", "refPayoutId": id, "status": "IN_PROGRESS" });
    let state = toss_state(&pool, &start_toss(json!([other, ours])).await);

    api::payout_sync::sync_pass(&state).await.expect("pass");
    let (st, transfer, _) = payout_row(&pool, &id).await;
    assert_eq!(
        (st.as_str(), transfer.as_deref()),
        ("sent", Some("FPA_ours")),
        "Toss has it: sent, with its id kept"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn status_check_leaves_a_send_toss_does_not_list_for_the_owner(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 1).await;
    let before = payout_row(&pool, &id).await;
    let state = toss_state(&pool, &start_toss(json!([])).await);

    // Absence from the list is not proof: the lookup's date filter and Toss's refPayoutId rules are unverified.
    assert_eq!(api::payout_sync::sync_pass(&state).await.expect("pass"), 0);
    assert_eq!(payout_row(&pool, &id).await, before, "still ambiguous, same key, for the owner to settle");
    let failure: Option<String> = sqlx::query_scalar("SELECT failure_reason FROM payouts WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("failure");
    assert_eq!(failure.as_deref(), Some("status 502 Bad Gateway"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_not_sent_when_toss_lists_nothing_fails_the_batch_and_keeps_the_key(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 0).await;
    let (_, _, key_before) = payout_row(&pool, &id).await;
    let app = api::routes::router(toss_state(&pool, &start_toss(json!([])).await));
    let (token, totp) = owner(&pool).await;

    let (code, body) = not_sent(&app, &id, &token, Some(&totp)).await;
    assert_eq!((code, body["state"].as_str()), (StatusCode::OK, Some("failed")), "{body}");
    let (st, _, key) = payout_row(&pool, &id).await;
    assert_eq!((st.as_str(), key), ("failed", key_before), "the owner's word never rotates the key");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_not_sent_needs_the_owner_and_a_code(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 0).await;
    let app = api::routes::router(toss_state(&pool, "http://127.0.0.1:9"));
    let (owner_token, _) = owner(&pool).await;
    let (code, body) = not_sent(&app, &id, &owner_token, None).await;
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("totp_required")),
        "{body}"
    );

    let admin = admin_token(&pool).await;
    let (code, body) = not_sent(&app, &id, &admin, Some("123456")).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "an admin who is not the owner: {body}");
    assert_eq!(payout_row(&pool, &id).await.0, "requested");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_not_sent_is_refused_when_toss_shows_the_transfer(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 0).await;
    let ours = json!({ "id": "FPA_ours", "refPayoutId": id, "status": "IN_PROGRESS" });
    let app = api::routes::router(toss_state(&pool, &start_toss(json!([ours])).await));
    let (token, totp) = owner(&pool).await;

    let (code, body) = not_sent(&app, &id, &token, Some(&totp)).await;
    assert_eq!((code, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("payout_checked")), "{body}");
    assert_eq!(payout_row(&pool, &id).await.0, "sent", "the answer from Toss wins over the owner");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_not_sent_while_toss_is_unreachable_fails_the_batch_and_unblocks_cancel(pool: PgPool) {
    let (id, settlement) = ambiguous_batch(&pool, 0).await;
    let (_, _, key_before) = payout_row(&pool, &id).await;
    let app = api::routes::router(toss_state(&pool, "http://127.0.0.1:9"));
    let (token, totp) = owner(&pool).await;
    let booking = booking_of(&pool, settlement).await;
    set_active(&pool, booking).await;
    let cancel = format!("/v1/admin/bookings/{booking}/cancel");
    let (_, body) = call(&app, "POST", &cancel, Some(&token), Some(json!({}))).await;
    assert_eq!(body["error"]["code"].as_str(), Some("settlement_in_transfer"), "blocked first: {body}");

    let (code, body) = not_sent(&app, &id, &token, Some(&totp)).await;
    assert_eq!((code, body["state"].as_str()), (StatusCode::OK, Some("failed")), "{body}");
    let (st, _, key) = payout_row(&pool, &id).await;
    assert_eq!(st, "failed");
    assert_eq!(key, key_before, "a resend keeps the key, so a send Toss did take is answered, not repeated");
    let (actor, detail): (Option<Uuid>, serde_json::Value) =
        sqlx::query_as("SELECT actor_id, data FROM audit_log WHERE action = 'payout.not_sent' AND target = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .expect("audit row");
    assert!(actor.is_some(), "who confirmed it");
    assert_eq!(detail["send_error"], "status 502 Bad Gateway", "{detail}");
    let (code, body) = not_sent(&app, &id, &token, Some(&totp)).await;
    assert_eq!(code, StatusCode::CONFLICT, "only once: {body}");

    let (code, body) = call(&app, "POST", &cancel, Some(&token), Some(json!({}))).await;
    assert_eq!(code, StatusCode::OK, "{body}");
}

/// Toss mock: `GET /v2/payouts/{id}` answers that payout with `status`.
async fn start_toss_status(status: &'static str) -> String {
    let router = axum::Router::new().route(
        "/v2/payouts/{id}",
        axum::routing::get(move |axum::extract::Path(id): axum::extract::Path<String>| async move { axum::Json(json!({ "id": id, "status": status })) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind toss mock");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, router).await.expect("mock serves") });
    format!("http://{addr}")
}

/// The pro is a Toss seller whose registered account is the one on the payout.
async fn toss_seller(pool: &PgPool) {
    let fingerprint = adapter::account_fingerprint(adapter::bank_code("국민은행").expect("bank"), "110-123-456789", "홍길동");
    sqlx::query("UPDATE pros SET toss_seller_id = 'seller-1', toss_seller_fp = $1")
        .bind(fingerprint)
        .execute(pool)
        .await
        .expect("seller");
}

async fn payout_action(app: &axum::Router, token: &str, id: &str, action: &str) -> String {
    let (code, body) = call(
        app,
        "POST",
        &format!("/v1/admin/settlements/payouts/{id}/{action}"),
        Some(token),
        Some(json!({})),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{action}: {body}");
    body["state"].as_str().expect("state").to_string()
}

/// The refPayoutId the dev transfer carried (the dev fake answers `dev_<refPayoutId>`).
async fn sent_ref(pool: &PgPool, id: &str) -> String {
    let (_, transfer, _) = payout_row(pool, id).await;
    transfer.and_then(|t| t.strip_prefix("dev_").map(String::from)).expect("dev transfer id")
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_retry_after_toss_failed_the_transfer_sends_a_new_ref_payout_id(pool: PgPool) {
    seed(&pool).await;
    let token = admin_token(&pool).await;
    let dev = app(pool.clone());
    let id = first_payout_id(&dev, &token).await;
    toss_seller(&pool).await;
    sqlx::query("UPDATE payouts SET provider = 'toss' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&pool)
        .await
        .expect("toss");
    assert_eq!(payout_action(&dev, &token, &id, "send").await, "sent");
    let first = sent_ref(&pool, &id).await;

    // Toss's own answer: the transfer FAILED, no money moved.
    api::payout_sync::sync_pass(&toss_state(&pool, &start_toss_status("FAILED").await))
        .await
        .expect("pass");
    assert_eq!(payout_row(&pool, &id).await.0, "failed");

    assert_eq!(payout_action(&dev, &token, &id, "retry").await, "requested");
    assert_eq!(payout_action(&dev, &token, &id, "send").await, "sent");
    let second = sent_ref(&pool, &id).await;
    assert_ne!(second, first, "Toss refuses a refPayoutId it has seen, so the retry needs a new one");
    assert!(
        second.starts_with(&id) && second.len() <= 50,
        "still traceable to the batch, within Toss's 50 chars: {second}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_ambiguous_send_keeps_its_ref_payout_id_through_the_owners_not_sent(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 0).await;
    toss_seller(&pool).await;
    let (token, totp) = owner(&pool).await;
    let toss = api::routes::router(toss_state(&pool, &start_toss(json!([])).await));
    let (code, body) = not_sent(&toss, &id, &token, Some(&totp)).await;
    assert_eq!((code, body["state"].as_str()), (StatusCode::OK, Some("failed")), "{body}");

    let dev = app(pool.clone());
    assert_eq!(payout_action(&dev, &token, &id, "retry").await, "requested");
    assert_eq!(payout_action(&dev, &token, &id, "send").await, "sent");
    assert_eq!(
        sent_ref(&pool, &id).await,
        id,
        "no Toss answer: the reference stays, so a send Toss did take is never repeated"
    );
}

/// Toss mock with scripted answers. `POST /v2/payouts` answers `sends` in turn (the last one repeats) and keeps
/// every decrypted body in `sent`; the lookups (`GET /v2/payouts` and `GET /v2/payouts/{id}`) answer `get`.
struct TossMock {
    base: String,
    sent: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
}

type Answer = (StatusCode, serde_json::Value);

async fn start_toss_mock(sends: Vec<Answer>, get: Answer) -> TossMock {
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = std::sync::Arc::clone(&sent);
    let key = api::providers::toss_jwe::key_bytes(SECURITY_HEX).expect("key");
    let send = move |body: String| async move {
        let mut log = log.lock().expect("log");
        log.push(api::providers::toss_jwe::open(&key, &body).expect("encrypted body"));
        let (status, answer) = sends[(log.len() - 1).min(sends.len() - 1)].clone();
        (status, axum::Json(answer))
    };
    let lookup = move || async move { (get.0, axum::Json(get.1)) };
    let router = axum::Router::new()
        .route("/v2/payouts", axum::routing::post(send).get(lookup.clone()))
        .route("/v2/payouts/{id}", axum::routing::get(lookup));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind toss mock");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, router).await.expect("mock serves") });
    TossMock {
        base: format!("http://{addr}"),
        sent,
    }
}

fn empty_list() -> Answer {
    (
        StatusCode::OK,
        json!({ "entityType": "payout-list", "entityBody": { "hasMore": false, "items": [] } }),
    )
}

/// The one transfer in a create body (Toss takes a list of up to 100).
fn transfer_in(body: &serde_json::Value) -> &serde_json::Value {
    body.get(0).unwrap_or(body)
}

async fn toss_meta(pool: &PgPool, id: &str) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT failure_reason, toss_ref FROM payouts WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("row")
}

async fn send_now(app: &axum::Router, token: &str, id: &str) -> (StatusCode, serde_json::Value) {
    call(app, "POST", &format!("/v1/admin/settlements/payouts/{id}/send"), Some(token), Some(json!({}))).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_resend_of_an_ambiguous_send_keeps_its_key_and_ref(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 0).await;
    toss_seller(&pool).await;
    let (_, _, key_before) = payout_row(&pool, &id).await;
    // A 4xx on the re-send (a refPayoutId Toss already holds, a key it no longer replays) does not prove the first send failed.
    let refused = (
        StatusCode::BAD_REQUEST,
        json!({ "code": "INVALID_REQUEST", "message": "이미 사용된 refPayoutId" }),
    );
    let toss = start_toss_mock(vec![refused], empty_list()).await;
    let app = api::routes::router(toss_state(&pool, &toss.base));
    let token = admin_token(&pool).await;

    for _ in 0..2 {
        let (code, body) = send_now(&app, &token, &id).await;
        assert_eq!(
            (code, body["error"]["code"].as_str()),
            (StatusCode::CONFLICT, Some("payout_in_transfer")),
            "{body}"
        );
    }
    let (st, _, key) = payout_row(&pool, &id).await;
    assert_eq!((st.as_str(), key), ("requested", key_before), "still open for the sync or the owner, same key");
    let (failure, toss_ref) = toss_meta(&pool, &id).await;
    assert_eq!(failure.as_deref(), Some("이미 사용된 refPayoutId"));
    assert_eq!(toss_ref, None, "the reference is never rotated without an answer from Toss");
    let refs: Vec<_> = toss.sent.lock().expect("log").iter().map(|b| transfer_in(b)["refPayoutId"].clone()).collect();
    assert_eq!(
        refs,
        vec![json!(id), json!(id)],
        "the same refPayoutId every time, so Toss can never take it twice"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_first_send_fails_the_batch_with_a_new_key_and_ref(pool: PgPool) {
    seed(&pool).await;
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app(pool.clone()), &token).await;
    sqlx::query("UPDATE payouts SET provider = 'toss' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&pool)
        .await
        .expect("toss");
    toss_seller(&pool).await;
    let (_, _, key_before) = payout_row(&pool, &id).await;
    let toss = start_toss_mock(vec![(StatusCode::BAD_REQUEST, json!({ "message": "계좌 오류" }))], empty_list()).await;
    let app = api::routes::router(toss_state(&pool, &toss.base));

    let (code, body) = send_now(&app, &token, &id).await;
    assert_eq!((code, body["state"].as_str()), (StatusCode::OK, Some("failed")), "{body}");
    let (_, _, key) = payout_row(&pool, &id).await;
    assert_ne!(key, key_before, "every earlier send had an answer, so this refusal is definite");
    assert_eq!(toss_meta(&pool, &id).await.1, Some(format!("{id}-1")));
}


#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_list_lookup_is_unreachable_so_the_owners_not_sent_stays_open(pool: PgPool) {
    let (id, _) = ambiguous_batch(&pool, 0).await;
    let refused = (StatusCode::BAD_REQUEST, json!({ "code": "INVALID_REQUEST", "message": "잘못된 요청" }));
    let toss = start_toss_mock(vec![(StatusCode::BAD_GATEWAY, json!({}))], refused).await;
    let app = api::routes::router(toss_state(&pool, &toss.base));
    let (token, totp) = owner(&pool).await;

    let (code, body) = call(&app, "POST", &format!("/v1/admin/settlements/payouts/{id}/sync"), Some(&token), Some(json!({}))).await;
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("provider_unreachable")),
        "{body}"
    );
    let (code, body) = not_sent(&app, &id, &token, Some(&totp)).await;
    assert_eq!((code, body["state"].as_str()), (StatusCode::OK, Some("failed")), "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_status_lookup_leaves_a_sent_transfer_open(pool: PgPool) {
    seed(&pool).await;
    let token = admin_token(&pool).await;
    let id = first_payout_id(&app(pool.clone()), &token).await;
    make_toss(&pool, &id, "sent", Some("FPA_1"), None, 0).await;
    let not_found = (StatusCode::NOT_FOUND, json!({ "code": "PAYOUT_NOT_FOUND" }));
    let toss = start_toss_mock(vec![(StatusCode::BAD_GATEWAY, json!({}))], not_found).await;
    let app = api::routes::router(toss_state(&pool, &toss.base));

    let (code, body) = call(&app, "POST", &format!("/v1/admin/settlements/payouts/{id}/sync"), Some(&token), Some(json!({}))).await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(payout_row(&pool, &id).await.0, "sent");
}
