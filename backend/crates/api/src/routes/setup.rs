// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Setup wizard API: first admin in the browser (one-time setup code), business profile, and the
//! integration cards (save, clear, test connection). The console's wizard is the only client.

use super::audit;
use super::uploads;
use crate::entropy;
use crate::{
    address,
    auth::{Admin, Auth, hash_password},
    backup,
    error::{ApiError, ApiResult},
    integrations::{self, Invalid, Kind, LockState, Section},
    owner,
    providers::{Gateway, fcm, portone, sms, toss_payments},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    routing::{get, post, put},
};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use std::net::{IpAddr, SocketAddr};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub mod brand;
mod operator;

const MAX_FIELD_CHARS: usize = 200;
const SETUP_CODE_LEN: usize = 12;
/// No 0/O/1/I/L: the code is read off a screen and typed by hand.
const SETUP_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const SETUP_CODE_KEY: &str = "setup_code";
/// The installer's setup code works for seven days after it is issued: owners often finish DNS and payment signups later.
const SETUP_CODE_TTL_SECS: i64 = 7 * 24 * 60 * 60;
const BUSINESS_KEY: &str = "business";
/// An owner unlock lets one save or clear through, then the card locks again.
const UNLOCK_TTL_SECS: i64 = 5 * 60;
const REVIEW_KEY: &str = "review_account";
/// App Review needs days, not hours; the account switches itself off after this.
const REVIEW_MAX_DAYS: i64 = 30;
const REVIEW_CODE_LEN: usize = 6;
/// Name of the customer row the review login seeds.
const REVIEW_NAME: &str = "리뷰 계정";
/// Gangnam-gu, where the sample address sits.
const REVIEW_DISTRICT: &str = "11680";

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/setup", get(state))
        .route("/setup/admin", post(create_admin))
        .route("/brand", get(brand::get_brand))
        .route("/brand/manifest/{app}", get(brand::get_manifest))
}

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/setup", get(overview))
        .route("/setup/business", put(put_business))
        .route("/setup/business-hours", get(operator::get_hours).put(operator::put_hours))
        .route("/setup/payout/switch", put(operator::put_payout_switch))
        .route("/setup/store-build/ack", post(operator::ack_store_build))
        .route("/setup/review", put(put_review).delete(clear_review))
        .route("/setup/{section}", put(put_section).delete(clear_section))
        .route("/setup/{section}/test", post(test_section))
        .route("/setup/{section}/unlock", post(unlock_section))
        .route("/setup/{section}/lock", post(lock_section))
}

async fn has_staff(db: &PgPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM staff)").fetch_one(db).await
}

fn random_code() -> String {
    (0..SETUP_CODE_LEN)
        .map(|_| SETUP_ALPHABET[entropy::below(SETUP_ALPHABET.len())] as char)
        .collect()
}

/// Whether a setup code issued at `issued_at` (unix seconds) is past its window. A code with no
/// issue time counts as expired, so a row written before expiry existed cannot be used.
fn setup_code_expired(issued_at: Option<i64>, now: i64) -> bool {
    issued_at.is_none_or(|at| now - at > SETUP_CODE_TTL_SECS)
}

/// The code and issue time from the stored settings row, if it has a code.
fn parse_setup_code(saved: Option<Value>) -> Option<(String, Option<i64>)> {
    saved.and_then(|v| Some((v["code"].as_str()?.to_string(), v["at"].as_i64())))
}

async fn saved_setup_code(db: &PgPool) -> Result<Option<(String, Option<i64>)>, sqlx::Error> {
    let saved: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(SETUP_CODE_KEY)
        .fetch_optional(db)
        .await?;
    Ok(parse_setup_code(saved))
}

/// Picks the code to use and its issue time. The installer's code wins (issued at `issued_at`,
/// or when it was first stored). Otherwise the stored code is kept until it expires, then a new one is made.
fn choose_setup_code(from_env: Option<&str>, issued_at: Option<i64>, saved: Option<(String, Option<i64>)>, now: i64) -> (String, i64) {
    match (from_env, saved) {
        (Some(env_code), saved) => {
            let stored_at = saved.filter(|(c, _)| c == env_code).and_then(|(_, at)| at);
            (env_code.to_string(), issued_at.or(stored_at).unwrap_or(now))
        }
        (None, Some((code, Some(at)))) if !setup_code_expired(Some(at), now) => (code, at),
        (None, _) => (random_code(), now),
    }
}

/// On a fresh install (no staff yet) makes sure a setup code exists and prints it, so the person
/// installing can open the console and create the owner's account. `SETUP_CODE` (set by the
/// installer, issued at `issued_at`) wins; otherwise a random code is kept in the database across restarts.
pub async fn ensure_setup_code(db: &PgPool, from_env: Option<&str>, issued_at: Option<i64>) -> Result<(), sqlx::Error> {
    if has_staff(db).await? {
        return Ok(());
    }
    let saved = saved_setup_code(db).await?;
    let (code, at) = choose_setup_code(from_env, issued_at, saved, Utc::now().timestamp());
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()")
        .bind(SETUP_CODE_KEY)
        .bind(json!({ "code": code, "at": at }))
        .execute(db)
        .await?;
    // Printed to stdout, not the structured log, so the code never lands in log shipping or search.
    println!("first-run setup: open the console and enter setup code {code}");
    tracing::warn!("first-run setup code issued; read it from the startup output");
    Ok(())
}

/// A wrong code gets 403. The right code past its seven days gets 410, so the owner is told to ask
/// for a new one instead of retyping; that reveals nothing an attacker did not already need.
fn check_setup_code(expected: &str, issued_at: Option<i64>, given: &str, now: i64) -> ApiResult<()> {
    let matches = !expected.is_empty() && bool::from(given.as_bytes().ct_eq(expected.as_bytes()));
    if !matches {
        return Err(ApiError::Forbidden);
    }
    if setup_code_expired(issued_at, now) {
        return Err(ApiError::SetupCodeExpired);
    }
    Ok(())
}

/// Public: whether the console should show "create the owner account" instead of login.
async fn state(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "needs_admin": !has_staff(&s.db).await?, "dev_mode": s.config.dev_mode })))
}

