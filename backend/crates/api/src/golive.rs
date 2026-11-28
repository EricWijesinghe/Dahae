// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Go-live gates (D10). Hard gates block live checkout: a payment key still in test mode, a required legal document
//! unpublished, a required business field missing, a marketplace model the country does not allow (or one that
//! needs a split-capable gateway without one), and a draft pack nobody acknowledged. The pack's checklist items
//! (regulator registrations and the like) only warn. While any hard gate is red the site is in preview: test keys
//! keep taking test payments, and a checkout on live keys is refused with `site_in_preview`. Each red gate names
//! the wizard step that fixes it, so the console shows the fix, never a server error.
//!
//! Enforced only for an install that chose its country in the wizard (`PUT /admin/setup/country`). An install
//! that ran before the country flow (set to KR at first boot, or never asked) keeps working as it did; its gates
//! are still reported.

use crate::{
    adapters::{self, Caps},
    error::{ApiError, ApiResult},
    integrations,
    pack::{MaturityLevel, Pack},
    privacy::LEGAL_LANG,
    providers::Gateway,
    routes::setup::{BUSINESS_KEY, install::MARKER_KEY},
    settings_registry,
    state::AppState,
    util::LOCAL_TODAY,
};
use serde::Serialize;
use serde_json::Value;

/// The marketplace model an install that never answered "How you sell" runs: the owner's company sells.
pub const DEFAULT_MODEL: &str = "merchant_of_record";
/// The model where the pros sell, which needs the gateway to split the money (`Caps::SPLIT`).
pub const SPLIT_MODEL: &str = "split_by_psp";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gate {
    pub id: &'static str,
    /// Hard gates block live checkout; the rest only warn.
    pub hard: bool,
    pub ok: bool,
    /// The setup-wizard step that fixes it (§5 screen names).
    pub step: &'static str,
    /// What is missing, by id (legal documents, business fields, a checklist item), for the fix text.
    pub missing: Vec<String>,
}

/// The facts the gates are computed from, gathered once so the rules stay pure and testable.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub dev_mode: bool,
    /// Key mode per enabled payment gateway: `Some("test")`, `Some("live")`, or None when the keys carry no mode.
    pub gateways: Vec<(&'static str, Option<&'static str>)>,
    pub published_legal: Vec<String>,
    pub business: Value,
    pub marketplace_model: Option<String>,
    pub draft_ack: bool,
}

/// Every gate for `pack` over `f`, hard gates first, in wizard order.
pub fn gates(pack: &Pack, f: &Facts) -> Vec<Gate> {
    let gate = |id, hard, ok, step, missing: Vec<String>| Gate { id, hard, ok, step, missing };
    let draft = pack.maturity.level == MaturityLevel::Draft;
    let missing_fields: Vec<String> = pack
        .business_fields
        .iter()
        .filter(|b| b.required && f.business["company"][&b.key].as_str().is_none_or(|v| v.trim().is_empty()))
        .map(|b| b.key.clone())
        .collect();
    let model = f.marketplace_model.as_deref().unwrap_or(DEFAULT_MODEL);
    let split_ready = f.gateways.iter().any(|(code, _)| adapters::spec(code).is_some_and(|s| s.caps.has(Caps::SPLIT)));
    let model_ok = pack.payments.marketplace_models.iter().any(|m| m == model) && (model != SPLIT_MODEL || split_ready);
    let live_keys = !f.dev_mode && !f.gateways.is_empty() && f.gateways.iter().all(|(_, mode)| *mode != Some("test"));
    let unpublished: Vec<String> = pack.legal.iter().filter(|d| !f.published_legal.contains(d)).cloned().collect();
    let mut out = vec![
        gate("draft_pack_ack", true, !draft || f.draft_ack, "country", Vec::new()),
        gate("business_fields", true, missing_fields.is_empty(), "business", missing_fields),
        gate(
            "marketplace_model",
            true,
            model_ok,
            "how_you_sell",
            if model_ok { Vec::new() } else { vec![model.to_string()] },
        ),
        gate("payment_live_keys", true, live_keys, "payments", Vec::new()),
        gate("legal_published", true, unpublished.is_empty(), "legal", unpublished),
    ];
    out.extend(pack.checklist.iter().map(|c| gate("checklist", false, false, "review", vec![c.id.clone()])));
    out
}

