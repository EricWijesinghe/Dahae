// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Payment method catalogue: the methods customers can pay with, and the admin switch for each.

use super::audit;
use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    providers::Gateway,
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

/// Methods offered to customers, with the gateway each one goes to: switched on by an admin and served by a
/// gateway (`Integrations::gateway_for`: PortOne with its channel key, else Toss while offered). The dev fake
/// provider takes no keys, so every enabled method is offered there (a local demo can pay).
pub async fn offered(s: &AppState) -> ApiResult<Vec<(String, Gateway)>> {
    let on = enabled_codes(&s.db).await?;
    let integ = s.providers.integ();
    let dev = s.providers.dev();
    Ok(on
        .into_iter()
        .filter_map(|c| integ.gateway_for(&c).or(dev.then_some(Gateway::Portone)).map(|g| (c, g)))
        .collect())
}

/// A method's flow on its gateway. Toss saves cards only (자동결제, card only), so on Toss every other method is
/// a one-off checkout, and card is a saved-card tile only while some gateway can save cards.
pub fn flow(gw: Gateway, code: &str, catalogue_flow: &'static str, card_billing: bool) -> &'static str {
    match gw {
        Gateway::Portone => catalogue_flow,
        Gateway::TossPayments if code == "card" && card_billing => "billing",
        Gateway::TossPayments => "checkout",
    }
}

/// Rejects unknown, switched-off or unserved methods before any intent is created; returns the method's gateway.
pub async fn ensure_enabled(s: &AppState, code: &str) -> ApiResult<Gateway> {
    if !METHODS.iter().any(|m| m.0 == code) {
        return Err(ApiError::BadRequest("invalid_payment_method"));
    }
    offered(s)
        .await?
        .into_iter()
        .find_map(|(c, g)| (c == code).then_some(g))
        .ok_or(ApiError::BadRequest("payment_method_disabled"))
}

#[derive(Serialize)]
struct Method {
    code: &'static str,
    flow: &'static str,
    /// Where `POST /checkouts` sends this method right now. Informational: the intent decides the page.
    gateway: &'static str,
}

async fn enabled(State(s): State<AppState>) -> ApiResult<Json<Vec<Method>>> {
    let on = offered(&s).await?;
    let card_billing = s.providers.integ().billing_gateway().is_some();
    Ok(Json(
        METHODS
            .iter()
            .filter_map(|&(code, f)| {
                let gw = on.iter().find_map(|(c, g)| (c == code).then_some(*g))?;
                Some(Method {
                    code,
                    flow: flow(gw, code, f, card_billing),
                    gateway: gw.code(),
                })
            })
            .collect(),
    ))
}

#[derive(Serialize)]
struct AdminMethod {
    code: &'static str,
    flow: &'static str,
    /// Switched on by an admin.
    enabled: bool,
    /// Served by a gateway (or dev mode); without one the method is never shown even when enabled.
    configured: bool,
    /// The gateway that serves the method: `portone`, `tosspayments`, or null when none does.
    gateway: Option<&'static str>,
}

async fn admin_view(s: &AppState) -> ApiResult<Vec<AdminMethod>> {
    let on = enabled_codes(&s.db).await?;
    let integ = s.providers.integ();
    let dev = s.providers.dev();
    Ok(METHODS
        .iter()
        .map(|&(code, flow)| {
            let gateway = integ.gateway_for(code).map(Gateway::code);
            AdminMethod {
                code,
                flow,
                enabled: on.iter().any(|c| c == code),
                configured: dev || gateway.is_some(),
                gateway,
            }
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
    use super::{Gateway, channel_key, flow};
    use std::collections::HashMap;

    #[test]
    fn under_toss_only_card_with_billing_keys_is_saved() {
        assert_eq!(flow(Gateway::Portone, "kakaopay", "billing", false), "billing");
        assert_eq!(flow(Gateway::Portone, "transfer", "checkout", true), "checkout");
        assert_eq!(flow(Gateway::TossPayments, "card", "billing", true), "billing");
        assert_eq!(flow(Gateway::TossPayments, "card", "billing", false), "checkout");
        assert_eq!(flow(Gateway::TossPayments, "kakaopay", "billing", true), "checkout");
        assert_eq!(flow(Gateway::TossPayments, "transfer", "checkout", true), "checkout");
    }

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