#[derive(Deserialize)]
struct NewAdmin {
    code: String,
    email: String,
    name: String,
    password: String,
}

async fn create_admin(State(s): State<AppState>, Json(req): Json<NewAdmin>) -> ApiResult<Json<Value>> {
    let email = req.email.trim().to_lowercase();
    let name = req.name.trim();
    if !email.contains('@') || email.len() > MAX_FIELD_CHARS || name.is_empty() || name.chars().count() > MAX_FIELD_CHARS {
        return Err(ApiError::BadRequest("invalid_input"));
    }
    crate::auth::check_password_policy(&req.password).map_err(ApiError::BadRequest)?;
    let hash = hash_password(&req.password)?;
    let mut tx = s.db.begin().await?;
    // Serialises concurrent attempts: only the first one can create the owner.
    sqlx::query("LOCK TABLE staff IN EXCLUSIVE MODE").execute(&mut *tx).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM staff)").fetch_one(&mut *tx).await?;
    if exists {
        return Err(ApiError::Conflict("already_set_up"));
    }
    let saved: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(SETUP_CODE_KEY)
        .fetch_optional(&mut *tx)
        .await?;
    let (expected, issued_at) = parse_setup_code(saved).unwrap_or_default();
    let given = req.code.trim().to_uppercase().replace(['-', ' '], "");
    check_setup_code(&expected, issued_at, &given, Utc::now().timestamp())?;
    // The first admin is the owner. Set here explicitly: the migration never picks one.
    let id: uuid::Uuid = sqlx::query_scalar("INSERT INTO staff (email, name, password_hash, role, is_owner) VALUES ($1, $2, $3, 'admin', TRUE) RETURNING id")
        .bind(&email)
        .bind(name)
        .bind(hash)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM settings WHERE key = $1")
        .bind(SETUP_CODE_KEY)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit(&s.db, "staff_admin", Some(id), "setup.create_admin", &email, json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

/// The review account while it is switched on (set and not yet expired).
async fn active_review(db: &PgPool) -> Result<Option<Value>, sqlx::Error> {
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(REVIEW_KEY)
        .fetch_optional(db)
        .await?;
    Ok(v.filter(|v| {
        v["expires_at"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .is_some_and(|t| t > Utc::now())
    }))
}

/// Store-review demo login: one phone number whose sign-in code is fixed by the owner, so app
/// reviewers abroad can log in without a Korean SMS. Returns the stored code hash while active.
/// The code still goes through `otp_codes`, so the normal attempt and hourly limits apply.
async fn review_code_hash(db: &PgPool, phone: &str) -> Result<Option<String>, sqlx::Error> {
    let Some(v) = active_review(db).await? else { return Ok(None) };
    let same = v["phone"].as_str() == Some(phone);
    Ok(same.then(|| v["code_hash"].as_str().map(String::from)).flatten())
}

/// The fixed code to store for a code request on `phone`: only while no pro, team member or staff member holds the
/// number, so a person added on it later gets a real SMS code. No real customer can hold it while the review is on:
/// a sign-up there lands on the review row, and binding the number needs a code it never gets (`consume_otp`).
pub async fn review_code_for_request(db: &PgPool, phone: &str) -> Result<Option<String>, sqlx::Error> {
    let Some(hash) = review_code_hash(db, phone).await? else { return Ok(None) };
    Ok((!held_by_partner_or_staff(db, phone).await?).then_some(hash))
}

/// True when `stored` (a burnt code's hash) is the active fixed code of the review number.
pub async fn is_review_code(db: &PgPool, phone: &str, stored: &str) -> Result<bool, sqlx::Error> {
    Ok(review_code_hash(db, phone).await?.is_some_and(|h| h == stored))
}

/// Sample bookings for the review account, so reviewers see a history without paying. Two rows
/// (one upcoming, one completed), written only while the account has none. Returns rows written.
async fn seed_review_samples(db: &PgPool, phone: &str) -> ApiResult<usize> {
    let mut tx = db.begin().await?;
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ($1, $2) ON CONFLICT (phone) DO UPDATE SET phone = EXCLUDED.phone RETURNING id")
            .bind(phone)
            .bind(REVIEW_NAME)
            .fetch_one(&mut *tx)
            .await?;
    let has: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM bookings WHERE user_id = $1)")
        .bind(user)
        .fetch_one(&mut *tx)
        .await?;
    let service: Option<String> = sqlx::query_scalar("SELECT code FROM services WHERE active ORDER BY sort_order LIMIT 1")
        .fetch_optional(&mut *tx)
        .await?;
    // No catalogue yet (or samples already there): nothing to add, the login still works.
    let Some(service) = service.filter(|_| !has) else {
        tx.rollback().await?;
        return Ok(0);
    };
    let pro: Option<(Uuid, String)> = sqlx::query_as("SELECT id, team_name FROM pros WHERE status = 'verified' ORDER BY created_at LIMIT 1")
        .fetch_optional(&mut *tx)
        .await?;
    let (pro_id, team) = pro.map_or((None, None), |(id, team)| (Some(id), Some(team)));
    let today = (Utc::now() + chrono::Duration::hours(9)).date_naive();
    let address = json!({ "kind": "home", "label": "샘플 집", "road_address": "서울특별시 강남구 테헤란로 1", "detail": "101동 101호", "size_pyeong": 20 });
    // (days from today, status): upcoming confirmed visit, then a completed one.
    for (offset, status) in [(3_i64, 1_i16), (-7, 3)] {
        let number = super::booking_core::next_number(&mut tx).await?;
        sqlx::query(
            "INSERT INTO bookings (id, number, user_id, service_code, address, district_code, visit_date, start_hour, status, pro_id, team_name, final_price, paid_amount, balance_paid, visit_notes)
             VALUES ($1,$2,$3,$4,$5,$6,$7,10,$8,$9,$10,$11,$11,TRUE,'{}'::jsonb)",
        )
        .bind(Uuid::now_v7())
        .bind(&number)
        .bind(user)
        .bind(&service)
        .bind(&address)
        .bind(REVIEW_DISTRICT)
        .bind(today + chrono::Duration::days(offset))
        .bind(status)
        .bind(pro_id)
        .bind(&team)
        .bind(120_000_i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(2)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewAccount {
    phone: String,
    code: String,
    days: Option<i64>,
}

/// True when a customer, pro, team member or staff member holds `phone`, other than the review account's own
/// seeded row: pointing the fixed-code login at a real person would let reviewers sign in as them.
async fn phone_in_use(db: &PgPool, phone: &str) -> Result<bool, sqlx::Error> {
    // shortcut: the seeded row is known by its name (no flag column), so a customer who renames themselves
    // '리뷰 계정' could be chosen; add a users flag if names become editable by staff.
    let customer: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE phone = $1 AND name IS DISTINCT FROM $2)")
        .bind(phone)
        .bind(REVIEW_NAME)
        .fetch_one(db)
        .await?;
    Ok(customer || held_by_partner_or_staff(db, phone).await?)
}

/// True when a pro, team member or staff member holds `phone`.
async fn held_by_partner_or_staff(db: &PgPool, phone: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pros WHERE phone = $1)
             OR EXISTS (SELECT 1 FROM team_members WHERE phone = $1)
             OR EXISTS (SELECT 1 FROM staff WHERE phone = $1)",
    )
    .bind(phone)
    .fetch_one(db)
    .await
}

/// Owner only, with the owner's code (`X-Owner-Totp`): the fixed-code login signs in without an SMS.
async fn put_review(
    State(s): State<AppState>,
    Admin(a): Admin,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(r): Json<ReviewAccount>,
) -> ApiResult<Json<Value>> {
    let step = owner::check_owner(&s, a, &headers, Some(peer.ip()), true).await?;
    let phone = crate::auth::normalize_phone(&r.phone).ok_or(ApiError::BadRequest("invalid_phone"))?;
    let code = r.code.trim();
    if code.len() != REVIEW_CODE_LEN || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::BadRequest("invalid_code"));
    }
    if phone_in_use(&s.db, &phone).await? {
        return Err(ApiError::Conflict("phone_in_use"));
    }
    owner::burn_totp(&s, a.id, owner::client_ip(&s, Some(peer.ip()), &headers), step).await?;
    let days = r.days.unwrap_or(14).clamp(1, REVIEW_MAX_DAYS);
    let expires_at = Utc::now() + chrono::Duration::days(days);
    let samples = seed_review_samples(&s.db, &phone).await?;
    let value = json!({
        "phone": phone,
        "code_hash": crate::auth::otp_hash(s.config.otp_key(), &phone, code),
        "expires_at": expires_at.to_rfc3339(),
    });
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()")
        .bind(REVIEW_KEY)
        .bind(&value)
        .execute(&s.db)
        .await?;
    audit(
        &s.db,
        a.role.as_str(),
        Some(a.id),
        "setup.review_account",
        &phone,
        json!({ "days": days, "samples": samples }),
    )
    .await?;
    Ok(Json(json!({ "phone": phone, "expires_at": expires_at, "samples": samples })))
}

