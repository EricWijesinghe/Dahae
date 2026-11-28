// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Setup-wizard steps 0 and 1 (§5) and the go-live review (D10).
//!
//! - `GET /admin/setup/languages`, `PUT /admin/setup/languages`: the console language for the install, the customer
//!   languages, and the signed-in staff member's own language (`staff.locale`).
//! - `GET /admin/setup/packs`: every country pack in the image as a preview card, with the install's facts.
//! - `GET /admin/setup/country/preview?country=XX`: what changing to that country would change, before it happens.
//! - `PUT /admin/setup/country`: the owner sets the country; refused after the first live payment.
//! - `PUT /admin/setup/marketplace`: "How you sell" (step 4), checked against the pack.
//! - `GET /admin/setup/gates`: the go-live gates and whether the site is in preview.

use super::catalog::recommended;
use crate::{
    adapters::{self, Capability},
    auth::{Admin, Staff},
    error::{ApiError, ApiResult},
    golive, owner,
    pack::{self, MaturityLevel, Pack},
    routes::me::SUPPORTED_LOCALES,
    settings_registry::{self, storage_key},
    state::AppState,
};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};

/// The install facts the country step writes, in the order the preview lists them.
const COUNTRY_FACTS: [&str; 4] = ["install.country", "install.currency", "install.tz", "install.locales"];

/// A shipped locale as the language picker shows it: its own name, its direction and whether it is a machine
/// translation (shown with a banner until reviewed).
fn locale_card(code: &str) -> Value {
    let (name, rtl, machine) = match code {
        "ko" => ("한국어", false, false),
        "en" => ("English", false, false),
        "ar" => ("العربية", true, true),
        _ => (code, false, true),
    };
    json!({ "code": code, "name": name, "rtl": rtl, "machine": machine })
}

async fn fact(s: &AppState, key: &str) -> ApiResult<Value> {
    settings_registry::get_value(&s.db, &s.config, key).await
}

fn is_set(v: &Value) -> bool {
    v.as_str().is_some_and(|t| !t.is_empty())
}

async fn my_locale(s: &AppState, staff: uuid::Uuid) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT locale FROM staff WHERE id = $1")
        .bind(staff)
        .fetch_optional(&s.db)
        .await?
        .flatten())
}