/// The hard gates that block a checkout on live keys: every red hard gate except the test-mode one, which a live
/// key has already passed for its own gateway.
pub fn blocking(gates: &[Gate]) -> Vec<&'static str> {
    gates.iter().filter(|g| g.hard && !g.ok && g.id != "payment_live_keys").map(|g| g.id).collect()
}

/// Whether the site is in preview: any hard gate red.
pub fn preview(gates: &[Gate]) -> bool {
    gates.iter().any(|g| g.hard && !g.ok)
}

async fn facts(s: &AppState) -> ApiResult<Facts> {
    let enabled = crate::routes::payments::enabled_codes(&s.db).await?;
    let integ = s.providers.integ();
    let gateways = integ.payment_gateways(&enabled).into_iter().map(|g| (g.code(), gateway_mode(s, g))).collect();
    let published_legal: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT DISTINCT doc FROM legal_documents WHERE lang = $1 AND effective_at <= {LOCAL_TODAY}"
    ))
    .bind(LEGAL_LANG)
    .fetch_all(&s.db)
    .await?;
    let business: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(BUSINESS_KEY)
        .fetch_optional(&s.db)
        .await?;
    let text = |v: Value| v.as_str().filter(|t| !t.is_empty()).map(str::to_string);
    Ok(Facts {
        dev_mode: s.config.dev_mode,
        gateways,
        published_legal,
        business: business.unwrap_or(Value::Null),
        marketplace_model: text(settings_registry::get_value(&s.db, &s.config, "install.marketplace_model").await?),
        draft_ack: text(settings_registry::get_value(&s.db, &s.config, "install.draft_ack").await?).is_some(),
    })
}

/// A gateway's key mode: Toss keys say test or live by prefix; PortOne ids carry none. Dev mode is always test.
fn gateway_mode(s: &AppState, g: Gateway) -> Option<&'static str> {
    if s.config.dev_mode {
        return Some("test");
    }
    match g {
        Gateway::Portone => None,
        Gateway::TossPayments => s
            .providers
            .integ()
            .toss_payments
            .as_ref()
            .and_then(|t| integrations::toss_key_mode(&t.secret_key)),
    }
}

/// Whether the gates are enforced: the install chose its country in the wizard. One set to KR at first boot, or a
/// fresh install from before the country flow that never picked one, keeps its old behaviour.
pub async fn enforced(s: &AppState) -> ApiResult<bool> {
    let marker: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(MARKER_KEY)
        .fetch_optional(&s.db)
        .await?;
    let auto_country = marker.is_some_and(|m| m["auto_country"].is_string());
    let country = settings_registry::get_value(&s.db, &s.config, "install.country").await?;
    Ok(!auto_country && country.is_string())
}

/// The gate report the console's review screen and the public preview flag read.
pub async fn report(s: &AppState) -> ApiResult<(Vec<Gate>, bool)> {
    Ok((gates(&s.pack, &facts(s).await?), enforced(s).await?))
}

