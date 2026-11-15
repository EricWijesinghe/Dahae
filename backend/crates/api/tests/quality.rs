//! Training gate on job acceptance. Off by default, so a required module never blocks offers until the owner
//! turns `training_gate_enabled` on. When on, a published required module without progress blocks, and
//! completing it lifts the block. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, build_state, error::ApiError, noshow, quality};
use axum::{Router, http::StatusCode};
use chrono::Utc;
use common::{book, call, customer, login, seed_catalog, staff_token, test_config, verified_pro};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn publish_required_module(db: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO training_modules (title, body, checklist, quiz, required, published)
         VALUES ('{\"ko\": \"안전 교육\"}', '{}', '[]', '{}', TRUE, TRUE) RETURNING id",
    )
    .fetch_one(db)
    .await
    .expect("module")
}

async fn set_training_gate(app: &Router, token: &str, on: bool) {
    let (st, doc) = call(
        app,
        "PUT",
        "/v1/admin/settings/quality",
        Some(token),
        Some(json!({ "values": { "training_gate_enabled": on } })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{doc}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn required_module_does_not_block_while_gate_is_off(pool: PgPool) {
    let app = common::app(pool.clone());
    let state = build_state(pool.clone(), test_config()).expect("state");
    let (_, pro) = verified_pro(&app, &pool, "010-5000-0301").await;
    publish_required_module(&pool).await;
    quality::check_offer_gate(&state, pro).await.expect("default setting does not gate offers");
}

#[sqlx::test(migrations = "../../migrations")]
async fn required_module_blocks_offers_once_gate_is_on(pool: PgPool) {
    let app = common::app(pool.clone());
    let state = build_state(pool.clone(), test_config()).expect("state");
    let (_, pro) = verified_pro(&app, &pool, "010-5000-0302").await;
    let module = publish_required_module(&pool).await;
    let (admin, _) = staff_token(&pool, Role::StaffAdmin).await;
    set_training_gate(&app, &admin, true).await;

    let blocked = quality::check_offer_gate(&state, pro).await;
    assert!(
        matches!(blocked, Err(ApiError::Conflict("training_required"))),
        "no progress blocks: {blocked:?}"
    );

    sqlx::query("INSERT INTO training_progress (pro_id, module_id, completed_at) VALUES ($1, $2, now())")
        .bind(pro)
        .bind(module)
        .execute(&pool)
        .await
        .expect("progress");
    quality::check_offer_gate(&state, pro).await.expect("completed module lifts the block");
}

async fn set_quality_status(db: &PgPool, pro: Uuid, status: &str) {
    sqlx::query(
        "INSERT INTO partner_quality (pro_id, score, status, lift_pause, inputs, computed_at)
         VALUES ($1, 20, $2, FALSE, '{}'::jsonb, now())
         ON CONFLICT (pro_id) DO UPDATE SET status = EXCLUDED.status",
    )
    .bind(pro)
    .bind(status)
    .execute(db)
    .await
    .expect("quality row");
}

/// One accepted job for the first partner, and two more verified partners who match its prefs.
async fn job_with_two_candidates(app: &Router, pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    seed_catalog(pool).await;
    let c = customer(app, pool, "010-5000-0600").await;
    let (job, _) = book(app, &c, 10, None).await;
    let (owner_token, _) = verified_pro(app, pool, "010-5000-0601").await;
    let (_, first) = verified_pro(app, pool, "010-5000-0602").await;
    let (_, second) = verified_pro(app, pool, "010-5000-0603").await;
    let (st, body) = call(app, "POST", &format!("/v1/partner/jobs/{job}/accept"), Some(owner_token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    (job, first, second)
}

#[sqlx::test(migrations = "../../migrations")]
async fn paused_partner_is_never_chosen_by_reassign(pool: PgPool) {
    let app = common::app(pool.clone());
    let state = build_state(pool.clone(), test_config()).expect("state");
    let (job, paused, free) = job_with_two_candidates(&app, &pool).await;
    set_quality_status(&pool, paused, "paused").await;
    let next = noshow::reassign(&state, job, 0).await.expect("the free partner takes the job");
    assert_eq!(next, free, "a paused partner must not receive the reassigned job");
}

#[sqlx::test(migrations = "../../migrations")]
async fn reassign_reports_quality_pause_when_only_paused_partners_remain(pool: PgPool) {
    let app = common::app(pool.clone());
    let state = build_state(pool.clone(), test_config()).expect("state");
    let (job, first, second) = job_with_two_candidates(&app, &pool).await;
    set_quality_status(&pool, first, "paused").await;
    set_quality_status(&pool, second, "paused").await;
    let err = noshow::reassign(&state, job, 0).await.expect_err("both candidates are paused");
    assert!(matches!(err, ApiError::Conflict("quality_paused")), "staff see the reason: {err:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn reassign_skips_partner_missing_required_training_once_gate_is_on(pool: PgPool) {
    let app = common::app(pool.clone());
    let state = build_state(pool.clone(), test_config()).expect("state");
    let (job, first, second) = job_with_two_candidates(&app, &pool).await;
    let module = publish_required_module(&pool).await;
    let (admin, _) = staff_token(&pool, Role::StaffAdmin).await;
    set_training_gate(&app, &admin, true).await;
    sqlx::query("INSERT INTO training_progress (pro_id, module_id, completed_at) VALUES ($1, $2, now())")
        .bind(second)
        .bind(module)
        .execute(&pool)
        .await
        .expect("progress");
    let next = noshow::reassign(&state, job, 0).await.expect("the trained partner takes the job");
    assert_ne!(next, first, "a partner without the required module must not receive the job");
}

#[sqlx::test(migrations = "../../migrations")]
async fn reassign_reports_training_when_no_candidate_is_trained(pool: PgPool) {
    let app = common::app(pool.clone());
    let state = build_state(pool.clone(), test_config()).expect("state");
    let (job, _, _) = job_with_two_candidates(&app, &pool).await;
    publish_required_module(&pool).await;
    let (admin, _) = staff_token(&pool, Role::StaffAdmin).await;
    set_training_gate(&app, &admin, true).await;
    let err = noshow::reassign(&state, job, 0).await.expect_err("nobody has the module");
    assert!(matches!(err, ApiError::Conflict("training_required")), "staff see the reason: {err:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn quality_endpoints_return_score_status_components_and_tips(pool: PgPool) {
    let app = common::app(pool.clone());
    let state = build_state(pool.clone(), test_config()).expect("state");
    let (_, first, _) = job_with_two_candidates(&app, &pool).await;
    quality::recompute_pass(&state, Utc::now()).await.expect("recompute");
    let (admin, _) = staff_token(&pool, Role::StaffAdmin).await;

    let (st, body) = call(&app, "GET", &format!("/v1/admin/pros/{first}/quality"), Some(admin.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert!(body["score"].is_u64(), "owner sees the score: {body}");
    assert_eq!(body["status"], "ok");
    assert!(body["components"].is_object(), "components: {body}");
    assert!(body["tips"].is_array(), "tips: {body}");

    let (st, list) = call(&app, "GET", "/v1/admin/pros", Some(admin.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{list}");
    let row = list.as_array().expect("rows").iter().find(|r| r["id"] == first.to_string()).expect("pro row");
    assert_eq!(row["quality_score"], body["score"], "the list shows the same score");
    assert_eq!(row["quality_status"], "ok");

    let (pro_token, _) = login(&app, "010-5000-0602", "partner").await;
    let (st, own) = call(&app, "GET", "/v1/partner/quality", Some(pro_token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{own}");
    assert_eq!(own["score"], body["score"], "the partner card shows the same score");
}