pub(super) async fn get_languages(State(s): State<AppState>, Staff(a): Staff) -> ApiResult<Json<Value>> {
    Ok(Json(json!({
        "console_locale": fact(&s, "install.console_locale").await?,
        "locales": fact(&s, "install.locales").await?,
        "my_locale": my_locale(&s, a.id).await?,
        "pack_locales": s.pack.locales,
        "shipped": SUPPORTED_LOCALES.iter().map(|c| locale_card(c)).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LanguagesPatch {
    #[serde(default)]
    console_locale: Option<String>,
    #[serde(default)]
    locales: Option<Vec<String>>,
    /// The caller's own console language; an empty string clears it (follow the install's).
    #[serde(default)]
    my_locale: Option<String>,
}

/// Install-wide languages need a staff admin; anyone on staff may set their own.
pub(super) async fn put_languages(State(s): State<AppState>, Staff(a): Staff, Json(p): Json<LanguagesPatch>) -> ApiResult<Json<Value>> {
    if p.console_locale.is_some() || p.locales.is_some() {
        settings_registry::ensure_can_write(&s.db, a.id).await?;
    }
    let mine = match p.my_locale.as_deref() {
        None => None,
        Some("") => Some(None),
        Some(l) if SUPPORTED_LOCALES.contains(&l) => Some(Some(l.to_string())),
        Some(_) => return Err(ApiError::BadRequest("invalid_locale")),
    };
    let mut writes = Vec::new();
    if let Some(c) = p.console_locale {
        writes.push(("install.console_locale", json!(c)));
    }
    if let Some(l) = p.locales {
        writes.push(("install.locales", json!(l)));
    }
    for (key, value) in &writes {
        let spec = settings_registry::find(key).ok_or_else(|| ApiError::Internal(format!("unknown setting {key}")))?;
        settings_registry::validate(spec, value).map_err(|_| ApiError::BadRequest("invalid_locale"))?;
    }
    let mut tx = s.db.begin().await?;
    for (key, value) in &writes {
        put_fact(&mut tx, key, value).await?;
    }
    if let Some(mine) = &mine {
        sqlx::query("UPDATE staff SET locale = $2 WHERE id = $1")
            .bind(a.id)
            .bind(mine)
            .execute(&mut *tx)
            .await?;
    }
    if !writes.is_empty() {
        let data = json!(writes.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<serde_json::Map<_, _>>());
        audit(&mut tx, a.id, "install.languages", data).await?;
    }
    tx.commit().await?;
    get_languages(State(s), Staff(a)).await
}

/// One install fact, checked against the registry, written over any earlier value.
async fn put_fact(tx: &mut Transaction<'_, Postgres>, key: &str, value: &Value) -> ApiResult<()> {
    let spec = settings_registry::find(key).ok_or_else(|| ApiError::Internal(format!("unknown setting {key}")))?;
    let stored = settings_registry::validate(spec, value)?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = now()")
        .bind(storage_key(key))
        .bind(stored)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn audit(tx: &mut Transaction<'_, Postgres>, actor: uuid::Uuid, action: &str, data: Value) -> ApiResult<()> {
    sqlx::query("INSERT INTO audit_log (actor_role, actor_id, action, target, data) VALUES ('staff', $1, $2, 'install', $3)")
        .bind(actor)
        .bind(action)
        .bind(data)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The preview card of one pack (§5 step 1): currency, zones, customer languages, recommended providers (only
/// those this image offers), the legal documents that will be prepared and the maturity.
fn pack_card(p: &Pack) -> Value {
    let rec: serde_json::Map<String, Value> = Capability::ALL
        .into_iter()
        .map(|c| (c.code().to_string(), json!(recommended(p, c))))
        .filter(|(_, v)| v.as_array().is_some_and(|a| !a.is_empty()))
        .collect();
    json!({
        "id": p.id,
        "name": p.name,
        "version": p.version,
        "maturity": p.maturity,
        "draft": p.maturity.level == MaturityLevel::Draft,
        "currency": p.currency.code,
        "minor_units": p.currency.minor_units,
        "zones": p.zones,
        "locales": p.locales,
        "legal": p.legal,
        "recommended": rec,
        "marketplace_models": p.payments.marketplace_models,
    })
}

/// The install's country facts and whether the country may still change.
async fn current(s: &AppState) -> ApiResult<Value> {
    let mut facts = serde_json::Map::new();
    for key in COUNTRY_FACTS.iter().chain(&[
        "install.pack_version_applied",
        "install.locked_at",
        "install.marketplace_model",
        "install.draft_ack",
    ]) {
        facts.insert(key.trim_start_matches("install.").to_string(), fact(s, key).await?);
    }
    let locked = facts.get("locked_at").is_some_and(is_set);
    facts.insert("locked".into(), json!(locked));
    Ok(Value::Object(facts))
}

pub(super) async fn get_packs(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Value>> {
    let packs = all_packs(&s.pack)?;
    Ok(Json(json!({
        "current": current(&s).await?,
        // The pack this image runs; the owner still confirms it.
        "default": s.pack.id,
        "packs": packs.iter().map(pack_card).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub(super) struct PreviewQuery {
    country: String,
}

/// A country pack in the image; the one this install runs is taken as loaded at boot.
fn load_country(active: &Pack, id: &str) -> ApiResult<Pack> {
    if !pack::country_ids(pack::EMBEDDED).contains(&id) {
        return Err(ApiError::BadRequest("unknown_country"));
    }
    if id == active.id {
        return Ok(active.clone());
    }
    pack::load(id).map_err(|e| ApiError::Internal(e.to_string()))
}

/// Every pack in the image, the running one as loaded at boot.
fn all_packs(active: &Pack) -> ApiResult<Vec<Pack>> {
    pack::country_ids(pack::EMBEDDED).into_iter().map(|id| load_country(active, id)).collect()
}

/// The facts a country sets: its currency, its first zone and its customer languages.
fn target_facts(p: &Pack) -> [(&'static str, Value); 4] {
    [
        (COUNTRY_FACTS[0], json!(p.id)),
        (COUNTRY_FACTS[1], json!(p.currency.code)),
        (COUNTRY_FACTS[2], json!(p.zones.first())),
        (COUNTRY_FACTS[3], json!(p.locales)),
    ]
}

/// The diff card: each fact that would change, the saved integrations the target country does not offer (their
/// keys stay stored; the cards just stop being offered), and whether the change is allowed now.
pub(super) async fn get_preview(State(s): State<AppState>, Admin(_): Admin, Query(q): Query<PreviewQuery>) -> ApiResult<Json<Value>> {
    let target = load_country(&s.pack, &q.country)?;
    let mut changes = Vec::new();
    for (key, to) in target_facts(&target) {
        let from = fact(&s, key).await?;
        if from != to {
            changes.push(json!({ "key": key.trim_start_matches("install."), "from": from, "to": to }));
        }
    }
    let saved: Vec<String> = sqlx::query_scalar("SELECT section FROM integration_settings ORDER BY section")
        .fetch_all(&s.db)
        .await?;
    let unavailable: Vec<&str> = adapters::ADAPTERS
        .iter()
        .filter(|a| !a.section.is_empty() && saved.iter().any(|x| x == a.section))
        .filter(|a| !a.countries.iter().any(|c| *c == "*" || *c == target.id))
        .map(|a| a.id)
        .collect();
    let locked = is_set(&fact(&s, "install.locked_at").await?);
    Ok(Json(json!({
        "country": pack_card(&target),
        "changes": changes,
        "unavailable_adapters": unavailable,
        "allowed": !locked,
        "reason": locked.then_some("country_locked"),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CountryReq {
    country: String,
    /// The owner ticked "Check with a local lawyer before taking payments" for a draft pack.
    #[serde(default)]
    draft_ack: bool,
}

/// The owner confirms the country. Choosing a new one resets currency, zone and customer languages to the pack's;
/// confirming the current one keeps them. Refused once the install took a live payment (`install.locked_at`).
pub(super) async fn put_country(State(s): State<AppState>, Admin(a): Admin, Json(req): Json<CountryReq>) -> ApiResult<Json<Value>> {
    if !owner::is_owner(&s.db, a.id).await? {
        return Err(ApiError::Forbidden);
    }
    let target = load_country(&s.pack, &req.country)?;
    let mut tx = s.db.begin().await?;
    // Serialises against the first live payment (`install::lock_on_payment` inserts the lock row).
    sqlx::query("LOCK TABLE settings IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    let locked: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(storage_key("install.locked_at"))
        .fetch_optional(&mut *tx)
        .await?;
    if locked.as_ref().is_some_and(is_set) {
        return Err(ApiError::Conflict("country_locked"));
    }
    let before = fact(&s, "install.country").await?;
    let changed = before != json!(target.id);
    if changed {
        for (key, value) in target_facts(&target) {
            put_fact(&mut tx, key, &value).await?;
        }
        // A new country's draft acknowledgement starts over.
        sqlx::query("DELETE FROM settings WHERE key = $1")
            .bind(storage_key("install.draft_ack"))
            .execute(&mut *tx)
            .await?;
    }
    put_fact(&mut tx, "install.pack_version_applied", &json!(target.version)).await?;
    if req.draft_ack && target.maturity.level == MaturityLevel::Draft {
        put_fact(&mut tx, "install.draft_ack", &json!(chrono::Utc::now().to_rfc3339())).await?;
    }
    let data = json!({ "from": before, "to": target.id, "pack_version": target.version, "draft_ack": req.draft_ack });
    audit(&mut tx, a.id, "install.country", data).await?;
    tx.commit().await?;
    get_packs(State(s), Admin(a)).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MarketplaceReq {
    model: String,
}

/// "How you sell": a model the pack allows. A split model is accepted even without a split-capable gateway yet;
/// the `marketplace_model` gate keeps the site in preview until one is set up.
pub(super) async fn put_marketplace(State(s): State<AppState>, Admin(a): Admin, Json(req): Json<MarketplaceReq>) -> ApiResult<Json<Value>> {
    if !owner::is_owner(&s.db, a.id).await? {
        return Err(ApiError::Forbidden);
    }
    if !s.pack.payments.marketplace_models.contains(&req.model) {
        return Err(ApiError::BadRequest("invalid_marketplace_model"));
    }
    let mut tx = s.db.begin().await?;
    put_fact(&mut tx, "install.marketplace_model", &json!(req.model)).await?;
    audit(&mut tx, a.id, "install.marketplace_model", json!({ "model": req.model })).await?;
    tx.commit().await?;
    get_gates(State(s), Admin(a)).await
}

pub(super) async fn get_gates(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Value>> {
    let (gates, enforced) = golive::report(&s).await?;
    Ok(Json(json!({
        "preview": golive::preview(&gates),
        "enforced": enforced,
        "blocking": golive::blocking(&gates),
        "gates": gates,
    })))
}

/// The public preview flag: true while an enforced install has a red hard gate. False for installs from before
/// the country flow, so their website never changes.
pub async fn public_preview(s: &AppState) -> ApiResult<bool> {
    if !golive::enforced(s).await? {
        return Ok(false);
    }
    Ok(golive::preview(&golive::report(s).await?.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_locale_has_a_card_with_its_own_name() {
        for code in SUPPORTED_LOCALES {
            let c = locale_card(code);
            assert_ne!(c["name"], code, "{code} needs its own name");
            assert_eq!(c["machine"], code == "ar");
            assert_eq!(c["rtl"], code == "ar");
        }
    }

    #[test]
    fn the_kr_card_offers_only_available_adapters() {
        let kr = pack::load("KR").expect("KR");
        let card = pack_card(&kr);
        assert_eq!(card["currency"], "KRW");
        assert_eq!(card["draft"], false);
        assert_eq!(card["recommended"]["payments"], json!(["portone", "tosspayments"]));
        let offered = adapters::offerable_codes();
        for list in card["recommended"].as_object().expect("rec").values() {
            for code in list.as_array().expect("list") {
                assert!(offered.contains(&code.as_str().expect("code")), "{code} is not offered");
            }
        }
        assert_eq!(target_facts(&kr)[2].1, json!("Asia/Seoul"));
    }

    #[test]
    fn unknown_countries_are_refused_before_loading() {
        let kr = pack::load("KR").expect("KR");
        assert!(matches!(load_country(&kr, "XX"), Err(ApiError::BadRequest("unknown_country"))));
        assert!(matches!(load_country(&kr, "_base"), Err(ApiError::BadRequest("unknown_country"))));
        assert!(load_country(&kr, "KR").is_ok());
    }
}
