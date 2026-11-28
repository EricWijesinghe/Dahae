// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The console's Legal inbox: each pack document in each language with its published versions and the pack's
//! current text as the draft. Nothing is published without the owner's click (D9); each publish adds an immutable
//! `legal_documents` row, and a version that asks for re-consent moves the consents behind that document
//! (`privacy::consent_versions`).

use super::{audit, setup::REAL_STAFF_EXISTS};
use crate::{
    auth::{Admin, live_admin},
    error::{ApiError, ApiResult},
    legal::{self, Template},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Written once with the baseline import; its presence means the import already ran on this database.
pub const BASELINE_MARKER: &str = "legal.baseline";

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/legal", get(inbox))
        .route("/legal/publish", post(publish))
        .route("/legal/versions/{id}", get(version))
}

#[derive(Serialize, sqlx::FromRow)]
struct VersionRow {
    id: i64,
    doc: String,
    lang: String,
    version: String,
    template_version: Option<String>,
    requires_reconsent: bool,
    effective_at: NaiveDate,
    published_at: DateTime<Utc>,
    published_by: Option<Uuid>,
}

/// Records one publish of `t` as rendered (`html`, from `values`).
async fn insert<'e>(db: impl PgExecutor<'e>, t: &Template, html: &str, values: &[(String, String)], by: Option<Uuid>) -> ApiResult<i64> {
    let slots: Map<String, Value> = values.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
    let id = sqlx::query_scalar(
        "INSERT INTO legal_documents (doc, lang, version, template_version, requires_reconsent, body_md, body_html, body_sha256,
                                      slots_json, effective_at, published_by)
         VALUES ($1, $2, $3, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(&t.doc)
    .bind(&t.lang)
    .bind(&t.version)
    .bind(t.requires_reconsent)
    .bind(&t.body)
    .bind(html)
    .bind(hex::encode(Sha256::digest(html.as_bytes())))
    .bind(Value::Object(slots))
    .bind(t.effective_from)
    .bind(by)
    .fetch_one(db)
    .await?;
    Ok(id)
}

/// Once per database: an install that was already running (it has real staff) has customers who agreed to the pack's
/// texts, so those are recorded as the first published versions, dated as the texts are. A fresh install imports
/// nothing and its owner publishes from the inbox. Returns how many rows were written.
pub async fn import_baseline(s: &AppState) -> ApiResult<usize> {
    let mut tx = s.db.begin().await?;
    let first: Option<String> = sqlx::query_scalar("INSERT INTO settings (key, value) VALUES ($1, '{}'::jsonb) ON CONFLICT (key) DO NOTHING RETURNING key")
        .bind(BASELINE_MARKER)
        .fetch_optional(&mut *tx)
        .await?;
    if first.is_none() || !sqlx::query_scalar::<_, bool>(REAL_STAFF_EXISTS).fetch_one(&mut *tx).await? {
        tx.commit().await?;
        return Ok(0);
    }
    let mut written = 0;
    for t in legal::active_templates(&s.pack)? {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM legal_documents WHERE doc = $1 AND lang = $2)")
            .bind(&t.doc)
            .bind(&t.lang)
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            let (html, values) = legal::render_live(s, &t).await?;
            insert(&mut *tx, &t, &html, &values, None).await?;
            written += 1;
        }
    }
    sqlx::query("INSERT INTO audit_log (actor_role, action, target, data) VALUES ('system', 'legal.baseline', 'legal_documents', $1)")
        .bind(json!({ "imported": written }))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(written)
}

async fn history(db: &PgPool) -> sqlx::Result<Vec<VersionRow>> {
    sqlx::query_as(
        "SELECT id, doc, lang, version, template_version, requires_reconsent, effective_at, published_at, published_by
         FROM legal_documents ORDER BY published_at DESC, id DESC",
    )
    .fetch_all(db)
    .await
}

async fn body_html(db: &PgPool, id: i64) -> sqlx::Result<String> {
    sqlx::query_scalar("SELECT body_html FROM legal_documents WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
}

/// `GET /admin/legal`: every pack document per language, newest version first. `pending` means the pack carries a
/// text that is not published yet (none published, or a newer template version); slot values alone never make it
/// pending, since the website and the draft read them live.
async fn inbox(State(s): State<AppState>, _a: Admin) -> ApiResult<Json<Value>> {
    let rows = history(&s.db).await?;
    let mut documents = Vec::new();
    for t in legal::active_templates(&s.pack)? {
        let mine: Vec<&VersionRow> = rows.iter().filter(|r| r.doc == t.doc && r.lang == t.lang).collect();
        let published = match mine.first() {
            Some(r) => Some(json!({ "id": r.id, "version": r.version, "effective_at": r.effective_at,
                                    "published_at": r.published_at, "body_html": body_html(&s.db, r.id).await? })),
            None => None,
        };
        let pending = mine.first().is_none_or(|r| r.template_version.as_deref() != Some(t.version.as_str()));
        let (draft_html, _) = legal::render_live(&s, &t).await?;
        documents.push(json!({
            "doc": t.doc, "lang": t.lang, "title": t.title, "pending": pending,
            "template": t, "draft_html": draft_html, "published": published, "versions": mine,
        }));
    }
    Ok(Json(json!({ "country": s.pack.id, "documents": documents })))
}

#[derive(Deserialize)]
struct PublishBody {
    doc: String,
    lang: String,
}

/// `POST /admin/legal/publish`: publishes the pack's current text of one document in one language, rendered with
/// this install's values now. Admin only, checked against the stored role; audited.
async fn publish(State(s): State<AppState>, Admin(a): Admin, Json(b): Json<PublishBody>) -> ApiResult<Json<Value>> {
    live_admin(&s, a.id).await?;
    let t = legal::active_templates(&s.pack)?
        .into_iter()
        .find(|t| t.doc == b.doc && t.lang == b.lang)
        .ok_or(ApiError::NotFound)?;
    let (html, values) = legal::render_live(&s, &t).await?;
    let id = insert(&s.db, &t, &html, &values, Some(a.id)).await?;
    let data = json!({ "id": id, "lang": t.lang, "version": t.version, "requires_reconsent": t.requires_reconsent });
    audit(&s.db, "staff", Some(a.id), "legal.publish", &t.doc, data).await?;
    Ok(Json(json!({ "id": id, "doc": t.doc, "lang": t.lang, "version": t.version })))
}

/// `GET /admin/legal/versions/{id}`: one published version with its text, for the inbox preview.
async fn version(State(s): State<AppState>, _a: Admin, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let row: VersionRow = sqlx::query_as(
        "SELECT id, doc, lang, version, template_version, requires_reconsent, effective_at, published_at, published_by
         FROM legal_documents WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let mut v = serde_json::to_value(&row).map_err(|e| ApiError::Internal(e.to_string()))?;
    v["body_html"] = Value::String(body_html(&s.db, id).await?);
    Ok(Json(v))
}