/// Owner only, with the owner's code, like switching it on.
async fn clear_review(State(s): State<AppState>, owner::OwnerTotp(a): owner::OwnerTotp) -> ApiResult<Json<Value>> {
    sqlx::query("DELETE FROM settings WHERE key = $1").bind(REVIEW_KEY).execute(&s.db).await?;
    audit(&s.db, a.role.as_str(), Some(a.id), "setup.review_account_cleared", REVIEW_KEY, json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn review_summary(db: &PgPool) -> Result<Value, sqlx::Error> {
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(REVIEW_KEY)
        .fetch_optional(db)
        .await?;
    Ok(v.map(|v| json!({ "phone": v["phone"], "expires_at": v["expires_at"] })).unwrap_or(Value::Null))
}

/// Shop name from the business profile, sent as the `#{brand}` AlimTalk variable.
pub async fn brand_name(db: &PgPool) -> String {
    brand::brand_name_for(db, "ko").await
}

async fn business(db: &PgPool) -> Result<Value, sqlx::Error> {
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(BUSINESS_KEY)
        .fetch_optional(db)
        .await?;
    Ok(v.unwrap_or_else(|| json!({})))
}

const BUSINESS_TEXT: [&str; 4] = ["name", "support_phone", "support_email", "support_hours"];
/// Store pages the website's download buttons point to.
/// https links: store pages, the Kakao channel and Naver Talk Talk (website chat buttons).
const STORE_LINKS: [&str; 6] = [
    "app_store_url",
    "play_store_url",
    "partner_app_store_url",
    "partner_play_store_url",
    "kakao_channel_url",
    "naver_talk_url",
];
/// Search Console / Search Advisor verification tokens, served as meta tags and verification files.
const VERIFY_KEYS: [&str; 2] = ["google_site_verification", "naver_site_verification"];
const COMPANY_TEXT: [&str; 8] = [
    "legal_name",
    "ceo",
    "registration_no",
    "mail_order_no",
    "address",
    "privacy_officer",
    "privacy_email",
    // 전자상거래법 시행규칙 제7조: the mall operator also names its hosting provider.
    "hosting_provider",
];

fn clean_text(v: &Value, key: &str) -> Result<Option<String>, ApiError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.chars().count() > MAX_FIELD_CHARS {
                return Err(ApiError::BadRequest("field_too_long"));
            }
            Ok((!t.is_empty()).then(|| t.to_string()))
        }
        _ => Err(ApiError::BadRequest("invalid_input")),
    }
}

const APP_VERSION_KEYS: [&str; 2] = ["min_app_version", "latest_app_version"];

/// `1.2.3`: the store version an app reports. Below `min_app_version` the app stops and asks to update.
pub fn is_app_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// `{"customer": {"min_app_version": "1.0.0", "latest_app_version": "1.2.0"}, "partner": {...}}`
fn clean_app_versions(input: &Value) -> Result<Value, ApiError> {
    let apps = input.as_object().ok_or(ApiError::BadRequest("invalid_input"))?;
    let mut out = Map::new();
    for app in ["customer", "partner"] {
        let mut entry = Map::new();
        for k in APP_VERSION_KEYS {
            if let Some(v) = clean_text(apps.get(app).unwrap_or(&Value::Null), k)? {
                if !is_app_version(&v) {
                    return Err(ApiError::BadRequest("invalid_version"));
                }
                entry.insert(k.into(), json!(v));
            }
        }
        if !entry.is_empty() {
            out.insert(app.into(), Value::Object(entry));
        }
    }
    Ok(Value::Object(out))
}

