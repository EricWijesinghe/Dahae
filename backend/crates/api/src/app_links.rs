// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Android App Links and Apple Universal Links for the Naver login callback (`https://DOMAIN/auth/naver/callback`).
//! The OS fetches these files from the site before it hands that URL to the app. No values = 404, so the link is not verified.
//! Values saved in the console (settings key `app_links`) replace the env values (`ANDROID_CERT_SHA256`, `APPLE_TEAM_ID`) as a set.
//! The app ids are the licensee's store apps; empty lists mean the built-in ids, so the files stay as they were.

use crate::{
    auth::{Admin, Staff},
    config,
    error::{ApiError, ApiResult},
    routes::audit,
    state::AppState,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Built-in store app ids (app/apps/customer, app/apps/partner): Android applicationId and iOS bundle identifier.
/// Store builds take others from the GitHub variables `CUSTOMER_APP_ID`/`PARTNER_APP_ID`.
const APP_IDS: [&str; 2] = ["kr.co.dahae.customer", "kr.co.dahae.partner"];
/// Customer, partner, and a spare for a renamed or second app.
const MAX_APP_IDS: usize = 4;
/// Naver login returns here. Must match the Android intent-filter, the iOS applinks entry and social_native_io.dart.
pub const NAVER_CALLBACK_PATH: &str = "/auth/naver/callback";
const SETTINGS_KEY: &str = "app_links";
/// Debug, upload, Play app signing and one rotation spare fit comfortably.
const MAX_CERTS: usize = 8;

/// Link values. The same shape is the stored settings row, the admin API body and the audit payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Links {
    #[serde(default)]
    pub android_cert_sha256: Vec<String>,
    #[serde(default)]
    pub apple_team_id: Option<String>,
    /// Android applicationIds in assetlinks.json. Empty = the built-in ids.
    #[serde(default)]
    pub android_packages: Vec<String>,
    /// iOS bundle ids in apple-app-site-association. Empty = the built-in ids.
    #[serde(default)]
    pub ios_bundle_ids: Vec<String>,
}

impl Links {
    fn android_ids(&self) -> Vec<&str> {
        ids_or_default(&self.android_packages)
    }

    fn ios_ids(&self) -> Vec<&str> {
        ids_or_default(&self.ios_bundle_ids)
    }
}

fn ids_or_default(ids: &[String]) -> Vec<&str> {
    if ids.is_empty() {
        APP_IDS.to_vec()
    } else {
        ids.iter().map(String::as_str).collect()
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/.well-known/assetlinks.json", get(assetlinks))
        .route("/.well-known/apple-app-site-association", get(apple_app_site_association))
}

/// Owner console (mounted under `/admin`): read, save and dry-run the link values.
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/app-links", get(get_links).put(put_links))
        .route("/app-links/verify", post(verify_links))
}

/// Saved values when the owner has saved any, otherwise the env values. Also returns where they came from.
pub async fn current(s: &AppState) -> ApiResult<(Links, &'static str)> {
    let stored: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(SETTINGS_KEY)
        .fetch_optional(&s.db)
        .await?;
    match stored {
        Some(v) => {
            let links: Links = serde_json::from_value(v).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            Ok((links, "database"))
        }
        None => {
            let links = Links {
                android_cert_sha256: s.config.android_cert_sha256.clone(),
                apple_team_id: s.config.apple_team_id.clone(),
                ..Links::default()
            };
            Ok((links, "environment"))
        }
    }
}

async fn assetlinks(State(s): State<AppState>) -> ApiResult<Response> {
    let (links, _) = current(&s).await?;
    if links.android_cert_sha256.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    Ok(Json(assetlinks_document(&links.android_cert_sha256, &links.android_ids())).into_response())
}

