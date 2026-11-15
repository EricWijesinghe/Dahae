//! Training gate on job acceptance. Off by default, so a required module never blocks offers until the owner
//! turns `training_gate_enabled` on. When on, a published required module without progress blocks, and
//! completing it lifts the block. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, build_state, error::ApiError, quality};
use axum::{Router, http::StatusCode};
use common::{call, staff_token, test_config, verified_pro};
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
