// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Operator settings that used to need the terminal or the database: the Toss payouts switch, business hours
//! (AI offline mode) and the store-build notice (what changed since the last store build).

use super::{audit, brand, business};
use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    integrations::{PayoutMode, TOSS_SWITCH_KEY},
    owner,
    state::AppState,
};
use axum::{Json, extract::State};
use chrono::{NaiveTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

const HOURS_KEY: &str = "business_hours";
const STORE_BUILD_KEY: &str = "store_build";
const MAX_BUILD_URL: usize = 500;

async fn load(db: &PgPool, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await
}

async fn save(db: &PgPool, key: &str, value: &Value) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()")
        .bind(key)
        .bind(value)
        .execute(db)
        .await
        .map(|_| ())
}

#[derive(Deserialize)]
pub(super) struct SwitchInput {
    on: bool,
}

/// `PUT /setup/payout/switch`: the owner's console switch for Toss payouts (the env `TOSS_PAYOUTS_ENABLED` still works).
/// Switching off while the payout card is in Toss mode would drop the card on reload, so the card goes manual first.
pub(super) async fn put_payout_switch(State(s): State<AppState>, owner::OwnerTotp(a): owner::OwnerTotp, Json(i): Json<SwitchInput>) -> ApiResult<Json<Value>> {
    let toss_mode = s.providers.integ().payout.as_ref().is_some_and(|p| p.mode == PayoutMode::Toss);
    if !i.on && toss_mode && !crate::integrations::toss_env_enabled() {
        return Err(ApiError::Conflict("payout_in_toss_mode"));
    }
    let value = json!({ "on": i.on, "by": a.id, "at": Utc::now().to_rfc3339() });
    save(&s.db, TOSS_SWITCH_KEY, &value).await?;
    audit(
        &s.db,
        a.role.as_str(),
        Some(a.id),
        "setup.payout_switch",
        TOSS_SWITCH_KEY,
        json!({ "on": i.on }),
    )
    .await?;
    super::reload(&s).await?;
    Ok(Json(value))
}

fn clock(v: &Value) -> Option<NaiveTime> {
    v.as_str().filter(|s| s.len() == 5).and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok())
}

/// `{"weekdays":[0..6] (0 = Monday, as ai.rs reads it), "open":"HH:MM", "close":"HH:MM"}`, open before close.
pub fn clean_hours(input: &Value) -> Result<Value, ApiError> {
    let bad = || ApiError::BadRequest("invalid_hours");
    let days = input["weekdays"].as_array().ok_or_else(bad)?;
    let mut weekdays = Vec::with_capacity(days.len());
    for d in days {
        let n = d.as_u64().filter(|n| *n <= 6).ok_or_else(bad)?;
        if !weekdays.contains(&n) {
            weekdays.push(n);
        }
    }
    weekdays.sort_unstable();
    let (open, close) = (clock(&input["open"]).ok_or_else(bad)?, clock(&input["close"]).ok_or_else(bad)?);
    if open >= close {
        return Err(bad());
    }
    Ok(json!({ "weekdays": weekdays, "open": open.format("%H:%M").to_string(), "close": close.format("%H:%M").to_string() }))
}

pub(super) async fn get_hours(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Value>> {
    Ok(Json(
        load(&s.db, HOURS_KEY)
            .await?
            .unwrap_or_else(|| json!({ "weekdays": [], "open": null, "close": null })),
    ))
}

pub(super) async fn put_hours(State(s): State<AppState>, Admin(a): Admin, Json(input): Json<Value>) -> ApiResult<Json<Value>> {
    let value = clean_hours(&input)?;
    save(&s.db, HOURS_KEY, &value).await?;
    audit(&s.db, a.role.as_str(), Some(a.id), "setup.business_hours", HOURS_KEY, value.clone()).await?;
    Ok(Json(value))
}

fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split(['/', ':']).next().unwrap_or_default().to_owned()
}

/// What a store build bakes in: name, icon, colour, app ids, link hosts, API base. Reads the `app_links` and
/// `domains` rows as JSON (no code dependency on their modules); missing rows count as empty.
async fn snapshot(s: &AppState) -> ApiResult<Value> {
    let b = business(&s.db).await?;
    let links = load(&s.db, "app_links").await?.unwrap_or_default();
    let domains = load(&s.db, "domains").await?.unwrap_or_default();
    let mut hosts = vec![host_of(&s.config.public_base_url)];
    let aliases = domains["aliases"].as_array().cloned().unwrap_or_default();
    hosts.extend(aliases.iter().filter_map(|a| a["host"].as_str().map(String::from)));
    Ok(json!({
        "name": brand::name_of(&b),
        "short_name": brand::short_of(&b),
        "store_icon": b["brand_assets"]["store_1024"],
        "primary_color": b["primary_color"],
        "app_ids": { "android": links["android_packages"], "ios": links["ios_bundle_ids"] },
        "app_link_hosts": hosts,
        "api_base": s.config.public_base_url,
    }))
}

