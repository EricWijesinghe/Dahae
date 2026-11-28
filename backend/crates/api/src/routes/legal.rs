// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The console's Legal inbox: each pack document in each language with its published versions and the pack's
//! current text as the draft. Nothing is published without the owner's click (D9): Publish is the owner's, with the
//! owner's code. Each publish adds an immutable `legal_documents` row with its own version (one version is one text),
//! the website and app show the newest text in force (`legal::published_in_place_of_pack`), and a version that asks
//! for re-consent moves the consents behind that document (`privacy::consent_versions`).

use super::{
    audit,
    seller::live::{invalidate_fragments, legal_override},
    setup::REAL_STAFF_EXISTS,
};
use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    legal::{self, Template, version_key},
    owner::OwnerTotp,
    privacy,
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
    custom: bool,
    body_sha256: String,
    effective_at: NaiveDate,
    published_at: DateTime<Utc>,
    published_by: Option<Uuid>,
    #[serde(skip)]
    slots_json: Value,
}

const COLUMNS: &str = "id, doc, lang, version, template_version, requires_reconsent, custom, body_sha256, effective_at, published_at, published_by, slots_json";

/// One text to record.
struct NewText<'a> {
    t: &'a Template,
    version: &'a str,
    template_version: Option<&'a str>,
    reconsent: bool,
    body_md: &'a str,
    html: &'a str,
    values: &'a [(String, String)],
    custom: bool,
}

fn sha(html: &str) -> String {
    hex::encode(Sha256::digest(html.as_bytes()))
}

async fn insert<'e>(db: impl PgExecutor<'e>, n: &NewText<'_>, by: Option<Uuid>) -> ApiResult<i64> {
    let slots: Map<String, Value> = n.values.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
    let id = sqlx::query_scalar(
        "INSERT INTO legal_documents (doc, lang, version, template_version, requires_reconsent, body_md, body_html, body_sha256,
                                      slots_json, custom, effective_at, published_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
    )
    .bind(&n.t.doc)
    .bind(&n.t.lang)
    .bind(n.version)
    .bind(n.template_version)
    .bind(n.reconsent)
    .bind(n.body_md)
    .bind(n.html)
    .bind(sha(n.html))
    .bind(Value::Object(slots))
    .bind(n.custom)
    .bind(n.t.effective_from)
    .bind(by)
    .fetch_one(db)
    .await?;
    Ok(id)
}

/// The owner's replacement of `t`'s page while one is live: then customers read it, not the pack text.
async fn custom_text(s: &AppState, t: &Template) -> ApiResult<Option<String>> {
    if t.lang != privacy::LEGAL_LANG {
        return Ok(None);
    }
    Ok(legal_override(&s.db, &legal::slug(&t.doc)).await?)
}

/// Once per database, over the pack's templates ([`import_baseline_from`]).
pub async fn import_baseline(s: &AppState) -> ApiResult<usize> {
    import_baseline_from(s, legal::active_templates(&s.pack)?).await
}

/// Once per database: an install that was already running (it has real staff) has customers who agreed to the texts
/// at [`privacy::CONSENT_VERSION`], so those are recorded as the first published versions, dated as the texts are. A
/// template at any other version (a release that changed it, booted by an install that skipped this one) was never
/// agreed to: it is not recorded and waits in the inbox for the owner. A page the owner replaced is recorded as that
/// text, marked custom, since it is what customers read. A fresh install imports nothing and its owner publishes from
/// the inbox. Returns how many rows were written.
pub async fn import_baseline_from(s: &AppState, templates: Vec<Template>) -> ApiResult<usize> {
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
    for t in templates.iter().filter(|t| t.version == privacy::CONSENT_VERSION) {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM legal_documents WHERE doc = $1 AND lang = $2)")
            .bind(&t.doc)
            .bind(&t.lang)
            .fetch_one(&mut *tx)
            .await?;
        if exists {
            continue;
        }
        let (html, values, custom) = match custom_text(s, t).await? {
            Some(html) => (html, Vec::new(), true),
            None => {
                let (html, values) = legal::render_live(s, t).await?;
                (html, values, false)
            }
        };
        let row = NewText {
            t,
            version: &t.version,
            template_version: (!custom).then_some(t.version.as_str()),
            reconsent: t.requires_reconsent && !custom,
            body_md: if custom { &html } else { &t.body },
            html: &html,
            values: &values,
            custom,
        };
        insert(&mut *tx, &row, None).await?;
        written += 1;
    }
    sqlx::query("INSERT INTO audit_log (actor_role, action, target, data) VALUES ('system', 'legal.baseline', 'legal_documents', $1)")
        .bind(json!({ "imported": written }))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(written)
}