async fn apple_app_site_association(State(s): State<AppState>) -> ApiResult<Response> {
    let (links, _) = current(&s).await?;
    Ok(match &links.apple_team_id {
        Some(team) => Json(aasa_document(team, &links.ios_ids())).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

async fn get_links(State(s): State<AppState>, _st: Staff) -> ApiResult<Json<Value>> {
    let (links, source) = current(&s).await?;
    Ok(Json(preview(&links, source)))
}

/// Owner-only. Validated before it replaces the values; the audit row keeps the fingerprints (public values).
async fn put_links(State(s): State<AppState>, Admin(a): Admin, Json(input): Json<Links>) -> ApiResult<Json<Value>> {
    let links = clean(input).map_err(ApiError::BadRequest)?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = now()")
        .bind(SETTINGS_KEY)
        .bind(sqlx::types::Json(&links))
        .execute(&s.db)
        .await?;
    audit(&s.db, "staff", Some(a.id), "app_links.update", "app_links", json!(links)).await?;
    Ok(Json(preview(&links, "database")))
}

/// Dry run of the save: checks the values and shows the two files that would be served. Saves nothing.
async fn verify_links(_a: Admin, Json(input): Json<Links>) -> ApiResult<Json<Value>> {
    let links = clean(input).map_err(ApiError::BadRequest)?;
    Ok(Json(preview(&links, "unsaved")))
}

/// Checks and normalises owner input with the env rules (config.rs): SHA-256 fingerprints and 10-character Team IDs.
/// A blank Team ID means unset. Returns an error code for `ApiError::BadRequest`.
pub fn clean(input: Links) -> Result<Links, &'static str> {
    if input.android_cert_sha256.len() > MAX_CERTS {
        return Err("app_link_too_many_certs");
    }
    let raw: Vec<String> = input.android_cert_sha256.iter().map(|c| c.trim().to_owned()).collect();
    let mut certs = config::cert_fingerprints("ANDROID_CERT_SHA256", raw).map_err(|_| "app_link_fingerprint")?;
    certs.sort();
    certs.dedup();
    let team = input.apple_team_id.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    let apple_team_id = config::team_id(team).map_err(|_| "app_link_team_id")?;
    Ok(Links {
        android_cert_sha256: certs,
        apple_team_id,
        android_packages: clean_ids(input.android_packages, false)?,
        ios_bundle_ids: clean_ids(input.ios_bundle_ids, true)?,
    })
}

/// Trimmed, deduplicated in order, at most four. Android: dot-separated segments that start with a letter and
/// hold letters, digits and `_`. iOS bundle ids also allow `-`. At least two segments, 150 characters at most.
fn clean_ids(input: Vec<String>, ios: bool) -> Result<Vec<String>, &'static str> {
    if input.len() > MAX_APP_IDS {
        return Err("app_link_too_many_ids");
    }
    let mut out: Vec<String> = Vec::new();
    for id in input.iter().map(|i| i.trim()) {
        if !valid_app_id(id, ios) {
            return Err("app_link_app_id");
        }
        if !out.iter().any(|o| o == id) {
            out.push(id.to_owned());
        }
    }
    Ok(out)
}

fn valid_app_id(id: &str, ios: bool) -> bool {
    let segment =
        |s: &str| s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || (ios && c == '-'));
    id.len() <= 150 && id.split('.').count() >= 2 && id.split('.').all(segment)
}

/// What the two well-known files would contain, so the console can show it before the owner saves.
pub fn preview(links: &Links, source: &'static str) -> Value {
    json!({
        "android_cert_sha256": links.android_cert_sha256,
        "apple_team_id": links.apple_team_id,
        "android_packages": links.android_packages,
        "ios_bundle_ids": links.ios_bundle_ids,
        "source": source,
        "assetlinks": (!links.android_cert_sha256.is_empty()).then(|| assetlinks_document(&links.android_cert_sha256, &links.android_ids())),
        "aasa": links.apple_team_id.as_deref().map(|t| aasa_document(t, &links.ios_ids())),
    })
}

