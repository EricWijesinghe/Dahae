// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Owner-queued server jobs (`/admin/host/jobs`). An accepted request writes one job file into the agent's
//! inbox and records a row; the live state is read back from the agent's status files. A job the agent holds for
//! the server approval code (`domain-set`) gets its code through `approve`; a job that publishes a result
//! (`domain-check`, `host-approval-enroll`, `keys-backup`) is read back through `output`; the sealed key copy only once,
//! and it is recorded as kept off the server only when the owner says so after saving it (`kept`).

use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    host_jobs::{self, JobSpec, Kind},
    host_status::{self, HostStatus},
    idempotency, owner,
    sms_outbox::{self, Message},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::SocketAddr,
    path::Path as FsPath,
};
use uuid::Uuid;

const LIST_LIMIT: i64 = 50;
/// A job with no status file this long after it was queued never reached the agent.
const QUEUED_WINDOW_MIN: i64 = 10;
/// Results in `out/<token>` are root-written text; the console never needs more than this.
const MAX_OUTPUT_BYTES: u64 = 64 * 1024;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/host/jobs", idempotency::replayable(get(list).post(enqueue)))
        .route("/host/jobs/{id}", get(detail))
        .route("/host/jobs/{id}/approve", post(approve))
        .route("/host/jobs/{id}/output", get(output))
        .route("/host/jobs/{id}/kept", post(kept))
}

#[derive(Deserialize)]
struct ApproveBody {
    code: String,
}

#[derive(Deserialize)]
struct EnqueueBody {
    kind: String,
    #[serde(default)]
    arg: String,
}

#[derive(FromRow)]
struct JobRow {
    id: Uuid,
    kind: String,
    arg: String,
    progress_token: String,
    created_at: DateTime<Utc>,
}

async fn enqueue(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Admin(a): Admin,
    Json(body): Json<EnqueueBody>,
) -> ApiResult<impl IntoResponse> {
    let key = idempotency_key(&headers)?;
    let mut spec = host_jobs::validate(&body.kind, &body.arg).map_err(ApiError::BadRequest)?;
    if spec.kind == Kind::Edition && !host_jobs::edition_offered(&spec.arg, &s.config.editions_offered) {
        return Err(ApiError::BadRequest("edition_not_offered"));
    }
    if spec.kind == Kind::HostApprovalEnroll {
        spec.arg = a.id.hyphenated().to_string();
    }
    // Checked before the TOTP is spent, so a missing inbox does not burn the owner's code.
    let dir = jobs_dir(&s)?;
    if spec.kind.needs_owner() {
        owner::verify_owner(&s, a, &headers, Some(peer.ip()), true).await?;
    }
    let id = Uuid::now_v7();
    let token = host_jobs::new_progress_token();
    let created_at = queue(&s, dir, id, &spec, &token, &key, a.id).await?;
    if spec.kind.needs_owner() {
        notify_owner(&s, a.id, spec.kind).await;
    }
    let row = JobRow {
        id,
        kind: spec.kind.name().into(),
        arg: stored_arg(&spec).into(),
        progress_token: token,
        created_at,
    };
    Ok((StatusCode::ACCEPTED, Json(job_json(&dir_status(&s, id), &row))))
}

/// What the database keeps of the argument: nothing for a secret one (it lives in the job file until the agent reads it).
fn stored_arg(spec: &JobSpec) -> &str {
    if spec.kind.secret_arg() { "" } else { &spec.arg }
}

fn idempotency_key(headers: &HeaderMap) -> ApiResult<String> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .ok_or(ApiError::BadRequest("idempotency_key_required"))
}

/// Writes the row, its audit entry and the job file in one transaction. The file rename is the point of no
/// return: once it succeeds the job runs, so a commit error after it is logged, not returned to the owner.
async fn queue(s: &AppState, dir: &FsPath, id: Uuid, spec: &JobSpec, token: &str, key: &str, actor: Uuid) -> ApiResult<DateTime<Utc>> {
    let mut tx = s.db.begin().await?;
    let created_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO host_jobs (id, kind, arg, progress_token, requested_by, idempotency_key) VALUES ($1, $2, $3, $4, $5, $6) RETURNING created_at",
    )
    .bind(id)
    .bind(spec.kind.name())
    .bind(stored_arg(spec))
    .bind(token)
    .bind(actor)
    .bind(key)
    .fetch_one(&mut *tx)
    .await?;
    let data = json!({ "kind": spec.kind.name(), "arg": stored_arg(spec) });
    sqlx::query("INSERT INTO audit_log (actor_role, actor_id, action, target, data) VALUES ('staff', $1, 'host_job.enqueue', $2, $3)")
        .bind(actor)
        .bind(id.to_string())
        .bind(data)
        .execute(&mut *tx)
        .await?;
    if let Err(e) = write_job_file(dir, id, spec, token) {
        // Dropping the transaction rolls back the row and the audit entry: a job never queued leaves no trace.
        tracing::error!(error = %e, "host job file not written");
        return Err(ApiError::Unavailable("host_queue_unwritable"));
    }
    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, job = %id, "host job queued but its record was not saved");
    }
    Ok(created_at)
}