/// Refuses a checkout that would take live money while a hard gate is red (`site_in_preview`). Test keys and dev
/// mode pass: their payments move no money, which is what preview is for.
pub async fn require_live_ready(s: &AppState, gw: Gateway) -> ApiResult<()> {
    if gateway_mode(s, gw) == Some("test") || !enforced(s).await? {
        return Ok(());
    }
    let gates = gates(&s.pack, &facts(s).await?);
    let red = blocking(&gates);
    if red.is_empty() {
        return Ok(());
    }
    tracing::warn!(gates = ?red, "live checkout refused: the site is in preview");
    Err(ApiError::Conflict("site_in_preview"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn kr() -> Pack {
        crate::pack::load("KR").expect("KR")
    }

    fn green() -> Facts {
        Facts {
            dev_mode: false,
            gateways: vec![("portone", None)],
            published_legal: vec!["terms".into(), "privacy".into(), "location_terms".into()],
            business: json!({ "company": { "legal_name": "a", "ceo": "b", "registration_no": "c", "address": "d" } }),
            marketplace_model: None,
            draft_ack: false,
        }
    }

    fn red(g: &[Gate]) -> Vec<&'static str> {
        g.iter().filter(|g| g.hard && !g.ok).map(|g| g.id).collect()
    }

    #[test]
    fn a_complete_kr_install_is_live_and_its_checklist_only_warns() {
        let g = gates(&kr(), &green());
        assert!(red(&g).is_empty(), "{g:?}");
        assert!(!preview(&g));
        let warn: Vec<&Gate> = g.iter().filter(|g| !g.hard).collect();
        assert_eq!(warn.len(), 1);
        assert_eq!(warn[0].missing, ["kr_mail_order_report"]);
    }

    #[test]
    fn each_hard_gate_turns_red_alone_and_names_its_fix() {
        let pack = kr();
        let cases: Vec<(Facts, &str, &str)> = vec![
            (
                Facts {
                    gateways: vec![("tosspayments", Some("test"))],
                    ..green()
                },
                "payment_live_keys",
                "payments",
            ),
            (
                Facts {
                    gateways: Vec::new(),
                    ..green()
                },
                "payment_live_keys",
                "payments",
            ),
            (Facts { dev_mode: true, ..green() }, "payment_live_keys", "payments"),
            (
                Facts {
                    published_legal: vec!["terms".into()],
                    ..green()
                },
                "legal_published",
                "legal",
            ),
            (
                Facts {
                    business: json!({ "company": { "legal_name": "a" } }),
                    ..green()
                },
                "business_fields",
                "business",
            ),
            (
                Facts {
                    business: json!({ "company": { "legal_name": "a", "ceo": " ", "registration_no": "c", "address": "d" } }),
                    ..green()
                },
                "business_fields",
                "business",
            ),
            (
                Facts {
                    marketplace_model: Some(SPLIT_MODEL.into()),
                    ..green()
                },
                "marketplace_model",
                "how_you_sell",
            ),
        ];
        for (f, id, step) in cases {
            let g = gates(&pack, &f);
            assert_eq!(red(&g), [id], "{f:?}");
            assert!(preview(&g));
            assert_eq!(g.iter().find(|x| x.id == id).map(|x| x.step), Some(step));
        }
        let g = gates(
            &pack,
            &Facts {
                published_legal: vec!["terms".into()],
                ..green()
            },
        );
        assert_eq!(
            g.iter().find(|x| x.id == "legal_published").map(|x| x.missing.clone()),
            Some(vec!["privacy".to_string(), "location_terms".to_string()])
        );
        let g = gates(
            &pack,
            &Facts {
                business: json!({}),
                ..green()
            },
        );
        assert_eq!(
            g.iter().find(|x| x.id == "business_fields").map(|x| x.missing.len()),
            Some(4),
            "the four required KR fields"
        );
    }

    #[test]
    fn a_model_the_country_does_not_list_is_red() {
        let mut pack = kr();
        pack.payments.marketplace_models = vec![SPLIT_MODEL.into()];
        assert_eq!(
            red(&gates(&pack, &green())),
            ["marketplace_model"],
            "an unanswered install runs merchant_of_record"
        );
    }

    #[test]
    fn a_draft_pack_needs_the_owners_acknowledgement() {
        let mut pack = kr();
        pack.maturity.level = MaturityLevel::Draft;
        assert_eq!(red(&gates(&pack, &green())), ["draft_pack_ack"]);
        assert!(red(&gates(&pack, &Facts { draft_ack: true, ..green() })).is_empty());
    }

    #[test]
    fn live_checkout_is_blocked_by_every_red_hard_gate_but_test_mode() {
        let pack = kr();
        let test_only = gates(
            &pack,
            &Facts {
                gateways: vec![("tosspayments", Some("test"))],
                ..green()
            },
        );
        assert!(preview(&test_only));
        assert!(blocking(&test_only).is_empty(), "a live key on another gateway is not blocked by a test one");
        let unpublished = gates(
            &pack,
            &Facts {
                published_legal: Vec::new(),
                ..green()
            },
        );
        assert_eq!(blocking(&unpublished), ["legal_published"]);
    }
}
