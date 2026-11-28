// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `GET /admin/setup/catalog`: every adapter this install may offer, grouped by capability, the pack's recommended
//! ones first. Each entry carries its card state, the fields to fill (required, set or not) and the pack's docs
//! links. No stored text is ever part of it, masked or not: the console's generic adapter card shows set/unset only.

use super::{card, gateway_section};
use crate::{
    adapters::{self, AdapterSpec, Capability},
    auth::Admin,
    error::ApiResult,
    integrations::{self, LockState, Section},
    pack::Pack,
    state::AppState,
};
use axum::{Json, extract::State};
use serde_json::{Value, json};

pub(super) async fn get_catalog(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Value>> {
    let base = s.config.public_base_url.trim_end_matches('/');
    let mut sections = Vec::new();
    for cap in Capability::ALL {
        let mut entries = Vec::new();
        for spec in offered(&s.pack, cap) {
            entries.push(entry(&s, base, spec).await?);
        }
        if !entries.is_empty() {
            sections.push(json!({ "capability": cap.code(), "adapters": entries }));
        }
    }
    Ok(Json(json!({ "country": s.pack.id, "pack_version": s.pack.version, "sections": sections })))
}

/// The pack's recommendations for a capability, in its order.
pub(super) fn recommended(p: &Pack, cap: Capability) -> Vec<&str> {
    let list: Vec<&String> = match cap {
        Capability::Payments => p.payments.recommended.iter().collect(),
        Capability::Payouts => p.payouts.recommended.iter().collect(),
        Capability::Messaging => p.messaging.recommended.iter().collect(),
        Capability::Login => p.login.recommended.iter().collect(),
        Capability::Address => std::iter::once(&p.address.adapter).chain(&p.address.fallback).collect(),
        Capability::Invoice | Capability::Identity => Vec::new(),
    };
    list.into_iter().map(String::as_str).collect()
}

/// `Available` adapters of `cap` that serve the pack's country: recommended first (pack order), then registry order.
fn offered(p: &Pack, cap: Capability) -> Vec<&'static AdapterSpec> {
    let rec = recommended(p, cap);
    let mut out: Vec<&'static AdapterSpec> = adapters::ADAPTERS
        .iter()
        .filter(|a| a.capability == cap && a.writable() && a.countries.iter().any(|c| *c == "*" || *c == p.id))
        .collect();
    out.sort_by_key(|a| rec.iter().position(|r| *r == a.id).unwrap_or(rec.len()));
    out
}

async fn entry(s: &AppState, base: &str, spec: &'static AdapterSpec) -> ApiResult<Value> {
    let docs = s.pack.docs.get(spec.id);
    let mut e = json!({
        "id": spec.id,
        "name": if spec.name.is_empty() { spec.id } else { spec.name },
        "section": spec.section,
        "recommended": recommended(&s.pack, spec.capability).contains(&spec.id),
        "caps": spec.caps.names(),
        "webhook_url": spec.webhook.map(|_| format!("{base}/v1/webhooks/{}", spec.id)),
        "docs": { "keys_url": docs.and_then(|d| d.keys_url.clone()), "signup_url": docs.and_then(|d| d.signup_url.clone()) },
    });
    let state = match Section::parse(spec.section) {
        // Nothing to enter (manual payouts): ready as shipped.
        None => json!({ "configured": true, "source": "builtin", "locked": false, "mode": null, "owner_code": false, "fields": [] }),
        Some(sec) => state(s, sec).await?,
    };
    if let (Some(e), Value::Object(st)) = (e.as_object_mut(), state) {
        e.extend(st);
    }
    Ok(e)
}

/// The section's card without any stored value: fields keep name, kind, required and set. A switch also keeps
/// its position (`value`, a boolean that is never a secret), so the card can show it.
async fn state(s: &AppState, sec: Section) -> ApiResult<Value> {
    let c = card(s, sec, None).await?;
    let fields: Vec<Value> = c["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let mut out = json!({ "name": f["name"], "kind": f["kind"], "required": f["required"], "set": f["set"] });
            if f["kind"] == "toggle" {
                out["value"] = json!(f["preview"].as_bool().unwrap_or(false));
            }
            out
        })
        .collect();
    // The first save of a gateway card needs the owner's code (`put_section`).
    let owner_code = gateway_section(sec) && integrations::lock_state(&s.db, sec).await? == LockState::Unsaved;
    Ok(json!({
        "configured": c["configured"],
        "source": c["source"],
        "locked": c["locked"],
        "mode": c["mode"],
        "owner_code": owner_code,
        "fields": fields,
    }))
}
