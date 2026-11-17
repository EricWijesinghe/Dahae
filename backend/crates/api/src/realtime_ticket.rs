// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Socket tickets: the WebSocket is opened with a 30-second, single-use ticket instead of the access token,
//! so the long-lived credential never appears in a URL (proxies and access logs record those).

use crate::{
    auth::{Auth, AuthEpoch, Role, new_refresh_token, sha256_hex, verify_access_live_epoch},
    error::{ApiError, ApiResult},
    state::AppState,
};
use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

pub const TICKET_TTL: Duration = Duration::from_secs(30);

/// Query of `GET /v1/ws`. A socket authenticates with `ticket`; `token` is the old form.
#[derive(Deserialize)]
pub struct WsQuery {
    ticket: Option<String>,
    /// shortcut: the access token in the URL is kept one release for apps installed before tickets; remove this field and its branch in `authenticate_socket` then.
    token: Option<String>,
}

/// `POST /v1/realtime/ticket`: a fresh ticket bound to the caller's id, role and session epoch.
pub async fn issue_ticket(State(state): State<AppState>, AuthEpoch(auth, epoch): AuthEpoch) -> ApiResult<Json<Value>> {
    // The same 256-bit opaque form as refresh tokens; only the hash is kept.
    let (ticket, hash) = new_refresh_token();
    sqlx::query("INSERT INTO realtime_tickets (token_hash, subject, role, expires_at, epoch) VALUES ($1, $2, $3, now() + make_interval(secs => $4), $5)")
        .bind(hash)
        .bind(auth.id)
        .bind(auth.role.as_str())
        .bind(TICKET_TTL.as_secs_f64())
        .bind(epoch)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ticket": ticket, "expires_in": TICKET_TTL.as_secs() })))
}

/// Spends a ticket. The row is deleted in the same statement that reads it, so a second use finds nothing.
pub async fn consume_ticket(db: &PgPool, ticket: &str) -> ApiResult<Auth> {
    redeem(db, ticket).await.map(|(auth, _)| auth)
}

/// Spends a ticket and returns its owner and session epoch. A staff ticket opens a socket only while the account
/// is active and the session it was issued in is still current (no sign-out everywhere since); any other ticket only
/// while its owner still has a signed-in device.
async fn redeem(db: &PgPool, ticket: &str) -> ApiResult<(Auth, i32)> {
    let row: Option<(Uuid, String, Option<i32>)> =
        sqlx::query_as("DELETE FROM realtime_tickets WHERE token_hash = $1 AND expires_at > now() RETURNING subject, role, epoch")
            .bind(sha256_hex(ticket.as_bytes()))
            .fetch_optional(db)
            .await?;
    let (id, role, epoch) = row.ok_or(ApiError::Unauthorized)?;
    let role = Role::parse(&role).ok_or(ApiError::Unauthorized)?;
    // A staff ticket without an epoch (issued before the column existed) is refused.
    if role.is_staff() {
        let live: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM staff WHERE id = $1 AND active AND session_epoch = $2)")
            .bind(id)
            .bind(epoch)
            .fetch_one(db)
            .await?;
        if !live {
            return Err(ApiError::Unauthorized);
        }
    } else if !has_signed_in_device(db, id).await? {
        return Err(ApiError::Unauthorized);
    }
    Ok((Auth { id, role }, epoch.unwrap_or(0)))
}

/// True while `subject` holds a live refresh token. Sign-out everywhere, account deletion, partner deletion and
/// team-member removal revoke them all, which is how a customer, partner or member socket learns its session ended.
pub async fn has_signed_in_device(db: &PgPool, subject: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM refresh_tokens WHERE subject = $1 AND revoked_at IS NULL AND rotated_at IS NULL AND expires_at > now())")
        .bind(subject)
        .fetch_one(db)
        .await
}

/// Identity of a socket opening and its session epoch, from its ticket (or, deprecated, its access token).
pub async fn authenticate_socket(state: &AppState, q: &WsQuery) -> ApiResult<(Auth, i32)> {
    if let Some(ticket) = q.ticket.as_deref() {
        return redeem(&state.db, ticket).await;
    }
    let token = q.token.as_deref().ok_or(ApiError::Unauthorized)?;
    tracing::warn!("realtime socket opened with the deprecated ?token= form; clients should send ?ticket=");
    // The epoch the token was checked at, not a second read: a sign-out landing in between must still end it.
    verify_access_live_epoch(&state.db, &state.config.jwt_keys(), token).await
}