/// Texts the owner which job was queued. Best effort: the job is already queued, so a failure is only logged.
async fn notify_owner(s: &AppState, actor: Uuid, kind: Kind) {
    let phone = match owner_phone(&s.db, actor).await {
        Ok(Some(phone)) => phone,
        Ok(None) => {
            tracing::warn!("owner has no phone on file; job text skipped");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "owner phone not read; job text skipped");
            return;
        }
    };
    let text = format!("서버 작업이 요청됐어요: {}. 본인이 요청하지 않았다면 바로 확인하세요.", kind.label());
    if let Err(e) = sms_outbox::enqueue(s, None, &phone, "host_job.enqueue", Message::Sms(&text)).await {
        tracing::warn!(error = %e, "owner job text not queued");
    }
}

async fn owner_phone(db: &PgPool, actor: Uuid) -> sqlx::Result<Option<String>> {
    let row: Option<Option<String>> = sqlx::query_scalar("SELECT phone FROM staff WHERE id = $1")
        .bind(actor)
        .fetch_optional(db)
        .await?;
    Ok(row.flatten())
}

async fn list(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Value>> {
    let rows: Vec<JobRow> = sqlx::query_as("SELECT id, kind, arg, progress_token, created_at FROM host_jobs ORDER BY created_at DESC LIMIT $1")
        .bind(LIST_LIMIT)
        .fetch_all(&s.db)
        .await?;
    let jobs: Vec<Value> = rows.iter().map(|r| job_json(&dir_status(&s, r.id), r)).collect();
    // The console's edition card: shown only when the installer offered this server another edition.
    Ok(Json(
        json!({ "jobs": jobs, "edition": s.config.edition, "editions_offered": s.config.editions_offered }),
    ))
}

async fn detail(State(s): State<AppState>, Admin(_): Admin, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let row = job_row(&s.db, id).await?;
    Ok(Json(job_json(&dir_status(&s, id), &row)))
}

async fn job_row(db: &PgPool, id: Uuid) -> ApiResult<JobRow> {
    sqlx::query_as("SELECT id, kind, arg, progress_token, created_at FROM host_jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Owner only. Hands the server approval code to the agent as `inbox/<id>.approve` (`code=NNNNNN`, one line).
/// The agent checks the code, counts wrong ones and locks; the API only checks its shape.
async fn approve(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<Uuid>, Json(body): Json<ApproveBody>) -> ApiResult<StatusCode> {
    if !owner::is_owner(&s.db, a.id).await? {
        return Err(ApiError::Forbidden);
    }
    let code = body.code.trim();
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::BadRequest("invalid_approval_code"));
    }
    let row = job_row(&s.db, id).await?;
    let waiting = Kind::parse(&row.kind).is_some_and(Kind::needs_approval) && dir_status(&s, id).is_some_and(|st| st.state == "awaiting_approval");
    if !waiting {
        return Err(ApiError::Conflict("job_not_awaiting_approval"));
    }
    let dir = jobs_dir(&s)?;
    if let Err(e) = write_inbox(dir, &format!("{}.approve", id.hyphenated()), &format!("code={code}\n")) {
        tracing::error!(error = %e, "approval file not written");
        return Err(ApiError::Unavailable("host_queue_unwritable"));
    }
    crate::routes::audit(&s.db, "staff", Some(a.id), "host_job.approve", &id.to_string(), json!({ "kind": row.kind })).await?;
    Ok(StatusCode::ACCEPTED)
}

/// The job's result file (`out/<token>`), capped and read as untrusted text. The approval enrolment result holds
/// the new secret, so only the owner who asked for it may read it. 404 once the agent has pruned it.
async fn output(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let row = job_row(&s.db, id).await?;
    if row.kind == Kind::HostApprovalEnroll.name() && (row.arg != a.id.hyphenated().to_string() || !owner::is_owner(&s.db, a.id).await?) {
        return Err(ApiError::Forbidden);
    }
    if row.kind == Kind::KeysBackup.name() {
        return take_key_copy(&s, a.id, &row).await;
    }
    let text = read_output(&s.config.host_jobs_dir, &row.progress_token).ok_or(ApiError::NotFound)?;
    Ok(Json(json!({ "text": text })))
}

/// Only the owner who asked for this key copy may take it or say it is kept.
async fn key_copy_owner(s: &AppState, actor: Uuid, row: &JobRow) -> ApiResult<()> {
    let requester: Uuid = sqlx::query_scalar("SELECT requested_by FROM host_jobs WHERE id = $1")
        .bind(row.id)
        .fetch_one(&s.db)
        .await?;
    if requester != actor || !owner::is_owner(&s.db, actor).await? {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

/// The sealed key copy (base64), for the owner who asked for it and only once. The mark and its audit entry
/// commit together. Taking it records nothing about keeping it: the browser may not have saved the file.
async fn take_key_copy(s: &AppState, actor: Uuid, row: &JobRow) -> ApiResult<Json<Value>> {
    key_copy_owner(s, actor, row).await?;
    let text = read_output(&s.config.host_jobs_dir, &row.progress_token).ok_or(ApiError::NotFound)?;
    let mut tx = s.db.begin().await?;
    // The passphrase never reaches the arg column, so for this kind it holds the one-time mark.
    let taken = sqlx::query("UPDATE host_jobs SET arg = 'downloaded' WHERE id = $1 AND arg = ''")
        .bind(row.id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        == 1;
    if !taken {
        return Err(ApiError::Conflict("key_copy_already_taken"));
    }
    crate::routes::audit(&mut *tx, "staff", Some(actor), "host_job.keys_download", &row.id.to_string(), json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({ "text": text })))
}

/// Owner only, after the console saved the copy and the owner confirmed it is kept off the server. Records it
/// once (`downloaded` to `kept`) and queues the agent's `keys-confirm`, which `dahae status` reads.
async fn kept(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let row = job_row(&s.db, id).await?;
    if row.kind != Kind::KeysBackup.name() {
        return Err(ApiError::BadRequest("not_a_key_copy"));
    }
    key_copy_owner(&s, a.id, &row).await?;
    let dir = jobs_dir(&s)?;
    let mut tx = s.db.begin().await?;
    let marked = sqlx::query("UPDATE host_jobs SET arg = 'kept' WHERE id = $1 AND arg = 'downloaded'")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        == 1;
    if !marked {
        return Err(ApiError::Conflict("key_copy_not_downloaded"));
    }
    crate::routes::audit(&mut *tx, "staff", Some(a.id), "host_job.keys_kept", &id.to_string(), json!({})).await?;
    let confirm = JobSpec {
        kind: Kind::KeysConfirm,
        arg: String::new(),
    };
    // Queued before the mark commits: if it cannot be queued, the mark rolls back and the owner can try again.
    queue(
        &s,
        dir,
        Uuid::now_v7(),
        &confirm,
        &host_jobs::new_progress_token(),
        &format!("keys-confirm:{id}"),
        a.id,
    )
    .await?;
    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, job = %id, "key copy confirmation queued but its mark was not saved");
    }
    Ok(StatusCode::ACCEPTED)
}

fn read_output(dir: &FsPath, token: &str) -> Option<String> {
    if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(dir.join("out").join(token))
        .ok()?
        .take(MAX_OUTPUT_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn jobs_dir(s: &AppState) -> ApiResult<&FsPath> {
    let dir = s.config.host_jobs_dir.as_path();
    if dir.join("inbox").is_dir() {
        Ok(dir)
    } else {
        Err(ApiError::Unavailable("host_queue_missing"))
    }
}

fn dir_status(s: &AppState, id: Uuid) -> Option<HostStatus> {
    host_status::read(&s.config.host_jobs_dir, id)
}

fn job_json(status: &Option<HostStatus>, row: &JobRow) -> Value {
    json!({
        "id": row.id,
        "kind": row.kind,
        "arg": row.arg,
        "progress_token": row.progress_token,
        "created_at": row.created_at,
        "state": state_of(status, row.created_at),
        "step": status.as_ref().and_then(|st| st.step.clone()),
    })
}

/// No status file yet means queued, but only for a short window. Older than that, the agent never picked it up.
fn state_of(status: &Option<HostStatus>, created_at: DateTime<Utc>) -> &str {
    match status {
        Some(st) => st.state.as_str(),
        None if Utc::now() - created_at <= chrono::Duration::minutes(QUEUED_WINDOW_MIN) => "queued",
        None => "unknown",
    }
}

fn write_job_file(dir: &FsPath, id: Uuid, spec: &JobSpec, token: &str) -> std::io::Result<()> {
    write_inbox(dir, &format!("{}.job", id.hyphenated()), &host_jobs::job_file(id, spec, token))
}

/// Writes a uniquely named temp file exclusively, then renames it to `inbox/<name>`, so the agent (which reads
/// only `*.job` and `*.approve`) never sees a partial file.
fn write_inbox(dir: &FsPath, name: &str, body: &str) -> std::io::Result<()> {
    let inbox = dir.join("inbox");
    let tmp = inbox.join(format!("{name}.{}.tmp", &host_jobs::new_progress_token()[..8]));
    let result = (|| {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, inbox.join(name))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
