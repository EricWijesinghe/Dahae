// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Country packs: every embedded pack validates, broken packs are refused, and each constant the KR pack
//! mirrors equals its pack value. A constant may only be deleted (and the code read the pack) while this is green.

use api::{
    integrations::Section,
    marketing, pack,
    pack::{EMBEDDED, Pack, PackError},
    privacy,
    providers::payout,
    seo,
    settings_registry::{COUNTRIES, CURRENCIES, LOCALES, ZONES},
};
use domain::{cancel_policy, cash_receipt, currency, finance, money, tax};
use serde_json::{Value, json};

fn kr() -> Pack {
    pack::load("KR").expect("KR pack validates")
}

/// The embedded files with `KR/pack.json` replaced by `kr` after `edit`.
fn broken(edit: impl FnOnce(&mut Value)) -> Result<Pack, PackError> {
    let mut kr: Value = serde_json::from_str(EMBEDDED.iter().find(|(p, _)| *p == "KR/pack.json").expect("KR").1).expect("json");
    edit(&mut kr);
    let body = kr.to_string();
    let files: Vec<(&str, &str)> = EMBEDDED
        .iter()
        .map(|(p, b)| if *p == "KR/pack.json" { (*p, body.as_str()) } else { (*p, *b) })
        .collect();
    pack::load_from("KR", &files)
}

fn refused(edit: impl FnOnce(&mut Value), needle: &str) {
    let e = broken(edit).expect_err("pack must be refused");
    assert!(e.to_string().contains(needle), "{e} should mention {needle}");
}

#[test]
fn every_embedded_country_pack_validates() {
    let packs = pack::load_all().expect("all packs");
    assert_eq!(packs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["KR"]);
    assert!(pack::load(pack::ACTIVE).is_ok());
}

#[test]
fn registry_choices_cover_every_pack() {
    for p in pack::load_all().expect("all packs") {
        assert!(COUNTRIES.contains(&p.id.as_str()), "{}", p.id);
        assert!(CURRENCIES.contains(&p.currency.code.as_str()), "{}", p.id);
        assert!(p.zones.iter().all(|z| ZONES.contains(&z.as_str())), "{}", p.id);
        assert!(p.locales.iter().all(|l| LOCALES.contains(&l.as_str())), "{}", p.id);
    }
}

#[test]
fn kr_inherits_base_and_overrides_it() {
    let p = kr();
    assert_eq!(p.week_start, pack::WeekStart::Mon, "from _base");
    assert_eq!(p.locales, ["ko", "en"], "KR replaces the base list");
    assert_eq!(p.tax.rounding, pack::Rounding::Floor);
}

#[test]
fn kr_mirrors_money_and_tax_constants() {
    let p = kr();
    assert_eq!(p.currency.code, "KRW");
    assert_eq!(p.currency.minor_units, 0);
    assert_eq!(p.currency.display_unit, money::ROUNDING_UNIT);
    let vat = p.tax.rates.iter().find(|r| r.category == "service").expect("service VAT");
    assert_eq!(i64::from(vat.rate_bps), finance::VAT_BPS);
    assert!(vat.until.is_none());
    let w = p.tax.withholding.as_ref().expect("withholding");
    assert_eq!(i64::from(w.rate_bps), tax::BUSINESS_INCOME_TAX_BPS);
    assert_eq!(w.base, pack::WithholdingBase::Gross);
    // Surtax rate and both rounding units live inline in business_withholding; prove them on its outputs.
    for gross in (0..=2_000_000).step_by(997) {
        let out = tax::business_withholding(domain::Won(gross));
        let income = gross * i64::from(w.rate_bps) / 10_000 / w.tax_unit * w.tax_unit;
        let local = income * i64::from(w.local_surtax_bps) / 10_000 / w.surtax_unit * w.surtax_unit;
        assert_eq!((out.income_tax.0, out.local_tax.0), (income, local), "gross {gross}");
    }
    assert_eq!(p.tax.cash_receipt_threshold, Some(cash_receipt::MANDATORY_ISSUE_MIN.0));
}

