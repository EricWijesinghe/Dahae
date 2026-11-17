// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Business KPI alerts and flow metrics over a migrated Postgres. Rows are inserted directly so every
//! count is known in advance: unassigned bookings, the same-day cancel rate, no-shows per service and
//! the median time to assign. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{ops, ops_alerts, settings_registry};
use axum::http::StatusCode;
use common::{call, seed_catalog, staff_token, test_config};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const KST_TODAY: &str = "(now() AT TIME ZONE 'Asia/Seoul')::date";

async fn user(db: &PgPool, phone: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ($1, 'kpi') RETURNING id")
        .bind(phone)
        .fetch_one(db)
        .await
        .expect("user")
}

/// A booking on today's Seoul date (`visit_today`) or an earlier one. `status` -1 is cancelled, and a
/// cancelled booking is stamped as cancelled now.
async fn booking(db: &PgPool, user: Uuid, number: &str, status: i16, visit_today: bool) -> Uuid {
    let visit = if visit_today { KST_TODAY.to_string() } else { "'2026-03-15'".to_string() };
    let sql = format!(
        "INSERT INTO bookings (number, user_id, service_code, address, district_code, visit_date, start_hour, status, final_price, list_price, discount, cancelled_at)
         VALUES ($1, $2, 'studio', '{{}}'::jsonb, '11680', {visit}, 10, $3, 50000, 50000, 0,
                 CASE WHEN $3 = -1 THEN now() END)
         RETURNING id"
    );
    sqlx::query_scalar(&sql)
        .bind(number)
        .bind(user)
        .bind(status)
        .fetch_one(db)
        .await
        .expect("booking")
}

async fn age(db: &PgPool, id: Uuid, interval: &str) {
    sqlx::query("UPDATE bookings SET created_at = now() - $2::interval WHERE id = $1")
        .bind(id)
        .bind(interval)
        .execute(db)
        .await
        .expect("age booking");
}

async fn assigned_after(db: &PgPool, id: Uuid, interval: &str) {
    sqlx::query("UPDATE bookings SET accepted_at = created_at + $2::interval WHERE id = $1")
        .bind(id)
        .bind(interval)
        .execute(db)
        .await
        .expect("assign booking");
}

async fn no_show_case(db: &PgPool, id: Uuid) {
    sqlx::query(
        "INSERT INTO cases (booking_id, kind, source, reporter_role, summary, sla_due_at) VALUES ($1, 'no_show', 'no_show', 'system', 'no show', now())",
    )
    .bind(id)
    .execute(db)
    .await
    .expect("no-show case");
}

fn fired(findings: &[ops_alerts::Finding], rule: &str) -> bool {
    findings.iter().any(|f| f.rule == rule)
}