async fn history(db: &PgPool) -> sqlx::Result<Vec<VersionRow>> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM legal_documents ORDER BY published_at DESC, id DESC"))
        .fetch_all(db)
        .await
}

async fn body_html(db: &PgPool, id: i64) -> sqlx::Result<String> {
    sqlx::query_scalar("SELECT body_html FROM legal_documents WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
}

/// The newest template version published from the pack (custom texts aside).
fn newest_template<'a>(mine: &[&'a VersionRow]) -> Option<&'a str> {
    mine.iter()
        .filter(|r| !r.custom)
        .filter_map(|r| r.template_version.as_deref())
        .max_by_key(|v| version_key(v))
}

/// The newest published text of template version `tv`.
fn latest_of_template<'a>(mine: &[&'a VersionRow], tv: &str) -> Option<&'a VersionRow> {
    mine.iter()
        .filter(|r| !r.custom && r.template_version.as_deref() == Some(tv))
        .max_by_key(|r| version_key(&r.version))
        .copied()
}

/// A pack text waits for Publish when none of the pack's is published, the pack carries a newer template than the
/// newest published (never an older one, as after a rollback), or this install's values (company fields, processors)
/// changed the text of the published template.
fn is_pending(t: &Template, mine: &[&VersionRow], live_sha: &str) -> bool {
    match newest_template(mine) {
        None => true,
        Some(v) if version_key(&t.version) > version_key(v) => true,
        Some(v) if v == t.version => latest_of_template(mine, v).is_some_and(|r| r.body_sha256 != live_sha),
        Some(_) => false,
    }
}

/// `GET /admin/legal`: every pack document per language, newest version first. `custom` marks a page the owner
/// replaced (it is what customers read, so the pack text is not offered for it).
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
        let custom = custom_text(&s, &t).await?.is_some();
        let (draft_html, _) = legal::render_live(&s, &t).await?;
        let pending = !custom && is_pending(&t, &mine, &sha(&draft_html));
        documents.push(json!({
            "doc": t.doc, "lang": t.lang, "title": t.title, "pending": pending, "custom": custom,
            "template": t, "draft_html": draft_html, "published": published, "versions": mine,
        }));
    }
    Ok(Json(json!({ "country": s.pack.id, "documents": documents })))
}

/// Whether the processor rows (`seller.uses_*`: who receives customer data, some of them abroad) differ from those a
/// published text was rendered with.
fn processors_changed(before: &Value, values: &[(String, String)]) -> bool {
    values
        .iter()
        .filter(|(k, _)| k.starts_with("seller.uses_"))
        .any(|(k, v)| before.get(k).and_then(Value::as_str) != Some(v.as_str()))
}

/// The version a publish of `t` (rendered now as `body_sha`) records, and whether it asks again. Refused when the
/// pack's template is older than one already published (a rollback) or the text is already the newest of its
/// template. A version already used (this install's values changed the text, or a custom text holds it) gets the
/// next counter, `YYYY-MM-DD.N`, which asks again only when the processor rows changed.
fn next_version(t: &Template, mine: &[&VersionRow], body_sha: &str, values: &[(String, String)]) -> ApiResult<(String, bool)> {
    if newest_template(mine).is_some_and(|v| version_key(&t.version) < version_key(v)) {
        return Err(ApiError::Conflict("legal_version_older"));
    }
    let same = latest_of_template(mine, &t.version);
    if same.is_some_and(|r| r.body_sha256 == body_sha) {
        return Err(ApiError::Conflict("legal_already_published"));
    }
    if !mine.iter().any(|r| r.version == t.version) {
        return Ok((t.version.clone(), t.requires_reconsent));
    }
    let (date, _) = version_key(&t.version).ok_or_else(|| ApiError::Internal(format!("legal template {} version", t.doc)))?;
    let n = mine
        .iter()
        .filter_map(|r| version_key(&r.version))
        .filter(|(d, _)| *d == date)
        .map(|(_, n)| n)
        .max()
        .unwrap_or(0);
    let reconsent = same.map_or(t.requires_reconsent, |r| processors_changed(&r.slots_json, values));
    Ok((format!("{}.{}", date.format("%Y-%m-%d"), n + 1), reconsent))
}