#[test]
fn kr_pack_feeds_the_generic_money_and_tax_rules() {
    let p = kr();
    let c = &p.currency;
    let k = currency::KRW;
    assert_eq!(
        (c.code.as_str(), c.minor_units, c.display_unit, c.cash_unit),
        (k.code, k.minor_units, k.display_unit, k.cash_unit)
    );
    assert_eq!(p.tax.withholding, Some(tax::KR_BUSINESS_WITHHOLDING));
    assert_eq!(p.tax.rates, [tax::kr_vat()]);
    let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 27).expect("date");
    assert_eq!(p.vat_bps(today), Some(finance::VAT_BPS));
    assert_eq!(
        p.vat_bps(chrono::NaiveDate::from_ymd_opt(1977, 6, 30).expect("date")),
        None,
        "before KR VAT began"
    );
}

/// Frozen: older apps send this version and every existing customer agreed to it, so neither the constant nor the
/// pack value may ever change; new texts move consents through the owner's Publish.
#[test]
fn the_agreed_consent_version_is_frozen() {
    assert_eq!(privacy::CONSENT_VERSION, "2026-11-17");
    assert_eq!(kr().privacy.consent_version, "2026-11-17");
    refused(|v| v["privacy"]["consent_version"] = json!("v1"), "privacy.consent_version must be");
    refused(|v| v["privacy"]["consent_version"] = json!("2026-11-31"), "privacy.consent_version must be");
}

#[test]
fn kr_mirrors_consent_constants() {
    let p = kr();
    assert_eq!(p.privacy.consent_version, privacy::CONSENT_VERSION);
    let kinds: Vec<(&str, bool)> = p.privacy.consent_kinds.iter().map(|k| (k.kind.as_str(), k.required)).collect();
    assert_eq!(kinds, privacy::CONSENTS);
    let docs: Vec<(&str, &str)> = p.privacy.consent_kinds.iter().map(|k| (k.kind.as_str(), k.doc.as_str())).collect();
    assert_eq!(docs, privacy::CONSENT_DOCS);
    assert_eq!(p.locales[0], privacy::LEGAL_LANG, "consent versions follow the pack's first-locale texts");

    assert_eq!(p.privacy.signup_required, privacy::SIGNUP_REQUIRED);
}

#[test]
fn kr_mirrors_the_cancel_default_and_quiet_hours() {
    let p = kr();
    let builtin: Value = serde_json::from_str(cancel_policy::DEFAULT_JSON).expect("default policy");
    assert_eq!(p.consumer.cancel_default, builtin);
    let [start, end] = p.messaging.quiet_hours.as_ref().expect("quiet hours");
    assert_eq!(pack::hhmm(start), Some((marketing::QUIET_START_HOUR, 0)));
    assert_eq!(pack::hhmm(end), Some((marketing::QUIET_END_HOUR, 0)));
}

#[test]
fn kr_mirrors_areas_banks_and_footer_fields() {
    let p = kr();
    let areas: Vec<(String, String, String)> = pack::areas(&p, EMBEDDED)
        .expect("areas")
        .into_iter()
        .map(|a| (a.code, a.name, a.name_en))
        .collect();
    let districts: Vec<(String, String, String)> = seo::SEOUL_DISTRICTS.iter().map(|(c, n, e)| ((*c).into(), (*n).into(), (*e).into())).collect();
    assert_eq!(areas, districts);
    let banks: Vec<(String, Vec<String>)> = pack::banks(&p, EMBEDDED).expect("banks").into_iter().map(|b| (b.code, b.aliases)).collect();
    let table: Vec<(String, Vec<String>)> = payout::BANKS
        .iter()
        .map(|(c, a)| ((*c).into(), a.iter().map(|s| (*s).into()).collect()))
        .collect();
    assert_eq!(banks, table);
    let keys: Vec<&str> = p.business_fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, seo::COMPANY_LABELS.map(|(k, _)| k));
}

