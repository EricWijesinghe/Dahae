// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Payment method catalogue: the methods customers can pay with, and the admin switch for each.

use super::audit;
use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    state::AppState,
};
use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;

const SETTING: &str = "payment_methods";

/// (code, flow). `billing` methods can be saved for one-tap payment; `checkout` ones are one-off via the SDK.
pub const METHODS: [(&str, &str); 10] = [
    ("card", "billing"),
    ("kakaopay", "billing"),
    ("naverpay", "billing"),
    ("tosspay", "billing"),
    ("payco", "billing"),
    ("samsungpay", "checkout"),
    ("applepay", "checkout"),
    ("transfer", "checkout"),
    ("virtual_account", "checkout"),
    ("phone", "checkout"),
];

pub fn routes() -> Router<AppState> {
    Router::new().route("/payments/methods", get(enabled))
}

pub fn admin_routes() -> Router<AppState> {
    Router::new().route("/payment-methods", get(admin_list).put(admin_update))
}

/// Enabled codes from settings; every method is enabled until an admin changes the list.
pub async fn enabled_codes(db: &PgPool) -> ApiResult<Vec<String>> {
    let stored: Option<sqlx::types::Json<Vec<String>>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(SETTING)
        .fetch_optional(db)
        .await?;
    Ok(stored.map(|j| j.0).unwrap_or_else(|| METHODS.iter().map(|m| m.0.to_string()).collect()))
}

/// The PortOne channel key for a method; a blank value counts as not configured.
pub fn channel_key(channel_keys: &HashMap<String, String>, code: &str) -> Option<String> {
    channel_keys.get(code).map(|k| k.trim()).filter(|k| !k.is_empty()).map(str::to_string)
}

/// Methods offered to customers: switched on by an admin and, on the real PortOne gateway, given a channel key.
/// The dev fake provider takes no keys, so every enabled method is offered there (a local demo can pay).
pub async fn offered_codes(s: &AppState) -> ApiResult<Vec<String>> {
    let on = enabled_codes(&s.db).await?;
    if s.providers.dev() {
        return Ok(on);
    }
    let integ = s.providers.integ();
    Ok(on.into_iter().filter(|c| channel_key(&integ.portone.channel_keys, c).is_some()).collect())
}

/// Rejects unknown, switched-off or unconfigured methods before any intent is created.
pub async fn ensure_enabled(s: &AppState, code: &str) -> ApiResult<()> {
    if !METHODS.iter().any(|m| m.0 == code) {
        return Err(ApiError::BadRequest("invalid_payment_method"));
    }
    if !offered_codes(s).await?.iter().any(|c| c == code) {
        return Err(ApiError::BadRequest("payment_method_disabled"));
    }
    Ok(())
}

#[derive(Serialize)]
struct Method {
    code: &'static str,
    flow: &'static str,
}

async fn enabled(State(s): State<AppState>) -> ApiResult<Json<Vec<Method>>> {
    let on = offered_codes(&s).await?;
    Ok(Json(
        METHODS
            .iter()
            .filter(|(code, _)| on.iter().any(|c| c == code))
            .map(|&(code, flow)| Method { code, flow })
            .collect(),
    ))
}

#[derive(Serialize)]
struct AdminMethod {
    code: &'static str,
    flow: &'static str,
    /// Switched on by an admin.
    enabled: bool,
    /// Has a PortOne channel key; without one the method is never shown to customers even when enabled.
    configured: bool,
}

async fn admin_view(s: &AppState) -> ApiResult<Vec<AdminMethod>> {
    let on = enabled_codes(&s.db).await?;
    let fake = s.providers.dev();
    let keys = &s.providers.integ().portone.channel_keys;
    Ok(METHODS
        .iter()
        .map(|&(code, flow)| AdminMethod {
            code,
            flow,
            enabled: on.iter().any(|c| c == code),
            configured: fake || channel_key(keys, code).is_some(),
        })
        .collect())
}

async fn admin_list(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Vec<AdminMethod>>> {
    Ok(Json(admin_view(&s).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    enabled: Vec<String>,
}

async fn admin_update(State(s): State<AppState>, Admin(a): Admin, Json(u): Json<Update>) -> ApiResult<Json<Vec<AdminMethod>>> {
    if u.enabled.is_empty() {
        // Saving nothing would hide every payment method from customers.
        return Err(ApiError::BadRequest("payment_methods_empty"));
    }
    if u.enabled.iter().any(|c| !METHODS.iter().any(|m| m.0 == c)) {
        return Err(ApiError::BadRequest("invalid_payment_method"));
    }
    // Keep catalogue order and drop duplicates so the stored list is canonical.
    let codes: Vec<&str> = METHODS.iter().map(|m| m.0).filter(|c| u.enabled.iter().any(|e| e == c)).collect();
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = now()")
        .bind(SETTING)
        .bind(sqlx::types::Json(&codes))
        .execute(&s.db)
        .await?;
    audit(&s.db, "staff", Some(a.id), "payment_methods.update", SETTING, json!(codes)).await?;
    Ok(Json(admin_view(&s).await?))
}

#[cfg(test)]
mod tests {
    use super::channel_key;
    use std::collections::HashMap;

    fn keys(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn channel_key_is_trimmed_and_blank_counts_as_missing() {
        let map = keys(&[("card", " ch-card "), ("kakaopay", "  "), ("tosspay", "")]);
        assert_eq!(channel_key(&map, "card").as_deref(), Some("ch-card"));
        assert_eq!(channel_key(&map, "kakaopay"), None);
        assert_eq!(channel_key(&map, "tosspay"), None);
        assert_eq!(channel_key(&map, "naverpay"), None);
    }
}