pub fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Only our own uploads or https images: the logo is shown in every app and on the website.
pub fn is_logo_url(base: &str, s: &str) -> bool {
    crate::util::own_upload(base, s).is_some() || (s.starts_with("https://") && !s.contains(char::is_whitespace))
}

/// Validated business profile; unknown keys are dropped.
pub fn clean_business(base: &str, input: &Value) -> Result<Value, ApiError> {
    let mut out = Map::new();
    for k in BUSINESS_TEXT {
        if let Some(t) = clean_text(input, k)? {
            out.insert(k.into(), json!(t));
        }
    }
    if let Some(c) = clean_text(input, "primary_color")? {
        if !is_hex_color(&c) {
            return Err(ApiError::BadRequest("invalid_color"));
        }
        out.insert("primary_color".into(), json!(c.to_uppercase()));
    }
    if let Some(u) = clean_text(input, "logo_url")? {
        if !is_logo_url(base, &u) {
            return Err(ApiError::BadRequest("invalid_logo"));
        }
        out.insert("logo_url".into(), json!(crate::util::own_upload(base, &u).unwrap_or(u)));
    }
    for k in STORE_LINKS {
        if let Some(u) = clean_text(input, k)? {
            if !u.starts_with("https://") || u.contains(char::is_whitespace) {
                return Err(ApiError::BadRequest("invalid_link"));
            }
            out.insert(k.into(), json!(u));
        }
    }
    for k in VERIFY_KEYS {
        if let Some(t) = clean_text(input, k)? {
            if !crate::seo::is_verification_token(&t) {
                return Err(ApiError::BadRequest("invalid_token"));
            }
            out.insert(k.into(), json!(t));
        }
    }
    let empty = json!({});
    let company_in = input.get("company").unwrap_or(&empty);
    let mut company = Map::new();
    for k in COMPANY_TEXT {
        if let Some(t) = clean_text(company_in, k)? {
            company.insert(k.into(), json!(t));
        }
    }
    out.insert("company".into(), Value::Object(company));
    if let Some(apps) = input.get("app_versions") {
        out.insert("app_versions".into(), clean_app_versions(apps)?);
    }
    brand::clean_extra(base, input, &mut out)?;
    if !out.contains_key("name") {
        return Err(ApiError::BadRequest("missing_field"));
    }
    Ok(Value::Object(out))
}

async fn put_business(State(s): State<AppState>, Admin(a): Admin, Json(input): Json<Value>) -> ApiResult<Json<Value>> {
    let value = clean_business(&s.config.public_base_url, &input)?;
    if let Some(logo) = value["logo_url"].as_str().filter(|l| l.starts_with("/files/"))
        && !uploads::owned(&s.db, None, &[logo.to_string()]).await?
    {
        return Err(ApiError::BadRequest("invalid_logo"));
    }
    brand::check_assets(&s, &value).await?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()")
        .bind(BUSINESS_KEY)
        .bind(&value)
        .execute(&s.db)
        .await?;
    super::config::invalidate();
    super::seller::invalidate_business();
    audit(&s.db, a.role.as_str(), Some(a.id), "setup.business", BUSINESS_KEY, json!({})).await?;
    Ok(Json(value))
}

fn section(code: &str) -> ApiResult<Section> {
    Section::parse(code).ok_or(ApiError::NotFound)
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Text => "text",
        Kind::Secret => "secret",
        Kind::List => "list",
        Kind::Map => "map",
        Kind::File => "file",
        Kind::Toggle => "toggle",
    }
}

async fn card(s: &AppState, sec: Section, last_test: Option<Value>) -> ApiResult<Value> {
    let saved = integrations::load_section(&s.db, &s.config.data_key, sec).await?;
    let base = integrations::Integrations::from_config(&s.config);
    let fields: Vec<Value> = sec
        .fields()
        .iter()
        .map(|f| {
            let v = saved.get(f.name);
            let preview = match (f.kind, v) {
                // An unsaved switch shows what it does today (the env default), not "off".
                (Kind::Toggle, _) => json!(v.and_then(Value::as_bool).unwrap_or_else(|| integrations::toggle_default(&base, sec, f.name))),
                (kind, Some(v)) => integrations::preview(kind, v),
                (_, None) => Value::Null,
            };
            json!({
                "name": f.name,
                "kind": kind_name(f.kind),
                "required": f.required,
                "set": v.is_some() || integrations::env_provides(&base, sec, f.name),
                "preview": preview,
            })
        })
        .collect();
    let env = base.has(sec);
    let lock = integrations::lock_state(&s.db, sec).await?;
    let source = if !saved.is_empty() {
        "console"
    } else if env {
        "env"
    } else {
        "none"
    };
    // Toss cards only: the key prefix says whether the saved key moves test or real money. PortOne ids carry no mode.
    let mode = saved
        .get("secret_key")
        .and_then(Value::as_str)
        .filter(|_| matches!(sec, Section::Payout | Section::TossPayments))
        .and_then(integrations::toss_key_mode);
    Ok(
        json!({ "code": sec.code(), "source": source, "configured": source != "none", "mode": mode, "fields": fields, "last_test": last_test, "locked": lock == LockState::Locked, "unlock_expires_at": unlock_expires(lock) }),
    )
}

fn unlock_expires(lock: LockState) -> Option<String> {
    match lock {
        LockState::Unlocked(until, _) => Some(until.to_rfc3339()),
        LockState::Unsaved | LockState::Locked => None,
    }
}

/// Set by install.sh from `MALKKEUM_HEARTBEAT_URL`; the wizard shows a checklist item until it is.
fn heartbeat_configured() -> bool {
    std::env::var("HEARTBEAT_URL").is_ok_and(|v| !v.trim().is_empty())
}