/// Items that differ between the last acknowledged build and now, in the console's words.
pub fn changed(old: &Value, now: &Value) -> Vec<&'static str> {
    let differ = |keys: &[&str]| keys.iter().any(|k| old[*k] != now[*k]);
    [
        ("name", &["name", "short_name"][..]),
        ("icon", &["store_icon"][..]),
        ("color", &["primary_color"][..]),
        ("app_ids", &["app_ids"][..]),
        ("domain", &["app_link_hosts", "api_base"][..]),
    ]
    .into_iter()
    .filter(|(_, keys)| differ(keys))
    .map(|(item, _)| item)
    .collect()
}

/// `store_build` in `GET /setup`. The first read stores the current snapshot, so existing installs see no alarm.
pub(super) async fn store_build(s: &AppState) -> ApiResult<Value> {
    let now = snapshot(s).await?;
    let Some(row) = load(&s.db, STORE_BUILD_KEY).await? else {
        sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING")
            .bind(STORE_BUILD_KEY)
            .bind(json!({ "snapshot": now }))
            .execute(&s.db)
            .await?;
        return Ok(json!({ "changed": [], "acked_at": null, "build_url": null }));
    };
    Ok(json!({ "changed": changed(&row["snapshot"], &now), "acked_at": row["acked_at"], "build_url": row["build_url"] }))
}

#[derive(Deserialize)]
pub(super) struct AckInput {
    build_url: Option<String>,
}

/// `POST /setup/store-build/ack` (owner): the store build is done; today's values become the new baseline.
pub(super) async fn ack_store_build(State(s): State<AppState>, Admin(a): Admin, Json(i): Json<AckInput>) -> ApiResult<Json<Value>> {
    if !owner::is_owner(&s.db, a.id).await? {
        return Err(ApiError::Forbidden);
    }
    let url = i.build_url.map(|u| u.trim().to_owned()).filter(|u| !u.is_empty());
    if url
        .as_ref()
        .is_some_and(|u| !u.starts_with("https://") || u.len() > MAX_BUILD_URL || u.contains(char::is_whitespace))
    {
        return Err(ApiError::BadRequest("invalid_link"));
    }
    let value = json!({ "snapshot": snapshot(&s).await?, "acked_at": Utc::now().to_rfc3339(), "acked_by": a.id, "build_url": url });
    save(&s.db, STORE_BUILD_KEY, &value).await?;
    audit(&s.db, a.role.as_str(), Some(a.id), "setup.store_build_ack", STORE_BUILD_KEY, json!({})).await?;
    Ok(Json(json!({ "changed": [], "acked_at": value["acked_at"], "build_url": value["build_url"] })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_are_validated_and_normalised() {
        let v = clean_hours(&json!({"weekdays": [4, 0, 0, 2], "open": "09:00", "close": "18:30"})).unwrap();
        assert_eq!(v, json!({"weekdays": [0, 2, 4], "open": "09:00", "close": "18:30"}));
        let bad = |v: Value| clean_hours(&v).is_err();
        assert!(bad(json!({"weekdays": [7], "open": "09:00", "close": "18:00"})), "weekday out of range");
        assert!(bad(json!({"weekdays": [-1], "open": "09:00", "close": "18:00"})));
        assert!(bad(json!({"weekdays": [0], "open": "9:00", "close": "18:00"})), "HH:MM only");
        assert!(bad(json!({"weekdays": [0], "open": "18:00", "close": "09:00"})), "open before close");
        assert!(bad(json!({"weekdays": [0], "open": "25:00", "close": "26:00"})));
        assert!(bad(json!({"open": "09:00", "close": "18:00"})), "weekdays required");
    }

    #[test]
    fn store_build_diff_names_each_changed_item() {
        let old = json!({"name": "다해", "short_name": "다해", "store_icon": null, "primary_color": "#187A8F",
                         "app_ids": {"android": null, "ios": null}, "app_link_hosts": ["a.kr"], "api_base": "https://a.kr"});
        assert!(changed(&old, &old).is_empty());
        let mut now = old.clone();
        now["short_name"] = json!("반짝");
        now["store_icon"] = json!("/files/x.png");
        now["api_base"] = json!("https://b.kr");
        assert_eq!(changed(&old, &now), vec!["name", "icon", "domain"]);
        assert_eq!(host_of("https://b.kr:8443/x"), "b.kr");
    }
}
