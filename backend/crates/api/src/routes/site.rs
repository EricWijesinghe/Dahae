// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! On-site steps for a partner: check-in and check-out (times are staff-visible), and the door/entry
//! code, revealed on the visit day only. Every reveal is audited.

use super::{audit, bookings::load_booking};
use crate::{
    auth::{Pro, Staff},
    crypto,
    error::{ApiError, ApiResult},
    realtime::{Recipient, notify},
    safety,
    state::AppState,
    util::{best_effort, today_kst},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use domain::booking::BookingStatus;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

pub fn partner_routes() -> Router<AppState> {
    Router::new()
        .route("/jobs/{id}/check-in", post(check_in))
        .route("/jobs/{id}/check-out", post(check_out))
        .route("/jobs/{id}/entry-code", get(partner_entry_code))
}

pub fn admin_routes() -> Router<AppState> {
    Router::new().route("/bookings/{id}/entry-code", get(staff_entry_code))
}

/// The partner's own job that is still open (assigned, or completed), or 404 for anyone else's.
async fn own_active_job(s: &AppState, pro: Uuid, id: Uuid) -> ApiResult<super::bookings::BookingRow> {
    let b = load_booking(&s.db, id).await?;
    if b.pro_id != Some(pro) {
        return Err(ApiError::NotFound);
    }
    if !matches!(b.status()?, BookingStatus::TeamAssigned | BookingStatus::Completed) {
        return Err(ApiError::Conflict("booking_closed"));
    }
    Ok(b)
}

/// Arrival and departure only on an assigned job, on its visit day (KST).
fn ensure_on_site_today(b: &super::bookings::BookingRow) -> ApiResult<()> {
    if b.status()? != BookingStatus::TeamAssigned {
        return Err(ApiError::Conflict("booking_closed"));
    }
    if b.visit_date != today_kst() {
        return Err(ApiError::Conflict("not_visit_day"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub enum Stamp {
    Arrival,
    Departure,
}

/// Stamps the arrival or departure of booking `id`, only while it is still `pro`'s and assigned. The caller's
/// snapshot can be stale: a reassignment or cancellation may commit between its read and this write.
pub async fn stamp_visit(db: &PgPool, id: Uuid, pro: Uuid, stamp: Stamp) -> ApiResult<()> {
    let column = match stamp {
        Stamp::Arrival => "checked_in_at",
        Stamp::Departure => "checked_out_at",
    };
    let sql = format!("UPDATE bookings SET {column} = COALESCE({column}, now()), updated_at = now() WHERE id = $1 AND pro_id = $2 AND status = $3");
    let done = sqlx::query(&sql)
        .bind(id)
        .bind(pro)
        .bind(BookingStatus::TeamAssigned.as_i16())
        .execute(db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::Conflict("booking_changed"));
    }
    Ok(())
}

/// Records the arrival. Shared by the owner and the team member; the caller has checked ownership.
pub async fn check_in_job(s: &AppState, b: &super::bookings::BookingRow) -> ApiResult<Json<Value>> {
    ensure_on_site_today(b)?;
    stamp_visit(&s.db, b.id, b.pro_id.ok_or(ApiError::NotFound)?, Stamp::Arrival).await?;
    best_effort(
        "check-in: customer",
        notify(
            s,
            Recipient::User(b.user_id),
            "booking.arrival",
            "청소팀이 도착했어요",
            &b.number,
            json!({ "booking_id": b.id }),
        ),
    )
    .await;
    s.hub.send(Recipient::Staff, "booking.updated", json!({ "booking_id": b.id }));
    s.hub.send(Recipient::User(b.user_id), "booking.updated", json!({ "booking_id": b.id }));
    Ok(Json(json!({ "ok": true, "booking_id": b.id })))
}

/// Records the departure. Shared by the owner and the team member; the caller has checked ownership.
pub async fn check_out_job(s: &AppState, b: &super::bookings::BookingRow) -> ApiResult<Json<Value>> {
    ensure_on_site_today(b)?;
    if b.checked_in_at.is_none() {
        return Err(ApiError::Conflict("not_checked_in"));
    }
    stamp_visit(&s.db, b.id, b.pro_id.ok_or(ApiError::NotFound)?, Stamp::Departure).await?;
    s.hub.send(Recipient::Staff, "booking.updated", json!({ "booking_id": b.id }));
    s.hub.send(Recipient::User(b.user_id), "booking.updated", json!({ "booking_id": b.id }));
    Ok(Json(json!({ "ok": true, "booking_id": b.id })))
}

async fn check_in(State(s): State<AppState>, Pro(a): Pro, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let b = own_active_job(&s, a.id, id).await?;
    check_in_job(&s, &b).await
}

async fn check_out(State(s): State<AppState>, Pro(a): Pro, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let b = own_active_job(&s, a.id, id).await?;
    check_out_job(&s, &b).await
}

async fn stored_code(s: &AppState, id: Uuid) -> ApiResult<Option<String>> {
    let enc: Option<String> = sqlx::query_scalar("SELECT access_code_enc FROM bookings WHERE id = $1")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    Ok(enc.and_then(|e| crypto::decrypt(&s.config.data_key, crypto::aad::ACCESS_CODE, &e)))
}

/// Assigned partner only, and only on the visit day (KST) while the job is active.
async fn partner_entry_code(State(s): State<AppState>, Pro(a): Pro, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let b = own_active_job(&s, a.id, id).await?;
    if !matches!(b.status()?, BookingStatus::TeamAssigned | BookingStatus::Completed) || !safety::reveal_allowed(b.visit_date, today_kst()) {
        return Err(ApiError::Conflict("entry_code_not_today"));
    }
    audit(&s.db, "pro", Some(a.id), "entry_code.reveal", &id.to_string(), json!({})).await?;
    Ok(Json(json!({ "entry_code": stored_code(&s, id).await? })))
}

/// Staff reveal any time, always audited with the staff member.
async fn staff_entry_code(State(s): State<AppState>, Staff(st): Staff, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    load_booking(&s.db, id).await?;
    audit(&s.db, "staff", Some(st.id), "entry_code.reveal", &id.to_string(), json!({})).await?;
    Ok(Json(json!({ "entry_code": stored_code(&s, id).await? })))
}
