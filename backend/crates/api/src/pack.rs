// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Country packs: data-only JSON under `backend/packs/`, embedded at build time so they ride the signed image.
//! A pack names a parent with `extends`; [`load`] deep-merges along that chain (objects merge, everything else
//! is replaced), reads the result into the typed [`Pack`] (unknown fields refused) and validates it. The server
//! refuses to boot on an invalid pack. Ids (adapters, validators, reports) refer to Rust code; URLs, wire formats
//! and money logic never live in a pack.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};

/// The pack this build runs. shortcut: R0 ships only KR and the code is KR; choose by `install.country` once a
/// second country pack ships.
pub const ACTIVE: &str = "KR";

/// Every embedded file, by its path under `backend/packs/`.
pub const EMBEDDED: &[(&str, &str)] = &[
    ("_base/pack.json", include_str!("../../../packs/_base/pack.json")),
    ("KR/pack.json", include_str!("../../../packs/KR/pack.json")),
    ("KR/areas.json", include_str!("../../../packs/KR/areas.json")),
    ("KR/banks.json", include_str!("../../../packs/KR/banks.json")),
];

/// Ids a pack may name, each backed by code in this build. Adapter ids come from the registry (`adapters`): the
/// codes of `Available` adapters.
pub const KNOWN_VALIDATORS: &[&str] = &["kr_brn", "kr_rrn"];
pub const KNOWN_REPORTS: &[&str] = &["kr_vat_quarter", "kr_tax_invoice", "kr_withholding", "kr_payroll"];
pub const KNOWN_LEGAL: &[&str] = &["terms", "privacy", "location_terms"];
pub const KNOWN_CHANNELS: &[&str] = &["alimtalk", "sms"];
pub const KNOWN_MARKETPLACE_MODELS: &[&str] = &["merchant_of_record", "split_by_psp"];
/// The only keys whose string values may be URLs; they are shown to the owner, never fetched (D4).
const URL_KEYS: &[&str] = &["why_url", "url", "keys_url", "signup_url"];
const MAX_BPS: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("country pack {pack}: {msg}")]
pub struct PackError {
    pub pack: String,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub id: String,
    pub version: String,
    pub name: String,
    pub maturity: Maturity,
    pub currency: Currency,
    pub zones: Vec<String>,
    pub locales: Vec<String>,
    pub week_start: WeekStart,
    #[serde(default)]
    pub holidays: Option<String>,
    pub tax: Tax,
    pub business_fields: Vec<BusinessField>,
    pub identifiers: Vec<Identifier>,
    pub payments: Payments,
    pub payouts: Payouts,
    pub messaging: Messaging,
    pub login: Login,
    pub address: Address,
    pub privacy: Privacy,
    pub consumer: Consumer,
    pub legal: Vec<String>,
    pub checklist: Vec<ChecklistItem>,
    /// Setup-wizard links per adapter id, shown on its card (`/setup/catalog`), never fetched.
    #[serde(default)]
    pub docs: BTreeMap<String, AdapterDocs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maturity {
    pub level: MaturityLevel,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaturityLevel {
    Draft,
    Reviewed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Currency {
    pub code: String,
    pub minor_units: u8,
    pub display_unit: i64,
    pub cash_unit: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekStart {
    Mon,
    Sat,
    Sun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tax {
    pub price_display: PriceDisplay,
    pub rounding: Rounding,
    pub rates: Vec<TaxRate>,
    #[serde(default)]
    pub withholding: Option<WithholdingRule>,
    pub reports: Vec<String>,
    #[serde(default)]
    pub cash_receipt_threshold: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceDisplay {
    Gross,
    Net,
}

/// The tax rule types are the domain's, so pack values feed `tax::rate_on`, `split_inclusive` and `withhold` as is.
pub use domain::tax::{Rounding, TaxRate, WithholdingBase, WithholdingRule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessField {
    pub key: String,
    #[serde(default)]
    pub validator: Option<String>,
    pub required: bool,
    #[serde(default)]
    pub shown_in: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identifier {
    pub id: String,
    #[serde(default)]
    pub legacy: Option<String>,
    pub validator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payments {
    pub recommended: Vec<String>,
    pub marketplace_models: Vec<String>,
    pub requires_split_for: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payouts {
    pub recommended: Vec<String>,
    #[serde(default)]
    pub banks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Messaging {
    pub channel_order: Vec<String>,
    pub recommended: Vec<String>,
    /// Advertising blocked from the first time to the second ("HH:MM", local time).
    #[serde(default)]
    pub quiet_hours: Option<[String; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    pub recommended: Vec<String>,
    pub oidc_issuers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub adapter: String,
    #[serde(default)]
    pub fallback: Option<String>,
    pub postcode_first: bool,
    #[serde(default)]
    pub areas: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Privacy {
    pub law: String,
    pub consent_model: ConsentModel,
    pub consent_version: String,
    pub consent_kinds: Vec<ConsentKind>,
    pub signup_required: Vec<String>,
    #[serde(default)]
    pub min_age: Option<u8>,
    #[serde(default)]
    pub dsr_deadline_days: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentModel {
    OptIn,
    OptOut,
}

/// One consent kind and whether the service cannot run without it once signed up.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentKind {
    pub kind: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consumer {
    pub withdrawal_days: Option<u16>,
    /// The built-in cancellation policy, in `domain::cancel_policy::CancelPolicy` form.
    pub cancel_default: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistItem {
    pub id: String,
    pub blocks_live: bool,
    #[serde(default)]
    pub why_url: Option<String>,
}

/// Where the owner gets an adapter's keys and opens an account. Display only (D4); `https` only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterDocs {
    #[serde(default)]
    pub keys_url: Option<String>,
    #[serde(default)]
    pub signup_url: Option<String>,
}

/// One entry of an areas file: a district code and its names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub code: String,
    pub name: String,
    pub name_en: String,
}

/// One entry of a banks file: the payout bank code and the name fragments that map to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bank {
    pub code: String,
    pub aliases: Vec<String>,
}

fn err(pack: &str, msg: impl Into<String>) -> PackError {
    PackError {
        pack: pack.to_string(),
        msg: msg.into(),
    }
}

fn file<'a>(files: &[(&str, &'a str)], path: &str) -> Option<&'a str> {
    files.iter().find(|(p, _)| *p == path).map(|(_, body)| *body)
}

fn parse_json(id: &str, files: &[(&str, &str)], path: &str) -> Result<Value, PackError> {
    let body = file(files, path).ok_or_else(|| err(id, format!("missing file {path}")))?;
    serde_json::from_str(body).map_err(|e| err(id, format!("{path}: {e}")))
}

/// Objects merge key by key; any other value in `over` replaces the one in `base`.
fn merge(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (k, v) in o {
                let merged = match b.remove(&k) {
                    Some(old) => merge(old, v),
                    None => v,
                };
                b.insert(k, merged);
            }
            Value::Object(b)
        }
        (_, over) => over,
    }
}

/// The pack's JSON with every parent merged under it, `extends` removed. Refuses cycles and deep chains.
fn merged(id: &str, files: &[(&str, &str)]) -> Result<Value, PackError> {
    let mut chain: Vec<Map<String, Value>> = Vec::new();
    let mut next = Some(id.to_string());
    while let Some(cur) = next {
        if chain.len() >= 4 {
            return Err(err(id, "extends chain too deep or cyclic"));
        }
        let Value::Object(mut obj) = parse_json(id, files, &format!("{cur}/pack.json"))? else {
            return Err(err(id, format!("{cur}/pack.json is not an object")));
        };
        if obj.get("id").and_then(Value::as_str) != Some(cur.as_str()) {
            return Err(err(id, format!("{cur}/pack.json: id does not match its folder")));
        }
        next = match obj.remove("extends") {
            None => None,
            Some(Value::String(parent)) => Some(parent),
            Some(_) => return Err(err(id, "extends must be a pack id")),
        };
        chain.push(obj);
    }
    Ok(chain.into_iter().rev().fold(Value::Object(Map::new()), |acc, o| merge(acc, Value::Object(o))))
}

/// Loads, merges and validates one embedded pack.
/// The rate category that VAT on services is filed under.
pub const VAT_CATEGORY: &str = "service";

impl Pack {
    /// VAT in basis points in force on `day`; None when the pack has no VAT rate for that day.
    pub fn vat_bps(&self, day: NaiveDate) -> Option<i64> {
        domain::tax::rate_on(&self.tax.rates, VAT_CATEGORY, day).map(|r| i64::from(r.rate_bps))
    }

    /// The first day of each VAT rate piece of `[from, to]`: `from`, then every day inside where a rate starts or
    /// ends. One rate applies to every day of a piece.
    pub fn vat_pieces(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        let mut starts: Vec<NaiveDate> = self
            .tax
            .rates
            .iter()
            .filter(|r| r.category == VAT_CATEGORY)
            .flat_map(|r| [Some(r.from), r.until])
            .flatten()
            .filter(|d| from < *d && *d <= to)
            .chain([from])
            .collect();
        starts.sort();
        starts.dedup();
        starts
    }
}

pub fn load(id: &str) -> Result<Pack, PackError> {
    load_from(id, EMBEDDED)
}

/// Every country pack in the build (folders not starting with `_`), each validated.
pub fn load_all() -> Result<Vec<Pack>, PackError> {
    country_ids(EMBEDDED).into_iter().map(load).collect()
}

/// The country ids among `files`: folders with a `pack.json` whose name does not start with `_`.
pub fn country_ids<'a>(files: &[(&'a str, &str)]) -> Vec<&'a str> {
    files
        .iter()
        .filter_map(|(p, _)| p.strip_suffix("/pack.json"))
        .filter(|id| !id.starts_with('_'))
        .collect()
}

/// [`load`] over any file set, so tests can feed broken packs.
pub fn load_from(id: &str, files: &[(&str, &str)]) -> Result<Pack, PackError> {
    let value = merged(id, files)?;
    check_no_urls(id, &value, "")?;
    let pack: Pack = serde_json::from_value(value).map_err(|e| err(id, e.to_string()))?;
    validate(&pack, files)?;
    Ok(pack)
}

/// The areas file a pack names, parsed.
pub fn areas(pack: &Pack, files: &[(&str, &str)]) -> Result<Vec<Area>, PackError> {
    side_file(pack, files, pack.address.areas.as_deref())
}

/// The banks file a pack names, parsed.
pub fn banks(pack: &Pack, files: &[(&str, &str)]) -> Result<Vec<Bank>, PackError> {
    side_file(pack, files, pack.payouts.banks.as_deref())
}

fn side_file<T: serde::de::DeserializeOwned>(pack: &Pack, files: &[(&str, &str)], path: Option<&str>) -> Result<Vec<T>, PackError> {
    let Some(path) = path else { return Ok(Vec::new()) };
    serde_json::from_value(parse_json(&pack.id, files, path)?).map_err(|e| err(&pack.id, format!("{path}: {e}")))
}

/// D4: no string may look like a URL outside the display-only keys.
fn check_no_urls(id: &str, v: &Value, key: &str) -> Result<(), PackError> {
    match v {
        Value::String(s) if !URL_KEYS.contains(&key) && (s.contains("://") || s.starts_with("www.")) => {
            Err(err(id, format!("{key}: URLs belong in code, not in a pack")))
        }
        Value::Array(items) => items.iter().try_for_each(|i| check_no_urls(id, i, key)),
        Value::Object(map) => map.iter().try_for_each(|(k, i)| check_no_urls(id, i, k)),
        _ => Ok(()),
    }
}

/// `YYYY.MM.N` as numbers, for ordering pack versions. None when malformed.
pub fn version_key(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
    let key = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(key)
}

fn known(id: &str, what: &str, items: &[String], allowed: &[&str]) -> Result<(), PackError> {
    match items.iter().find(|i| !allowed.contains(&i.as_str())) {
        Some(bad) => Err(err(id, format!("unknown {what} id {bad}"))),
        None => Ok(()),
    }
}

fn validate(p: &Pack, files: &[(&str, &str)]) -> Result<(), PackError> {
    let id = p.id.as_str();
    let upper = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_uppercase());
    let ensure = |ok: bool, msg: &str| if ok { Ok(()) } else { Err(err(id, msg)) };
    ensure(upper(id, 2), "id must be an ISO 3166-1 alpha-2 code")?;
    ensure(version_key(&p.version).is_some(), "version must be YYYY.MM.N")?;
    ensure(upper(&p.currency.code, 3), "currency code must be ISO 4217")?;
    ensure(
        p.currency.minor_units <= 3 && p.currency.display_unit >= 1 && p.currency.cash_unit >= 1,
        "currency units out of range",
    )?;
    ensure(!p.zones.is_empty() && !p.locales.is_empty(), "zones and locales must not be empty")?;
    ensure(p.locales.iter().all(|l| valid_locale(l)), "locale must look like ko or pt-BR")?;
    ensure(p.zones.iter().all(|z| z.contains('/')), "zone must be an IANA name")?;
    check_rates(id, &p.tax.rates)?;
    if let Some(w) = &p.tax.withholding {
        ensure(w.rate_bps <= MAX_BPS && w.local_surtax_bps <= MAX_BPS, "withholding rate above 100%")?;
        ensure(w.tax_unit >= 1 && w.surtax_unit >= 1, "withholding units must be at least 1")?;
    }
    ensure(
        p.tax.cash_receipt_threshold.is_none_or(|t| t >= 0),
        "cash receipt threshold must not be negative",
    )?;
    known(id, "report", &p.tax.reports, KNOWN_REPORTS)?;
    let validators: Vec<String> = p
        .business_fields
        .iter()
        .filter_map(|f| f.validator.clone())
        .chain(p.identifiers.iter().map(|i| i.validator.clone()))
        .collect();
    known(id, "validator", &validators, KNOWN_VALIDATORS)?;
    let mut adapters: Vec<String> = [&p.payments.recommended, &p.payouts.recommended, &p.messaging.recommended, &p.login.recommended]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    adapters.push(p.address.adapter.clone());
    adapters.extend(p.address.fallback.clone());
    known(id, "adapter", &adapters, &crate::adapters::offerable_codes())?;
    known(id, "channel", &p.messaging.channel_order, KNOWN_CHANNELS)?;
    known(id, "marketplace model", &p.payments.marketplace_models, KNOWN_MARKETPLACE_MODELS)?;
    known(id, "legal document", &p.legal, KNOWN_LEGAL)?;
    check_docs(p)?;
    if let Some([from, to]) = &p.messaging.quiet_hours {
        ensure(hhmm(from).is_some() && hhmm(to).is_some(), "quiet hours must be HH:MM")?;
    }
    check_consents(p)?;
    let policy: domain::cancel_policy::CancelPolicy =
        serde_json::from_value(p.consumer.cancel_default.clone()).map_err(|e| err(id, format!("cancel_default: {e}")))?;
    policy.validate().map_err(|e| err(id, format!("cancel_default: {e}")))?;
    ensure(areas(p, files)?.iter().all(|a| !a.code.is_empty()), "area without a code")?;
    ensure(
        banks(p, files)?.iter().all(|b| !b.code.is_empty() && !b.aliases.is_empty()),
        "bank without a code or name",
    )?;
    if let Some(h) = &p.holidays {
        parse_json(id, files, h)?;
    }
    Ok(())
}

/// Docs links name a registry adapter and open over https: the console renders them as links.
fn check_docs(p: &Pack) -> Result<(), PackError> {
    for (adapter, d) in &p.docs {
        if crate::adapters::spec(adapter).is_none() {
            return Err(err(&p.id, format!("docs: unknown adapter id {adapter}")));
        }
        if [&d.keys_url, &d.signup_url].into_iter().flatten().any(|u| !u.starts_with("https://")) {
            return Err(err(&p.id, format!("docs.{adapter}: links must start with https://")));
        }
    }
    Ok(())
}

fn valid_locale(l: &str) -> bool {
    let (lang, region) = l.split_once('-').map_or((l, None), |(a, b)| (a, Some(b)));
    lang.len() == 2 && lang.bytes().all(|b| b.is_ascii_lowercase()) && region.is_none_or(|r| r.len() == 2 && r.bytes().all(|b| b.is_ascii_uppercase()))
}

/// "HH:MM" as (hour, minute).
pub fn hhmm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    let (h, m) = (h.parse().ok()?, m.parse().ok()?);
    (s.len() == 5 && h < 24 && m < 60).then_some((h, m))
}

