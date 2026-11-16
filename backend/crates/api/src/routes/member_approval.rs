// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Platform owner approval for team members. A new invite waits as `pending_approval`; until the owner
//! approves it the member sees no jobs or addresses and cannot be assigned. The team lead cannot approve.

use super::audit;
use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde_json::{Value, json};
use uuid::Uuid;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/team-members/pending", get(list_pending))
        .route("/team-members/{id}/approve", post(approve))
}

async fn list_pending(State(s): State<AppState>, Admin(_a): Admin) -> ApiResult<Json<Value>> {
    let v: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object('id', m.id, 'pro_id', m.pro_id, 'team_name', p.team_name, 'name', m.name,
           'phone', m.phone, 'created_at', m.created_at) ORDER BY m.created_at), '[]')
         FROM team_members m JOIN pros p ON p.id = m.pro_id
         WHERE m.role = 'member' AND m.invite_status = 'pending_approval'",
    )
    .fetch_one(&s.db)
    .await?;
    Ok(Json(v))
}

async fn approve(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let mut tx = s.db.begin().await?;
    let r = sqlx::query("UPDATE team_members SET invite_status = 'active' WHERE id = $1 AND role = 'member' AND invite_status = 'pending_approval'")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    audit(&mut *tx, "staff", Some(a.id), "team_member.approve", &id.to_string(), json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