/// Everything the wizard and the status page need in one call.
async fn overview(State(s): State<AppState>, Admin(a): Admin) -> ApiResult<Json<Value>> {
    let tests: Vec<(String, Option<Value>, Option<Value>)> = sqlx::query_as("SELECT section, last_test, last_check FROM integration_settings")
        .fetch_all(&s.db)
        .await?;
    let viewer_is_owner = owner::is_owner(&s.db, a.id).await?;
    let mut cards = Vec::new();
    for sec in Section::ALL {
        let (last, check) = tests
            .iter()
            .find(|(c, _, _)| c == sec.code())
            .map(|(_, t, c)| (t.clone(), c.clone()))
            .unwrap_or_default();
        let lock = integrations::lock_state(&s.db, sec).await?;
        let mut c = card(&s, sec, last.clone()).await?;
        if let Some(fields) = c.as_object_mut() {
            // The latest check, passed or failed; rows from before last_check existed only have last_test.
            fields.insert("last_check".into(), check.or(last).unwrap_or(Value::Null));
            fields.insert("can_test".into(), json!(may_test(lock, a.id, viewer_is_owner)));
            // Only the owner sees when an unlock window closes; anyone else would learn when the owner is editing.
            if !viewer_is_owner {
                fields.remove("unlock_expires_at");
            }
        }
        cards.push(c);
    }
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM staff WHERE active),
                (SELECT count(*) FROM pros WHERE status = 'verified'),
                (SELECT count(*) FROM services WHERE active),
                (SELECT count(*) FROM bookings)",
    )
    .fetch_one(&s.db)
    .await?;
    let gateways = payment_gateways(&s).await?;
    Ok(Json(json!({
        "business": business(&s.db).await?,
        "store_build": operator::store_build(&s).await?,
        // The console shows the switch on when either is true; `env` means TOSS_PAYOUTS_ENABLED forces it on.
        "payout_toss_switch": { "on": integrations::toss_switch_on(&s.db).await?, "env": integrations::toss_env_enabled() },
        "review_account": review_summary(&s.db).await?,
        "integrations": cards,
        "counts": { "staff": counts.0, "verified_pros": counts.1, "active_services": counts.2, "bookings": counts.3 },
        "dev_mode": s.config.dev_mode,
        // Only a flag: the ping URL is a secret-like address and stays on the host.
        "heartbeat_configured": heartbeat_configured(),
        "public_base_url": s.config.public_base_url,
        "webhook_url": format!("{}/v1/webhooks/portone", s.config.public_base_url.trim_end_matches('/')),
        "toss_webhook_url": format!("{}/v1/webhooks/toss", s.config.public_base_url.trim_end_matches('/')),
        "toss_payments_webhook_url": format!("{}/v1/webhooks/tosspayments", s.config.public_base_url.trim_end_matches('/')),
        // Every gateway a new payment can reach, PortOne first; `payment_gateway(_mode)` is the first one, for
        // consoles built before both could run.
        "payment_gateways": gateways,
        "payment_gateway": gateways.first().map_or(json!("portone"), |g| g["code"].clone()),
        "payment_gateway_mode": gateways.first().map_or(Value::Null, |g| g["mode"].clone()),
    })))
}

/// `payment_gateways` of the overview: `[{code, mode}]`, `mode` "test" while Toss takes payments on test keys
/// (bookings get paid with no money behind them), null for PortOne (its ids carry no mode).
async fn payment_gateways(s: &AppState) -> ApiResult<Vec<Value>> {
    let enabled = super::payments::enabled_codes(&s.db).await?;
    let integ = s.providers.integ();
    Ok(integ
        .payment_gateways(&enabled)
        .into_iter()
        .map(|g| {
            let mode = match g {
                Gateway::Portone => None,
                Gateway::TossPayments => integ.toss_payments.as_ref().and_then(|t| integrations::toss_key_mode(&t.secret_key)),
            };
            json!({ "code": g.code(), "mode": mode })
        })
        .collect())
}

/// Recomputes the live keys from env + every saved section and hands them to the providers.
pub async fn reload(s: &AppState) -> Result<(), sqlx::Error> {
    let integ = integrations::load(&s.db, &s.config).await?;
    s.providers.set_integrations(integ);
    Ok(())
}

fn is_toss(values: &Map<String, Value>) -> bool {
    values.get("mode").and_then(Value::as_str) == Some("toss")
}

async fn put_section(
    State(s): State<AppState>,
    Admin(a): Admin,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(input): Json<Value>,
) -> ApiResult<Json<Value>> {
    let sec = section(&code)?;
    let lock = integrations::lock_state(&s.db, sec).await?;
    if lock == LockState::Locked {
        return Err(ApiError::Conflict("card_locked"));
    }
    require_unlocker(&s, a, lock).await?;
    let incoming = input.as_object().ok_or(ApiError::BadRequest("invalid_input"))?;
    let current = integrations::load_section(&s.db, &s.config.data_key, sec).await?;
    let merged = integrations::merge(sec, &current, incoming);
    // The first save of a card that moves money needs the owner's code: either payment card adds a place checkout
    // money goes, and Toss payouts move real money. An unlock already checked it. The code is checked now and
    // spent only once the values validate, so a typo does not cost a code.
    let money_card = match sec {
        Section::Portone | Section::TossPayments => true,
        Section::Payout => is_toss(&merged),
        _ => false,
    };
    let owner_step = if money_card && lock == LockState::Unsaved {
        Some(owner::check_owner(&s, a, &headers, Some(peer.ip()), true).await?)
    } else {
        None
    };
    // Validate against env-only keys so this section stands on its own.
    let mut base = integrations::Integrations::from_config(&s.config);
    if sec == Section::Payout {
        base.payout_toss_ready = payout_toss_ready(&s, &merged).await?;
    }
    integrations::apply(&base, sec, &merged).map_err(|Invalid(code)| ApiError::BadRequest(code))?;
    // Leaving Toss strands transfers still open at Toss: their sync needs these keys.
    if sec == Section::Payout && !is_toss(&merged) && crate::payout_sync::toss_transfers_open(&s.db).await? {
        return Err(ApiError::Conflict("payout_in_transfer"));
    }
    if let Some(step) = owner_step {
        owner::burn_totp(&s, a.id, owner::client_ip(&s, Some(peer.ip()), &headers), step).await?;
    }
    if !integrations::save_section(&s.db, &s.config.data_key, sec, &merged, a.id).await? {
        return Err(ApiError::Conflict("card_locked"));
    }
    reload(&s).await?;
    audit(
        &s.db,
        a.role.as_str(),
        Some(a.id),
        "setup.integration",
        sec.code(),
        // The gateways new payments reach after the save are on record, so each switch is audited.
        json!({ "fields": merged.keys().collect::<Vec<_>>(), "gateways": payment_gateways(&s).await?.iter().map(|g| g["code"].clone()).collect::<Vec<_>>() }),
    )
    .await?;
    Ok(Json(card(&s, sec, None).await?))
}