#[test]
fn kr_recommended_order_follows_todays_console_cards() {
    let p = kr();
    let codes: Vec<&str> = Section::ALL.iter().map(|s| s.code()).collect();
    for list in [&p.payments.recommended, &p.messaging.recommended, &p.login.recommended] {
        let pos: Vec<usize> = list.iter().map(|id| codes.iter().position(|c| c == id).expect("a console card")).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]), "{list:?} out of console order");
    }
    assert_eq!(p.payments.recommended.first().map(String::as_str), Some("portone"), "PortOne stays first");
    assert_eq!(p.address.adapter, Section::Juso.code());
}

#[test]
fn urls_are_refused_outside_display_keys() {
    refused(|v| v["name"] = json!("https://example.com"), "URLs belong in code");
    refused(|v| v["legal"] = json!(["www.example.com"]), "URLs belong in code");
    let ok = broken(|v| v["checklist"][0]["why_url"] = json!("https://example.com/why"));
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn docs_links_name_registry_adapters_and_open_over_https() {
    assert_eq!(kr().docs["portone"].keys_url.as_deref(), Some("https://admin.portone.io"));
    refused(
        |v| v["docs"]["stripe"] = json!({"keys_url": "https://stripe.example"}),
        "docs: unknown adapter id stripe",
    );
    refused(
        |v| v["docs"]["portone"]["keys_url"] = json!("http://admin.portone.io"),
        "links must start with https://",
    );
    refused(
        |v| v["docs"]["portone"]["signup_url"] = json!("javascript:alert(1)"),
        "links must start with https://",
    );
    refused(|v| v["docs"]["portone"]["help"] = json!("x"), "unknown field");
}

#[test]
fn unknown_ids_and_fields_are_refused() {
    refused(|v| v["payments"]["recommended"] = json!(["stripe"]), "unknown adapter id stripe");
    refused(|v| v["tax"]["reports"] = json!(["gb_vat"]), "unknown report id gb_vat");
    refused(|v| v["identifiers"][0]["validator"] = json!("nope"), "unknown validator id nope");
    refused(|v| v["surprise"] = json!(1), "unknown field");
    refused(|v| v["tax"]["rounding"] = json!("banker"), "unknown variant");
}

#[test]
fn overlapping_or_malformed_values_are_refused() {
    refused(
        |v| {
            v["tax"]["rates"] =
                json!([{"category": "service", "rate_bps": 1000, "from": "2020-01-01"}, {"category": "service", "rate_bps": 800, "from": "2024-01-01"}])
        },
        "overlap",
    );
    let ok = broken(
        |v| v["tax"]["rates"] = json!([{"category": "service", "rate_bps": 1000, "from": "2020-01-01", "until": "2024-01-01"}, {"category": "service", "rate_bps": 800, "from": "2024-01-01"}]),
    );
    assert!(ok.is_ok(), "{ok:?}");
    refused(|v| v["tax"]["rates"][0]["rate_bps"] = json!(10_001), "malformed");
    refused(|v| v["tax"]["rates"][0]["category"] = json!("services"), "need a service rate");
    refused(|v| v["tax"]["rates"] = json!([]), "need a service rate");
    refused(|v| v["privacy"]["signup_required"] = json!(["terms", "ghost"]), "unknown consent kind ghost");
    refused(|v| v["consumer"]["cancel_default"]["tiers"] = json!([]), "cancel_default");
    refused(|v| v["version"] = json!("1.0"), "version");
    refused(|v| v["id"] = json!("XX"), "id does not match");
    refused(|v| v["payouts"]["banks"] = json!("KR/missing.json"), "missing file");
}

#[test]
fn extends_chains_must_resolve() {
    refused(|v| v["extends"] = json!("EU"), "missing file EU/pack.json");
    refused(|v| v["extends"] = json!("KR"), "too deep or cyclic");
}

/// The embedded files with one file's text replaced (or removed when `text` is None).
fn with_file(path: &str, text: Option<&str>) -> Result<Pack, PackError> {
    let files: Vec<(&str, &str)> = EMBEDDED
        .iter()
        .filter_map(|(p, b)| if *p == path { text.map(|t| (*p, t)) } else { Some((*p, *b)) })
        .collect();
    pack::load_from("KR", &files)
}

fn kr_text(path: &str) -> &'static str {
    EMBEDDED.iter().find(|(p, _)| *p == path).expect("embedded").1
}