/// Each category's periods are well formed and never overlap, so a day maps to at most one rate, and the
/// VAT category is present.
fn check_rates(id: &str, rates: &[TaxRate]) -> Result<(), PackError> {
    let mut by_cat: BTreeMap<&str, Vec<&TaxRate>> = BTreeMap::new();
    for r in rates {
        if r.rate_bps > MAX_BPS || r.until.is_some_and(|u| u <= r.from) {
            return Err(err(id, format!("tax rate {} is malformed", r.category)));
        }
        by_cat.entry(r.category.as_str()).or_default().push(r);
    }
    if !by_cat.contains_key(VAT_CATEGORY) {
        return Err(err(id, format!("tax rates need a {VAT_CATEGORY} rate (the VAT estimate's rate)")));
    }
    for (cat, mut list) in by_cat {
        list.sort_by_key(|r| r.from);
        if list.windows(2).any(|w| w[0].until.is_none_or(|u| u > w[1].from)) {
            return Err(err(id, format!("tax rate periods overlap for {cat}")));
        }
    }
    Ok(())
}

fn check_consents(p: &Pack) -> Result<(), PackError> {
    let mut seen = HashSet::new();
    for k in &p.privacy.consent_kinds {
        let ok = (1..=40).contains(&k.kind.len()) && k.kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
        if !ok || !seen.insert(k.kind.as_str()) {
            return Err(err(&p.id, format!("consent kind {} is malformed or repeated", k.kind)));
        }
    }
    match p.privacy.signup_required.iter().find(|k| !seen.contains(k.as_str())) {
        Some(k) => Err(err(&p.id, format!("signup requires unknown consent kind {k}"))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_keeps_parent_keys_and_replaces_arrays() {
        let out = merge(json!({"a": {"x": 1, "y": [1, 2]}, "b": 1}), json!({"a": {"y": [3]}, "c": 2}));
        assert_eq!(out, json!({"a": {"x": 1, "y": [3]}, "b": 1, "c": 2}));
    }

    #[test]
    fn versions_order_numerically() {
        assert!(version_key("2026.11.0") < version_key("2026.11.10"));
        assert!(version_key("2026.9.0") < version_key("2026.11.0"));
        assert_eq!(version_key("2026.11"), None);
        assert_eq!(version_key("2026.11.0.1"), None);
        assert_eq!(version_key("v1.2.3"), None);
    }

    #[test]
    fn quiet_hours_parse_strictly() {
        assert_eq!(hhmm("21:00"), Some((21, 0)));
        assert_eq!(hhmm("8:00"), None);
        assert_eq!(hhmm("24:00"), None);
    }
}