/// While a card is open only the owner who opened it may change it.
async fn require_unlocker(s: &AppState, a: Auth, lock: LockState) -> ApiResult<()> {
    let LockState::Unlocked(_, by) = lock else { return Ok(()) };
    if by != a.id || !owner::is_owner(&s.db, a.id).await? {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

async fn clear_section(State(s): State<AppState>, Admin(a): Admin, Path(code): Path<String>) -> ApiResult<Json<Value>> {
    let sec = section(&code)?;
    let lock = integrations::lock_state(&s.db, sec).await?;
    if lock == LockState::Locked {
        return Err(ApiError::Conflict("card_locked"));
    }
    if lock == LockState::Unsaved {
        return Ok(Json(card(&s, sec, None).await?));
    }
    require_unlocker(&s, a, lock).await?;
    if sec == Section::Payout && crate::payout_sync::toss_transfers_open(&s.db).await? {
        return Err(ApiError::Conflict("payout_in_transfer"));
    }
    // The row stays, locked and empty: a non-owner admin cannot re-save the card without an unlock.
    if !integrations::save_section(&s.db, &s.config.data_key, sec, &Map::new(), a.id).await? {
        return Err(ApiError::Conflict("card_locked"));
    }
    reload(&s).await?;
    audit(&s.db, a.role.as_str(), Some(a.id), "setup.integration_cleared", sec.code(), json!({})).await?;
    Ok(Json(card(&s, sec, None).await?))
}

/// Owner unlock of a saved card for `UNLOCK_TTL_SECS`. Every outcome is audited; no secret is in the record.
async fn unlock_section(
    State(s): State<AppState>,
    Admin(a): Admin,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> ApiResult<Json<Value>> {
    let sec = section(&code)?;
    match try_unlock(&s, a, &headers, peer.ip(), sec).await {
        Ok(until) => Ok(Json(json!({ "unlocked_until": until.to_rfc3339() }))),
        Err(e) => {
            // The caller gets the real reason even when this record cannot be written.
            if let Err(audit_error) = audit(
                &s.db,
                a.role.as_str(),
                Some(a.id),
                "setup.unlock_failed",
                sec.code(),
                json!({ "reason": unlock_failure_reason(&e) }),
            )
            .await
            {
                tracing::error!(error = ?audit_error, "unlock failure not recorded");
            }
            Err(e)
        }
    }
}

async fn try_unlock(s: &AppState, a: Auth, headers: &HeaderMap, ip: IpAddr, sec: Section) -> ApiResult<DateTime<Utc>> {
    let step = owner::check_owner(s, a, headers, Some(ip), true).await?;
    if integrations::lock_state(&s.db, sec).await? == LockState::Unsaved {
        return Err(ApiError::Conflict("nothing_saved"));
    }
    owner::burn_totp(s, a.id, owner::client_ip(s, Some(ip), headers), step).await?;
    // The window and its audit record commit together: an open window always has a record.
    let mut tx = s.db.begin().await?;
    let until: Option<DateTime<Utc>> = sqlx::query_scalar(
        "UPDATE integration_settings SET unlocked_until = now() + make_interval(secs => $2), unlocked_by = $3 WHERE section = $1 RETURNING unlocked_until",
    )
    .bind(sec.code())
    .bind(UNLOCK_TTL_SECS as f64)
    .bind(a.id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    let until = until.ok_or(ApiError::Conflict("nothing_saved"))?;
    audit(
        &mut *tx,
        a.role.as_str(),
        Some(a.id),
        "setup.unlock",
        sec.code(),
        json!({ "expires_at": until.to_rfc3339() }),
    )
    .await?;
    tx.commit().await?;
    Ok(until)
}

fn unlock_failure_reason(e: &ApiError) -> &'static str {
    match e {
        ApiError::Forbidden => "not_owner",
        ApiError::BadRequest(c) | ApiError::Conflict(c) => c,
        ApiError::RateLimited => "rate_limited",
        ApiError::Unauthorized => "unauthorized",
        _ => "error",
    }
}

async fn lock_section(State(s): State<AppState>, Admin(a): Admin, Path(code): Path<String>) -> ApiResult<Json<Value>> {
    let sec = section(&code)?;
    sqlx::query("UPDATE integration_settings SET unlocked_until = NULL WHERE section = $1")
        .bind(sec.code())
        .execute(&s.db)
        .await?;
    audit(&s.db, a.role.as_str(), Some(a.id), "setup.lock", sec.code(), json!({})).await?;
    Ok(Json(card(&s, sec, None).await?))
}

/// Maps an HTTP status from a provider's check call to the wizard's result code.
pub fn check_result(status: StatusCode, ok_statuses: &[StatusCode]) -> (bool, &'static str) {
    if ok_statuses.contains(&status) {
        (true, "connected")
    } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        (false, "invalid_key")
    } else {
        (false, "provider_error")
    }
}

async fn probe(s: &AppState, sec: Section) -> (bool, &'static str) {
    let integ = s.providers.integ();
    let http = s.providers.http();
    match sec {
        Section::Portone => match integ.portone.api_secret.as_deref() {
            None => (false, "not_configured"),
            Some(secret) => match portone::check(http, secret).await {
                Ok(st) => check_result(st, &[StatusCode::NOT_FOUND, StatusCode::OK]),
                Err(_) => (false, "unreachable"),
            },
        },
        Section::TossPayments => match integ.toss_payments.as_ref() {
            None => (false, "not_configured"),
            Some(t) => toss_payments_probe(s, t).await,
        },
        Section::Solapi => match integ.solapi.as_ref() {
            None => (false, "not_configured"),
            Some(cfg) => match sms::check(http, cfg).await {
                Ok(st) => check_result(st, &[StatusCode::OK]),
                Err(_) => (false, "unreachable"),
            },
        },
        Section::Push => match integ.fcm.as_ref() {
            None => (false, "not_configured"),
            Some(acc) => match fcm::exchange(http, acc, Utc::now().timestamp()).await {
                Ok(_) => (true, "connected"),
                Err(_) => (false, "invalid_key"),
            },
        },
        Section::Ai => match integ.ai.as_ref() {
            None => (false, "not_configured"),
            Some(cfg) => match crate::ai::check(http, &s.config.anthropic_base_url, &cfg.api_key).await {
                Ok(st) => check_result(st, &[StatusCode::OK]),
                Err(_) => (false, "unreachable"),
            },
        },
        Section::Backup => match integ.backup.as_ref() {
            None => (false, "not_configured"),
            Some(cfg) => match backup::check(http, cfg).await {
                Ok(()) => (true, "connected"),
                Err(e) => (false, e.code()),
            },
        },
        Section::Juso => match integ.juso_confm_key.as_deref() {
            None => (false, "not_configured"),
            Some(key) => match address::check(http, key).await {
                Ok(()) => (true, "connected"),
                Err(_) => (false, "provider_error"),
            },
        },
        // Login keys can only be proven by a real sign-in from the app.
        _ if integ.has(sec) => (true, "saved_check_in_app"),
        _ => (false, "not_configured"),
    }
}

/// Checks the window MID's secret, then the billing MID's when one is set. 404 on an order that cannot exist = accepted.
async fn toss_payments_probe(s: &AppState, t: &integrations::TossPayments) -> (bool, &'static str) {
    let probe = |secret: String| async move {
        match toss_payments::check(&s.providers, &secret).await {
            // Toss also answers a bad secret with 400 INVALID_API_KEY.
            Ok(StatusCode::BAD_REQUEST) => (false, "invalid_key"),
            Ok(st) => check_result(st, &[StatusCode::NOT_FOUND]),
            Err(_) => (false, "unreachable"),
        }
    };
    let window = probe(t.secret_key.clone()).await;
    match &t.billing {
        Some((_, sk)) if window.0 => match probe(sk.clone()).await {
            (false, "invalid_key") => (false, "billing_invalid_key"),
            other => other,
        },
        _ => window,
    }
}

/// Whether the Toss gate is open for these payout values: env or console switch on and the saved test passed for this key.
async fn payout_toss_ready(s: &AppState, values: &serde_json::Map<String, Value>) -> ApiResult<bool> {
    let last: Option<Value> = sqlx::query_scalar("SELECT last_test FROM integration_settings WHERE section = $1")
        .bind(Section::Payout.code())
        .fetch_optional(&s.db)
        .await?
        .flatten();
    let secret = values.get("secret_key").and_then(Value::as_str).unwrap_or_default();
    Ok(integrations::toss_ready(
        integrations::toss_switch_on(&s.db).await?,
        last.as_ref(),
        &s.config.data_key,
        secret,
    ))
}

/// Connection check for Toss payouts: `GET /v2/balances` (read-only) with the saved secret key. A `test_sk_` key
/// records `sandbox_connected`, a `live_sk_` key records `live_connected`. The record keeps a key fingerprint,
/// so changing the key means testing again.
async fn payout_test(s: &AppState) -> ApiResult<Value> {
    let saved = integrations::load_section(&s.db, &s.config.data_key, Section::Payout).await?;
    let secret = saved.get("secret_key").and_then(Value::as_str).unwrap_or_default();
    let mode_code = integrations::connected_code(secret);
    let (ok, code) = if secret.is_empty() {
        (false, "not_configured")
    } else if mode_code.is_none() {
        (false, "invalid_secret_key")
    } else {
        match s.providers.payout_balance_check(secret).await {
            Ok(st) => check_result(st, &[StatusCode::OK]),
            Err(_) => (false, "unreachable"),
        }
    };
    let code = match (ok, mode_code) {
        (true, Some(connected)) => connected,
        _ => code,
    };
    Ok(json!({ "ok": ok, "code": code, "key_fp": integrations::key_fingerprint(&s.config.data_key, secret), "at": Utc::now() }))
}

/// Who may run a card's connection test. It takes no values: it probes with what is saved and records only its
/// result, so the owner runs it on a locked card without an unlock. An open unlock window belongs to the owner
/// who opened it. Anyone may test a card that was never saved.
fn may_test(lock: LockState, viewer: Uuid, viewer_is_owner: bool) -> bool {
    match lock {
        LockState::Unsaved => true,
        LockState::Locked => viewer_is_owner,
        LockState::Unlocked(_, by) => viewer_is_owner && by == viewer,
    }
}

async fn test_section(State(s): State<AppState>, Admin(a): Admin, Path(code): Path<String>) -> ApiResult<Json<Value>> {
    let sec = section(&code)?;
    let lock = integrations::lock_state(&s.db, sec).await?;
    if !may_test(lock, a.id, owner::is_owner(&s.db, a.id).await?) {
        return Err(ApiError::Forbidden);
    }
    let record = if sec == Section::Payout {
        payout_test(&s).await?
    } else {
        let (ok, result) = probe(&s, sec).await;
        json!({ "ok": ok, "code": result, "at": Utc::now() })
    };
    // The saved payout pass is what opens Toss (`toss_ready`): a failed check, say a short Toss outage, must not
    // replace it, or the loader would drop the saved Toss settings and stop the sync of open transfers. The
    // failure is still answered, audited and kept as last_check, so the card shows it.
    let keeps_pass = sec == Section::Payout && record["ok"] != json!(true);
    let stored: Option<Value> = sqlx::query_scalar(
        "UPDATE integration_settings SET last_check = $2, last_test = CASE WHEN $3 THEN last_test ELSE $2 END
         WHERE section = $1 RETURNING last_test",
    )
    .bind(sec.code())
    .bind(&record)
    .bind(keeps_pass)
    .fetch_optional(&s.db)
    .await?
    .flatten();
    audit(&s.db, a.role.as_str(), Some(a.id), "setup.test", sec.code(), record.clone()).await?;
    let mut body = record;
    body["last_test"] = stored.unwrap_or(Value::Null);
    Ok(Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_owner_tests_a_locked_or_open_card() {
        let (me, other) = (Uuid::now_v7(), Uuid::now_v7());
        let open = |by| LockState::Unlocked(Utc::now(), by);
        assert!(may_test(LockState::Unsaved, me, false), "a card never saved");
        assert!(may_test(LockState::Locked, me, true));
        assert!(!may_test(LockState::Locked, me, false));
        assert!(may_test(open(me), me, true));
        assert!(!may_test(open(other), me, true), "another owner's window");
        assert!(!may_test(open(me), me, false));
    }

    #[test]
    fn setup_code_uses_unambiguous_characters() {
        let c = random_code();
        assert_eq!(c.len(), SETUP_CODE_LEN);
        assert!(c.bytes().all(|b| SETUP_ALPHABET.contains(&b)));
        assert!(!c.contains(['0', 'O', '1', 'I', 'L']));
    }

    #[test]
    fn setup_code_expires_seven_days_after_issue() {
        let now = 1_000_000;
        assert!(!setup_code_expired(Some(now), now));
        assert!(!setup_code_expired(Some(now - SETUP_CODE_TTL_SECS), now));
        assert!(setup_code_expired(Some(now - SETUP_CODE_TTL_SECS - 1), now));
        assert!(setup_code_expired(None, now), "a code without an issue time is never valid");
    }

    #[test]
    fn installer_code_keeps_its_issue_time_and_expired_stored_code_is_replaced() {
        let now = 1_000_000;
        let issued = now - 60;
        let saved = Some(("OLDCODE23456".to_string(), Some(issued)));
        assert_eq!(
            choose_setup_code(Some("NEWCODE23456"), Some(issued), saved.clone(), now),
            ("NEWCODE23456".to_string(), issued)
        );
        assert_eq!(choose_setup_code(None, None, saved, now), ("OLDCODE23456".to_string(), issued));
        let stale = Some(("OLDCODE23456".to_string(), Some(now - SETUP_CODE_TTL_SECS - 1)));
        let (code, at) = choose_setup_code(None, None, stale, now);
        assert_ne!(code, "OLDCODE23456");
        assert_eq!(at, now);
    }

    #[test]
    fn right_but_expired_code_is_told_apart_from_wrong() {
        let now = 1_000_000;
        let issued = now - SETUP_CODE_TTL_SECS - 1;
        assert!(matches!(check_setup_code("CODE2345678A", Some(now), "CODE2345678A", now), Ok(())));
        assert!(matches!(
            check_setup_code("CODE2345678A", Some(issued), "CODE2345678A", now),
            Err(ApiError::SetupCodeExpired)
        ));
        assert!(matches!(
            check_setup_code("CODE2345678A", Some(issued), "WRONG2345678", now),
            Err(ApiError::Forbidden)
        ));
        assert!(matches!(
            check_setup_code("CODE2345678A", Some(now), "WRONG2345678", now),
            Err(ApiError::Forbidden)
        ));
        assert!(
            matches!(check_setup_code("", None, "", now), Err(ApiError::Forbidden)),
            "no stored code never matches"
        );
    }

    #[test]
    fn business_profile_is_validated_and_trimmed() {
        let v = clean_business(
            "http://127.0.0.1:8080",
            &json!({
                "name": "  다해 ", "primary_color": "#3b5bfd", "logo_url": "/files/0123456789abcdef0123456789abcdef.png",
                "company": { "ceo": "홍길동", "hosting_provider": " 예시호스팅(주) ", "unknown": "x" }, "evil": 1
            }),
        )
        .unwrap();
        assert_eq!(v["name"], "다해");
        assert_eq!(v["primary_color"], "#3B5BFD");
        assert_eq!(v["company"]["ceo"], "홍길동");
        assert_eq!(v["company"]["hosting_provider"], "예시호스팅(주)");
        assert!(v["company"].get("unknown").is_none());
        assert!(v.get("evil").is_none());
    }

    #[test]
    fn business_profile_rejects_bad_values() {
        let err = |v: Value| clean_business("http://127.0.0.1:8080", &v).unwrap_err().to_string();
        assert!(err(json!({})).contains("missing_field"));
        assert!(err(json!({"name": "a", "primary_color": "blue"})).contains("invalid_color"));
        assert!(err(json!({"name": "a", "logo_url": "javascript:alert(1)"})).contains("invalid_logo"));
        assert!(err(json!({"name": "a", "logo_url": "/files/../etc/passwd"})).contains("invalid_logo"));
        assert!(err(json!({"name": "x".repeat(201)})).contains("field_too_long"));
        assert!(err(json!({"name": "a", "play_store_url": "http://x"})).contains("invalid_link"));
        assert!(err(json!({"name": "a", "app_versions": {"customer": {"min_app_version": "1.0"}}})).contains("invalid_version"));
    }

    #[test]
    fn app_versions_are_kept_per_app() {
        let v = clean_business(
            "http://127.0.0.1:8080",
            &json!({"name": "a", "app_versions": {"customer": {"min_app_version": " 1.0.0 ", "latest_app_version": "1.2.0"}, "partner": {}}}),
        )
        .unwrap();
        assert_eq!(v["app_versions"]["customer"]["min_app_version"], "1.0.0");
        assert_eq!(v["app_versions"]["customer"]["latest_app_version"], "1.2.0");
        assert!(v["app_versions"].get("partner").is_none());
        assert!(is_app_version("10.0.12") && !is_app_version("1.0.0-beta") && !is_app_version("1..0"));
    }

    #[test]
    fn provider_status_maps_to_wizard_result() {
        assert_eq!(check_result(StatusCode::NOT_FOUND, &[StatusCode::NOT_FOUND]), (true, "connected"));
        assert_eq!(check_result(StatusCode::UNAUTHORIZED, &[StatusCode::OK]), (false, "invalid_key"));
        assert_eq!(check_result(StatusCode::BAD_GATEWAY, &[StatusCode::OK]), (false, "provider_error"));
    }
}