/// Digital Asset Links, one statement per app: https://developers.google.com/digital-asset-links/v1/getting-started
pub fn assetlinks_document(certs: &[String], app_ids: &[&str]) -> Value {
    Value::Array(
        app_ids
            .iter()
            .map(|id| {
                json!({
                    "relation": ["delegate_permission/common.handle_all_urls"],
                    "target": { "namespace": "android_app", "package_name": id, "sha256_cert_fingerprints": certs },
                })
            })
            .collect(),
    )
}

/// Apple App Site Association: https://developer.apple.com/documentation/bundleresources/applinks
pub fn aasa_document(team: &str, bundle_ids: &[&str]) -> Value {
    let app_ids: Vec<String> = bundle_ids.iter().map(|id| format!("{team}.{id}")).collect();
    // webcredentials: ASWebAuthenticationSession (iOS 17.4+) only accepts an https callback for a domain the app
    // is associated with through this service, applinks alone is not enough.
    json!({
        "applinks": {
            "details": [{ "appIDs": app_ids, "components": [{ "/": NAVER_CALLBACK_PATH, "comment": "Naver login" }] }]
        },
        "webcredentials": { "apps": app_ids }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(byte: &str) -> String {
        vec![byte; 32].join(":")
    }

    #[test]
    fn assetlinks_names_both_apps_and_the_signing_certs() {
        let certs = vec!["AA:BB".to_string()];
        let doc = assetlinks_document(&certs, &APP_IDS);
        let statements = doc.as_array().expect("array");
        assert_eq!(statements.len(), APP_IDS.len());
        assert_eq!(statements[0]["target"]["package_name"], "kr.co.dahae.customer");
        assert_eq!(statements[1]["target"]["package_name"], "kr.co.dahae.partner");
        assert_eq!(statements[1]["target"]["sha256_cert_fingerprints"], json!(["AA:BB"]));
        assert_eq!(statements[0]["relation"][0], "delegate_permission/common.handle_all_urls");
    }

    #[test]
    fn aasa_limits_the_link_to_the_naver_path_and_both_app_ids() {
        let doc = aasa_document("ABCDE12345", &APP_IDS);
        let detail = &doc["applinks"]["details"][0];
        assert_eq!(detail["appIDs"], json!(["ABCDE12345.kr.co.dahae.customer", "ABCDE12345.kr.co.dahae.partner"]));
        assert_eq!(detail["components"][0]["/"], NAVER_CALLBACK_PATH);
        assert_eq!(doc["webcredentials"]["apps"], detail["appIDs"]);
    }

    #[test]
    fn clean_uppercases_trims_and_dedupes_fingerprints() {
        let lower = fingerprint("ab");
        let input = Links {
            android_cert_sha256: vec![format!(" {lower} "), lower.to_uppercase()],
            apple_team_id: None,
            ..Links::default()
        };
        let out = clean(input).expect("valid");
        assert_eq!(out.android_cert_sha256, vec![fingerprint("AB")]);
    }

    #[test]
    fn clean_refuses_a_malformed_fingerprint() {
        let short = Links {
            android_cert_sha256: vec!["AA:BB".into()],
            apple_team_id: None,
            ..Links::default()
        };
        assert_eq!(clean(short), Err("app_link_fingerprint"));
        let bad_hex = Links {
            android_cert_sha256: vec![fingerprint("zz")],
            apple_team_id: None,
            ..Links::default()
        };
        assert_eq!(clean(bad_hex), Err("app_link_fingerprint"));
    }

    #[test]
    fn clean_treats_a_blank_team_id_as_unset_and_refuses_a_bad_one() {
        let blank = Links {
            android_cert_sha256: vec![],
            apple_team_id: Some("  ".into()),
            ..Links::default()
        };
        assert_eq!(clean(blank).expect("unset").apple_team_id, None);
        let short = Links {
            android_cert_sha256: vec![],
            apple_team_id: Some("SHORT".into()),
            ..Links::default()
        };
        assert_eq!(clean(short), Err("app_link_team_id"));
        let punct = Links {
            android_cert_sha256: vec![],
            apple_team_id: Some("ABCDE-1234".into()),
            ..Links::default()
        };
        assert_eq!(clean(punct), Err("app_link_team_id"));
    }

    #[test]
    fn clean_caps_the_number_of_certificates() {
        let many = Links {
            android_cert_sha256: (0..=MAX_CERTS).map(|i| fingerprint(&format!("{i:02X}"))).collect(),
            apple_team_id: None,
            ..Links::default()
        };
        assert_eq!(clean(many), Err("app_link_too_many_certs"));
    }

    #[test]
    fn default_ids_give_todays_files_byte_for_byte() {
        let links = Links {
            android_cert_sha256: vec!["AA:BB".into()],
            apple_team_id: Some("ABCDE12345".into()),
            ..Links::default()
        };
        let p = preview(&links, "unsaved");
        assert_eq!(
            p["assetlinks"].to_string(),
            r#"[{"relation":["delegate_permission/common.handle_all_urls"],"target":{"namespace":"android_app","package_name":"kr.co.dahae.customer","sha256_cert_fingerprints":["AA:BB"]}},{"relation":["delegate_permission/common.handle_all_urls"],"target":{"namespace":"android_app","package_name":"kr.co.dahae.partner","sha256_cert_fingerprints":["AA:BB"]}}]"#
        );
        assert_eq!(
            p["aasa"].to_string(),
            r#"{"applinks":{"details":[{"appIDs":["ABCDE12345.kr.co.dahae.customer","ABCDE12345.kr.co.dahae.partner"],"components":[{"/":"/auth/naver/callback","comment":"Naver login"}]}]},"webcredentials":{"apps":["ABCDE12345.kr.co.dahae.customer","ABCDE12345.kr.co.dahae.partner"]}}"#
        );
    }

    #[test]
    fn custom_ids_replace_the_built_in_ones() {
        let links = clean(Links {
            android_cert_sha256: vec![],
            apple_team_id: Some("ABCDE12345".into()),
            android_packages: vec![" com.shop.app ".into(), "com.shop.app".into(), "com.shop.partner".into()],
            ios_bundle_ids: vec!["com.shop.my-app".into()],
        })
        .expect("valid");
        assert_eq!(links.android_packages, vec!["com.shop.app", "com.shop.partner"]);
        let doc = assetlinks_document(&["AA".into()], &links.android_ids());
        assert_eq!(doc[1]["target"]["package_name"], "com.shop.partner");
        let aasa = aasa_document("ABCDE12345", &links.ios_ids());
        assert_eq!(aasa["webcredentials"]["apps"], json!(["ABCDE12345.com.shop.my-app"]));
    }

    #[test]
    fn clean_refuses_bad_app_ids() {
        let cases: [(&[&str], &[&str]); 4] = [(&["noDots"], &[]), (&["com.1shop"], &[]), (&["com.my-app"], &[]), (&[], &["com.a b"])];
        for (android, ios) in cases {
            let input = Links {
                android_packages: android.iter().map(|s| s.to_string()).collect(),
                ios_bundle_ids: ios.iter().map(|s| s.to_string()).collect(),
                ..Links::default()
            };
            assert_eq!(clean(input), Err("app_link_app_id"), "{android:?} {ios:?}");
        }
        let many = Links {
            android_packages: (0..=MAX_APP_IDS).map(|i| format!("com.shop.a{i}")).collect(),
            ..Links::default()
        };
        assert_eq!(clean(many), Err("app_link_too_many_ids"));
    }

    #[test]
    fn preview_leaves_out_a_document_with_no_values() {
        let empty = preview(&Links::default(), "unsaved");
        assert!(empty["assetlinks"].is_null());
        assert!(empty["aasa"].is_null());
        assert_eq!(empty["source"], "unsaved");
    }
}