#[sqlx::test(migrations = "../../migrations")]
async fn unassigned_rule_counts_only_old_confirmed_bookings_with_nobody_on_them(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000001").await;
    let old = booking(&pool, u, "KPI-1", 1, true).await;
    age(&pool, old, "30 hours").await;
    let fresh = booking(&pool, u, "KPI-2", 1, true).await;
    age(&pool, fresh, "2 hours").await;
    let team = booking(&pool, u, "KPI-3", 1, true).await;
    age(&pool, team, "30 hours").await;
    sqlx::query("UPDATE bookings SET team_name = 'team' WHERE id = $1")
        .bind(team)
        .execute(&pool)
        .await
        .expect("team");

    assert_eq!(ops::unassigned_older_than(&pool, 24).await.expect("count"), 1);
    assert_eq!(ops::flow_metrics(&pool).await.expect("flow").unassigned_over_24h, 1);

    let findings = ops_alerts::business_findings(&pool, &test_config()).await.expect("findings");
    assert!(fired(&findings, "unassigned_stale"), "one old unassigned booking fires the rule");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unassigned_rule_ignores_bookings_whose_visit_has_passed(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000007").await;
    let abandoned = booking(&pool, u, "KPI-PAST", 1, false).await;
    age(&pool, abandoned, "30 hours").await;

    assert_eq!(ops::unassigned_older_than(&pool, 24).await.expect("count"), 0, "a past visit needs no staffing");
    let findings = ops_alerts::business_findings(&pool, &test_config()).await.expect("findings");
    assert!(
        !fired(&findings, "unassigned_stale"),
        "an abandoned past booking must not keep the alert firing"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_day_cancel_rule_fires_over_the_threshold_only(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000002").await;
    for i in 0..6 {
        booking(&pool, u, &format!("SDC-A{i}"), if i < 4 { -1 } else { 1 }, true).await;
    }
    for i in 0..4 {
        booking(&pool, u, &format!("SDC-B{i}"), 1, true).await;
    }
    // 4 cancelled of 10 today is 40%, over the default 30%.
    assert_eq!(ops::same_day_cancels(&pool).await.expect("counts"), (4, 10));
    let flow = ops::flow_metrics(&pool).await.expect("flow");
    assert_eq!((flow.same_day.cancelled, flow.same_day.booked), (4, 10), "the flow card shows the same counts");
    let findings = ops_alerts::business_findings(&pool, &test_config()).await.expect("findings");
    assert!(fired(&findings, "same_day_cancel"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_day_cancel_rule_stays_quiet_at_the_threshold_and_on_small_samples(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000003").await;
    for i in 0..3 {
        booking(&pool, u, &format!("SDQ-A{i}"), -1, true).await;
    }
    for i in 0..7 {
        booking(&pool, u, &format!("SDQ-B{i}"), 1, true).await;
    }
    // Exactly 30% is not over 30%.
    let findings = ops_alerts::business_findings(&pool, &test_config()).await.expect("findings");
    assert!(!fired(&findings, "same_day_cancel"), "exactly the threshold stays quiet");
}

#[sqlx::test(migrations = "../../migrations")]
async fn no_show_rate_is_per_service_and_leaves_out_cancelled_bookings(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000004").await;
    let first = booking(&pool, u, "NS-1", 1, false).await;
    booking(&pool, u, "NS-2", 3, false).await;
    booking(&pool, u, "NS-3", 1, false).await;
    booking(&pool, u, "NS-4", -1, false).await;
    no_show_case(&pool, first).await;

    let rows = ops::no_show_by_service(&pool).await.expect("rows");
    let studio = rows.iter().find(|r| r.service_code == "studio").expect("studio row");
    assert_eq!(studio.bookings, 3, "cancelled bookings are left out");
    assert_eq!(studio.no_shows, 1, "one booking with a no-show case");
    assert_eq!(studio.rate_bps, 3333, "one of three is 33.33%");
}

#[sqlx::test(migrations = "../../migrations")]
async fn no_show_rate_ignores_bookings_outside_the_window_and_cancelled_ones(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000008").await;
    booking(&pool, u, "NSW-1", 1, false).await;
    let old = booking(&pool, u, "NSW-2", 1, false).await;
    age(&pool, old, "40 days").await;
    no_show_case(&pool, old).await;
    let cancelled = booking(&pool, u, "NSW-3", -1, false).await;
    no_show_case(&pool, cancelled).await;

    let rows = ops::no_show_by_service(&pool).await.expect("rows");
    assert!(rows.iter().all(|r| r.rate_bps <= 10_000), "a rate can never pass 100%: {rows:?}");
    let studio = rows.iter().find(|r| r.service_code == "studio").expect("studio row");
    assert_eq!((studio.bookings, studio.no_shows, studio.rate_bps), (1, 0, 0), "{studio:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn median_hours_to_assign_reads_the_middle_of_assigned_bookings(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000005").await;
    let a = booking(&pool, u, "MED-1", 2, false).await;
    let b = booking(&pool, u, "MED-2", 2, false).await;
    let c = booking(&pool, u, "MED-3", 2, false).await;
    assigned_after(&pool, a, "2 hours").await;
    assigned_after(&pool, b, "4 hours").await;
    assigned_after(&pool, c, "9 hours").await;
    let median = ops::median_hours_to_assign(&pool).await.expect("median").expect("some");
    assert!((median - 4.0).abs() < 1e-9, "median of 2, 4, 9 is 4, got {median}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn dashboard_and_status_page_show_the_flow_figures(pool: PgPool) {
    seed_catalog(&pool).await;
    let u = user(&pool, "01080000006").await;
    let id = booking(&pool, u, "DASH-1", 2, false).await;
    assigned_after(&pool, id, "3 hours").await;
    let app = common::app(pool.clone());
    let (token, _) = staff_token(&pool, api::auth::Role::StaffAdmin).await;

    let (st, dash) = call(&app, "GET", "/v1/admin/dashboard", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{dash}");
    assert_eq!(dash["flow"]["median_hours_to_assign"], 3.0);
    assert_eq!(dash["flow"]["unassigned_over_24h"], 0);
    assert!(dash["flow"]["no_show_by_service"].is_array());

    let (st, status) = call(&app, "GET", "/v1/admin/ops/status", Some(token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{status}");
    let flow: &Value = &status["flow"];
    assert_eq!(flow["median_hours_to_assign"], 3.0);
    assert_eq!(flow["unassigned_over_24h"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_alert_thresholds_have_safe_defaults(pool: PgPool) {
    let cfg = test_config();
    assert_eq!(settings_registry::get(&pool, &cfg, "unassigned_alert_hours").await.expect("hours"), 24);
    assert_eq!(settings_registry::get(&pool, &cfg, "same_day_cancel_alert_pct").await.expect("pct"), 30);
    assert_eq!(settings_registry::get(&pool, &cfg, "tax_deadline_remind_days").await.expect("days"), 7);
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_photo_offsite_backup_alerts_until_a_later_upload_succeeds(pool: PgPool) {
    assert!(ops_alerts::offsite_backup_findings(&pool).await.expect("findings").is_empty(), "no upload yet");
    ops::write_outcome(&pool, "backup", Err("unreachable")).await.expect("database upload failed");
    let db_only = ops_alerts::offsite_backup_findings(&pool).await.expect("findings");
    assert!(
        !fired(&db_only, "backup_uploads_failing"),
        "a database upload failure does not fire the photo rule"
    );
    ops::write_outcome(&pool, ops::PHOTOS_INTEGRATION, Err("unreachable"))
        .await
        .expect("record failure");
    let findings = ops_alerts::offsite_backup_findings(&pool).await.expect("findings");
    assert!(fired(&findings, "backup_uploads_failing"), "photo failure fires the owner alert");
    assert!(ops_alerts::Prefs::default().enabled("backup_uploads_failing"), "on by default");
    ops::write_outcome(&pool, ops::PHOTOS_INTEGRATION, Ok(())).await.expect("record success");
    let after = ops_alerts::offsite_backup_findings(&pool).await.expect("findings");
    assert!(!fired(&after, "backup_uploads_failing"), "a later photo success clears the photo rule");
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_database_offsite_backup_alerts_until_a_later_upload_succeeds(pool: PgPool) {
    assert!(ops_alerts::offsite_backup_findings(&pool).await.expect("findings").is_empty(), "no upload yet");
    ops::write_outcome(&pool, "backup", Err("unreachable")).await.expect("record failure");
    let findings = ops_alerts::offsite_backup_findings(&pool).await.expect("findings");
    assert!(fired(&findings, "backup_offsite_failing"), "database upload failure fires the owner alert");
    assert!(!fired(&findings, "backup_uploads_failing"), "the photo rule is separate");
    assert!(ops_alerts::Prefs::default().enabled("backup_offsite_failing"), "on by default");
    ops::write_outcome(&pool, "backup", Ok(())).await.expect("record success");
    assert!(
        ops_alerts::offsite_backup_findings(&pool).await.expect("findings").is_empty(),
        "a later success clears it"
    );
}

/// Inserts `n` sign-in codes for made-up numbers sent now.
async fn seed_otp(db: &PgPool, n: i64) {
    sqlx::query(
        "INSERT INTO otp_codes (phone, code_hash, expires_at) SELECT '0107' || lpad(g::text, 7, '0'), 'x', now() + interval '3 minutes' FROM generate_series(1, $1::int) g",
    )
    .bind(n as i32)
    .execute(db)
    .await
    .expect("seed codes");
}

#[sqlx::test(migrations = "../../migrations")]
async fn otp_volume_alert_fires_at_the_warn_level_only(pool: PgPool) {
    let warn = ops_alerts::OTP_GLOBAL_WARN_PER_HOUR;
    seed_otp(&pool, warn - 1).await;
    assert!(ops_alerts::otp_volume(&pool).await.expect("rule").is_empty(), "under the warn level");
    seed_otp(&pool, 1).await;
    let found = ops_alerts::otp_volume(&pool).await.expect("rule");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rule, "otp_volume");
    assert!(!found[0].body.contains("막고"), "nothing is refused at the warn level: {}", found[0].body);
}

#[sqlx::test(migrations = "../../migrations")]
async fn otp_volume_alert_says_new_sign_ins_are_refused_at_the_ceiling(pool: PgPool) {
    seed_otp(&pool, ops_alerts::OTP_GLOBAL_CEILING_PER_HOUR).await;
    let found = ops_alerts::otp_volume(&pool).await.expect("rule");
    assert_eq!(found.len(), 1);
    assert!(found[0].body.contains("새로 가입") && found[0].body.contains("막고"), "{}", found[0].body);
}

/// A Toss batch whose send ended with no answer `minutes` ago.
async fn ambiguous_toss_payout(db: &PgPool, phone: &str, minutes: i32) {
    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone, name, status) VALUES ($1, 'pro', 'verified') RETURNING id")
        .bind(phone)
        .fetch_one(db)
        .await
        .expect("pro");
    sqlx::query(
        "INSERT INTO payouts (pro_id, payout_date, amount, items, provider, state, bank, holder, account_enc, account_last4, failure_reason, updated_at)
         VALUES ($1, current_date, 80000, 1, 'toss', 'requested', '국민은행', '홍길동', 'x', '6789', 'status 502', now() - make_interval(mins => $2))",
    )
    .bind(pro)
    .bind(minutes)
    .execute(db)
    .await
    .expect("payout");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unanswered_toss_payout_alerts_the_owner_after_half_an_hour(pool: PgPool) {
    let state = api::build_state(pool.clone(), test_config()).expect("state");
    ambiguous_toss_payout(&pool, "01080000901", 10).await;
    let findings = ops_alerts::evaluate(&state).await.expect("findings");
    assert!(!fired(&findings, "payout_unconfirmed"), "a fresh one may still settle by itself");

    ambiguous_toss_payout(&pool, "01080000902", 45).await;
    let findings = ops_alerts::evaluate(&state).await.expect("findings");
    assert!(fired(&findings, "payout_unconfirmed"), "the owner hears about it");
    assert!(ops_alerts::RULES.contains(&"payout_unconfirmed"), "listed on the alert card");
    assert!(ops_alerts::Prefs::default().enabled("payout_unconfirmed"), "on by default");
}