#[test]
fn every_kr_legal_document_has_a_korean_template_and_every_consent_names_one() {
    let docs: Vec<String> = api::legal::templates(&kr(), EMBEDDED)
        .expect("templates")
        .iter()
        .map(|t| format!("{}.{}@{}", t.doc, t.lang, t.version))
        .collect();
    assert_eq!(docs, ["terms.ko@2026-11-17", "privacy.ko@2026-11-17", "location_terms.ko@2026-11-17"]);
    refused(|p| p["privacy"]["consent_kinds"][3]["doc"] = json!("cookies"), "not in legal");
}

#[test]
fn broken_legal_templates_are_refused() {
    let terms = kr_text("KR/legal/terms.ko.md");
    let check = |text: Option<&str>, needle: &str| {
        let e = with_file("KR/legal/terms.ko.md", text).expect_err("pack must be refused");
        assert!(e.to_string().contains(needle), "{e} should mention {needle}");
    };
    check(None, "has no ko text");
    check(Some(&terms.replace("{seller.ceo}", "{seller.shoe_size}")), "unknown slot {seller.shoe_size}");
    check(Some(&terms.replace("{pack.privacy.min_age}", "{pack.privacy}")), "unknown slot {pack.privacy}");
    check(Some(&terms.replace("{seller.ceo}", "{seller.ceo")), "terms.ko.md");
    check(Some(&terms.replace("version: 2026-11-17", "version: v2")), "version must be");
    check(Some(&terms.replace("doc: terms", "doc: privacy")), "does not match");
    let install = terms.replace("{seller.ceo}", "{install.currency}");
    assert!(with_file("KR/legal/terms.ko.md", Some(&install)).is_ok(), "install facts are slots");
}

/// A template past `privacy.consent_version` keeps the agreed text beside it (what the baseline records and the website
/// shows until the owner publishes); a pack without it, or with it at another version, is refused.
#[test]
fn a_template_past_the_agreed_version_keeps_the_agreed_text() {
    let terms = kr_text("KR/legal/terms.ko.md");
    let newer = format!(
        "{}<p>개정</p>
",
        terms.replace("version: 2026-11-17", "version: 2026-12-01")
    );
    let e = with_file("KR/legal/terms.ko.md", Some(&newer)).expect_err("needs the agreed text");
    assert!(e.to_string().contains("legal/terms.ko.2026-11-17.md"), "{e}");
    let load = |agreed: &str| {
        let mut files: Vec<(&str, &str)> = EMBEDDED
            .iter()
            .map(|(p, b)| (*p, if *p == "KR/legal/terms.ko.md" { newer.as_str() } else { *b }))
            .collect();
        files.push(("KR/legal/terms.ko.2026-11-17.md", agreed));
        pack::load_from("KR", &files).and_then(|p| api::legal::agreed_templates(&p, &files))
    };
    let agreed = load(terms).expect("with the agreed text");
    let t = agreed.iter().find(|t| t.doc == "terms").expect("terms");
    let original = api::legal::parse("terms", "ko", terms).expect("terms");
    assert_eq!((t.version.as_str(), &t.body), ("2026-11-17", &original.body));
    let e = load(&newer).expect_err("the agreed text names the agreed version");
    assert!(e.to_string().contains("version must be privacy.consent_version"), "{e}");
    let today = api::legal::agreed_templates(&kr(), EMBEDDED).expect("agreed");
    assert!(
        today.iter().all(|t| t.version == privacy::CONSENT_VERSION),
        "today every template is the agreed text"
    );
}