#[derive(Deserialize)]
struct PublishBody {
    doc: String,
    lang: String,
}

/// `POST /admin/legal/publish`: publishes the pack's current text of one document in one language, rendered with
/// this install's values now. The owner only, with the owner's code (it can withdraw every customer's optional
/// consents); the row and its audit entry commit together.
async fn publish(State(s): State<AppState>, OwnerTotp(a): OwnerTotp, Json(b): Json<PublishBody>) -> ApiResult<Json<Value>> {
    let t = legal::active_templates(&s.pack)?
        .into_iter()
        .find(|t| t.doc == b.doc && t.lang == b.lang)
        .ok_or(ApiError::NotFound)?;
    if custom_text(&s, &t).await?.is_some() {
        return Err(ApiError::Conflict("legal_custom_text"));
    }
    let (html, values) = legal::render_live(&s, &t).await?;
    let body_sha = sha(&html);
    let mut tx = s.db.begin().await?;
    // One publish at a time, so two clicks never record one version twice.
    sqlx::query("LOCK TABLE legal_documents IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    let rows: Vec<VersionRow> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM legal_documents WHERE doc = $1 AND lang = $2"))
        .bind(&t.doc)
        .bind(&t.lang)
        .fetch_all(&mut *tx)
        .await?;
    let (version, reconsent) = next_version(&t, &rows.iter().collect::<Vec<_>>(), &body_sha, &values)?;
    let row = NewText {
        t: &t,
        version: &version,
        template_version: Some(&t.version),
        reconsent,
        body_md: &t.body,
        html: &html,
        values: &values,
        custom: false,
    };
    let id = insert(&mut *tx, &row, Some(a.id)).await?;
    let data = json!({ "id": id, "lang": t.lang, "version": version, "template_version": t.version,
                       "requires_reconsent": reconsent, "body_sha256": body_sha });
    audit(&mut *tx, "staff", Some(a.id), "legal.publish", &t.doc, data).await?;
    tx.commit().await?;
    invalidate_fragments();
    Ok(Json(
        json!({ "id": id, "doc": t.doc, "lang": t.lang, "version": version, "requires_reconsent": reconsent }),
    ))
}

/// `GET /admin/legal/versions/{id}`: one published version with its text, for the inbox preview.
async fn version(State(s): State<AppState>, _a: Admin, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let row: VersionRow = sqlx::query_as(&format!("SELECT {COLUMNS} FROM legal_documents WHERE id = $1"))
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut v = serde_json::to_value(&row).map_err(|e| ApiError::Internal(e.to_string()))?;
    v["body_html"] = Value::String(body_html(&s.db, id).await?);
    Ok(Json(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processor_rows_decide_whether_a_republish_asks_again() {
        let before = json!({ "seller.uses_ai": "미사용", "seller.legal_name": "A" });
        let values = |ai: &str, name: &str| {
            vec![
                ("seller.uses_ai".to_string(), ai.to_string()),
                ("seller.legal_name".to_string(), name.to_string()),
            ]
        };
        assert!(!processors_changed(&before, &values("미사용", "B")), "a company field alone does not");
        assert!(processors_changed(&before, &values("사용", "A")), "a new recipient of customer data does");
        assert!(processors_changed(&Value::Null, &values("미사용", "A")), "nothing recorded to compare with");
    }
}
