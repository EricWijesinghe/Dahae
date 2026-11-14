//! Keys bundle for restores on a fresh server (`keys_bundle.rs`). Every request needs the owner and a TOTP code
//! in `X-Owner-Totp` (`owner::OwnerTotp`).

use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    keys_bundle::{self, BundleError, Keys},
    owner::OwnerTotp,
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::header,
    response::IntoResponse,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

/// Base64 of the largest bundle, plus the passphrase and JSON framing.
const VERIFY_BODY_LIMIT: usize = 8 * 1024;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/keys/status", get(status))
        .route("/keys/bundle", post(make_bundle))
        .route("/keys/verify", post(verify).layer(DefaultBodyLimit::max(VERIFY_BODY_LIMIT)))
}

fn bundle_failure(e: BundleError) -> ApiError {
    ApiError::Internal(format!("keys bundle: {e}"))
}

async fn status(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Value>> {
    let verified_at: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT verified_at FROM key_bundle_checks WHERE id = 1")
        .fetch_optional(&s.db)
        .await?
        .flatten();
    Ok(Json(json!({ "verified_at": verified_at })))
}

/// Seals the live keys under a new passphrase and returns both. The passphrase is never stored.
async fn make_bundle(State(s): State<AppState>, OwnerTotp(_): OwnerTotp) -> ApiResult<impl IntoResponse> {
    let keys = Keys::from_parts(s.config.data_key, &s.config.jwt_secret).map_err(bundle_failure)?;
    let passphrase = keys_bundle::new_passphrase();
    let sealed = keys_bundle::seal(&keys, &passphrase).map_err(bundle_failure)?;
    // The passphrase is shown only after the bundle has opened and matched the live keys.
    let opened = keys_bundle::open(&sealed, &passphrase).map_err(bundle_failure)?;
    if !opened.matches(&s.config.data_key, &s.config.jwt_secret) {
        return Err(ApiError::Internal("keys bundle self-test failed".into()));
    }
    let body = json!({
        "file_name": format!("dahae-keys-{}.mkk", Utc::now().format("%Y%m%d")),
        "bundle": STANDARD.encode(&sealed),
        "passphrase": passphrase,
    });
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(body)))
}

#[derive(Deserialize)]
struct VerifyInput {
    bundle: String,
    passphrase: String,
}

/// Upload test: a bundle that opens with the passphrase and matches the live keys sets `verified_at`.
async fn verify(State(s): State<AppState>, OwnerTotp(_): OwnerTotp, Json(req): Json<VerifyInput>) -> ApiResult<Json<Value>> {
    let raw = STANDARD.decode(req.bundle.trim()).map_err(|_| ApiError::BadRequest("keys_not_opened"))?;
    let opened = keys_bundle::open(&raw, &req.passphrase).map_err(|_| ApiError::BadRequest("keys_not_opened"))?;
    if !opened.matches(&s.config.data_key, &s.config.jwt_secret) {
        return Err(ApiError::BadRequest("keys_mismatch"));
    }
    let verified_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO key_bundle_checks (id, verified_at) VALUES (1, now())
         ON CONFLICT (id) DO UPDATE SET verified_at = excluded.verified_at
         RETURNING verified_at",
    )
    .fetch_one(&s.db)
    .await?;
    Ok(Json(json!({ "verified_at": verified_at })))
}
