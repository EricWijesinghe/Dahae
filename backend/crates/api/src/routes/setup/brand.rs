// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Brand profile v2: the extra name fields, the image slots the console derives from the logo, the brand version
//! every response carries (`X-Brand-Version`), `GET /v1/brand` and the PWA manifests built from the profile.

use super::{business, clean_text};
use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::io::AsyncReadExt;

/// Image slots: key, width, height. The console makes each one; the server checks the PNG header size.
pub const SLOTS: [(&str, u32, u32); 7] = [
    ("favicon_32", 32, 32),
    ("apple_180", 180, 180),
    ("icon_192", 192, 192),
    ("icon_512", 512, 512),
    ("maskable_512", 512, 512),
    ("og_1200", 1200, 630),
    ("store_1024", 1024, 1024),
];
/// The product's own name, used only before an owner saves a profile.
pub const DEFAULT_NAME: &str = "다해";
pub const DEFAULT_COLOR: &str = "#187A8F";
/// SMS prefix `[..]` and PG order names stay short; a longer name is cut to this many characters.
pub const SHORT_MAX: usize = 12;
/// Optional text fields and their caps (characters).
const EXTRA_TEXT: [(&str, usize); 4] = [("name_en", 60), ("short_name", SHORT_MAX), ("tagline", 60), ("tagline_en", 60)];
/// `brand_version`: hex digits of SHA-256 over the stored profile.
const VERSION_HEX: usize = 16;
/// PNG signature (8) + IHDR length (4) + "IHDR" (4) + width (4) + height (4).
const PNG_HEAD: usize = 24;
const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Same stored JSON, same value on every node. A new logo is a new file name, so a new version.
pub fn brand_version(business: &Value) -> String {
    let bytes = serde_json::to_vec(business).unwrap_or_default();
    hex::encode(Sha256::digest(&bytes))[..VERSION_HEX].to_string()
}

fn text<'a>(b: &'a Value, key: &str) -> Option<&'a str> {
    b[key].as_str().map(str::trim).filter(|s| !s.is_empty())
}

pub fn name_of(b: &Value) -> String {
    text(b, "name").unwrap_or(DEFAULT_NAME).to_owned()
}

/// The English name for `en` when the owner set one; the main name otherwise.
pub fn name_for(b: &Value, lang: &str) -> String {
    match (lang.starts_with("en"), text(b, "name_en")) {
        (true, Some(en)) => en.to_owned(),
        _ => name_of(b),
    }
}

/// `short_name`, or the name cut to [`SHORT_MAX`] characters.
pub fn short_of(b: &Value) -> String {
    text(b, "short_name").map_or_else(|| name_of(b).chars().take(SHORT_MAX).collect(), str::to_owned)
}

async fn profile(db: &PgPool) -> Value {
    business(db).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "brand lookup failed; using default brand");
        json!({})
    })
}

/// Name for texts in `lang` (`{brand}` in English notices gets `name_en`).
pub async fn brand_name_for(db: &PgPool, lang: &str) -> String {
    name_for(&profile(db).await, lang)
}

/// Short name for SMS prefixes and PG order names.
pub async fn short_brand(db: &PgPool) -> String {
    short_of(&profile(db).await)
}

/// The v2 fields of the profile, validated into `out`.
pub(super) fn clean_extra(base: &str, input: &Value, out: &mut Map<String, Value>) -> Result<(), ApiError> {
    for (k, cap) in EXTRA_TEXT {
        if let Some(t) = clean_text(input, k)? {
            if t.chars().count() > cap {
                return Err(ApiError::BadRequest("field_too_long"));
            }
            out.insert(k.into(), json!(t));
        }
    }
    match input.get("brand_assets") {
        None | Some(Value::Null) => {}
        Some(v) => {
            out.insert("brand_assets".into(), clean_assets(base, v)?);
        }
    }
    Ok(())
}

fn slot(key: &str) -> Option<(u32, u32)> {
    SLOTS.iter().find(|(k, _, _)| *k == key).map(|(_, w, h)| (*w, *h))
}

/// Slot keys only; each value one of our own PNG uploads, stored as `/files/<name>`.
fn clean_assets(base: &str, v: &Value) -> Result<Value, ApiError> {
    let bad = ApiError::BadRequest("invalid_brand_asset");
    let obj = v.as_object().ok_or(ApiError::BadRequest("invalid_brand_asset"))?;
    let mut out = Map::new();
    for (k, url) in obj {
        if slot(k).is_none() {
            return Err(bad);
        }
        let path = url
            .as_str()
            .and_then(|u| crate::util::own_upload(base, u.trim()))
            .filter(|p| p.ends_with(".png"))
            .ok_or(ApiError::BadRequest("invalid_brand_asset"))?;
        out.insert(k.clone(), json!(path));
    }
    Ok(Value::Object(out))
}

/// Width and height from a PNG's IHDR chunk (bytes 16-24); None when the bytes are not a PNG header.
pub fn png_size(head: &[u8]) -> Option<(u32, u32)> {
    if head.len() < PNG_HEAD || head[..8] != PNG_SIG || &head[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
    Some((be(16), be(20)))
}

async fn file_size(s: &AppState, name: &str) -> Option<(u32, u32)> {
    let mut f = tokio::fs::File::open(s.config.upload_dir.join(name)).await.ok()?;
    let mut head = [0u8; PNG_HEAD];
    f.read_exact(&mut head).await.ok()?;
    png_size(&head)
}

/// Every slot file was uploaded here (an `uploads` row) and has exactly the slot's size.
pub(super) async fn check_assets(s: &AppState, profile: &Value) -> ApiResult<()> {
    let Some(assets) = profile["brand_assets"].as_object() else {
        return Ok(());
    };
    let paths: Vec<String> = assets.values().filter_map(Value::as_str).map(String::from).collect();
    if !super::uploads::owned(&s.db, None, &paths).await? {
        return Err(ApiError::BadRequest("invalid_brand_asset"));
    }
    for (k, path) in assets {
        let name = path.as_str().unwrap_or_default().trim_start_matches("/files/");
        if file_size(s, name).await != slot(k) {
            return Err(ApiError::BadRequest("invalid_brand_asset"));
        }
    }
    Ok(())
}

/// Public, unsigned URL of a stored brand file (`/files/x` → `{base}/v1/files/x`); https links stay as they are.
pub fn public_url(base: &str, stored: &str) -> String {
    match stored.strip_prefix("/files/") {
        Some(name) => format!("{}/v1/files/{name}", base.trim_end_matches('/')),
        None => stored.to_owned(),
    }
}

/// `assets` in `/v1/brand`: absolute public URLs, only the slots that are set (`logo` from `logo_url`).
pub fn assets(base: &str, b: &Value) -> Value {
    let mut out = Map::new();
    if let Some(l) = text(b, "logo_url") {
        out.insert("logo".into(), json!(public_url(base, l)));
    }
    for (k, _, _) in SLOTS {
        if let Some(p) = b["brand_assets"][k].as_str() {
            out.insert(k.into(), json!(public_url(base, p)));
        }
    }
    Value::Object(out)
}

/// Whether `name` is the current logo or a current slot file: only those are served without a signature.
pub fn is_brand_file(b: &Value, name: &str) -> bool {
    let path = format!("/files/{name}");
    b["logo_url"].as_str() == Some(path.as_str())
        || b["brand_assets"]
            .as_object()
            .is_some_and(|m| m.values().any(|v| v.as_str() == Some(path.as_str())))
}

/// `GET /v1/brand`: the stored profile (`logo_url` stays signed for installed apps), its version and public assets.
pub(super) async fn get_brand(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let b = business(&s.db).await?;
    let mut out = b.clone();
    if let Some(m) = out.as_object_mut() {
        m.insert("version".into(), json!(brand_version(&b)));
        m.insert("assets".into(), assets(&s.config.public_base_url, &b));
        // The install's languages, default first: the website's `lang`/`dir` follow them when the visitor's own
        // language has no copy. Empty until the country is set.
        let locales = crate::settings_registry::get_list(&s.db, &s.config, "install.locales").await?;
        m.insert("locales".into(), json!(locales));
    }
    Ok(Json(out))
}

/// One icon entry: the brand slot's public path when set, else the built-in file.
fn icon(b: &Value, slot_key: &str, fallback: &str, size: &str, purpose: Option<&str>) -> Value {
    let src = b["brand_assets"][slot_key].as_str().map_or_else(|| fallback.to_owned(), |p| public_url("", p));
    let mut v = json!({ "src": src, "sizes": size, "type": "image/png" });
    if let Some(p) = purpose {
        v["purpose"] = json!(p);
    }
    v
}

fn icons(b: &Value, dir: &str, maskable: bool) -> Vec<Value> {
    let (i192, i512) = if dir == "/assets" {
        ("/assets/icon-192.png".to_owned(), "/assets/icon-512.png".to_owned())
    } else {
        (format!("{dir}/icons/Icon-192.png"), format!("{dir}/icons/Icon-512.png"))
    };
    let mut out = vec![icon(b, "icon_192", &i192, "192x192", None), icon(b, "icon_512", &i512, "512x512", None)];
    if maskable {
        out.push(icon(
            b,
            "maskable_512",
            &format!("{dir}/icons/Icon-maskable-512.png"),
            "512x512",
            Some("maskable"),
        ));
    }
    out
}

/// The PWA manifest for `site`, `customer` or `console`: today's static fields, the name, colour and icons from
/// the profile. Ids stay fixed, so existing installs are not split.
pub fn manifest(app: &str, b: &Value) -> Option<Value> {
    let (name, short) = (name_of(b), short_of(b));
    let color = text(b, "primary_color").unwrap_or(DEFAULT_COLOR);
    let tagline = text(b, "tagline");
    let mut m = match app {
        "site" => json!({
            "name": name, "short_name": short, "lang": "ko", "start_url": "/", "display": "browser",
            "description": tagline.unwrap_or("검증된 청소 전문가, 투명한 가격, 1분 예약."),
            "icons": icons(b, "/assets", false),
        }),
        "customer" => json!({
            "name": name, "short_name": short, "id": "/app/", "start_url": "/app/", "scope": "/app/",
            "display": "standalone", "lang": "ko", "orientation": "portrait-primary", "prefer_related_applications": false,
            "description": tagline.unwrap_or("검증된 청소 전문가를 1분 만에 예약하세요."),
            "icons": icons(b, "/app", true),
        }),
        "console" => json!({
            "name": format!("{name} 관리자"), "short_name": format!("{short} 관리"), "id": "malkkeum-console",
            "start_url": "/admin/", "display": "standalone", "lang": "ko", "orientation": "any",
            "prefer_related_applications": false, "description": format!("{name} 운영 콘솔 — 예약, 상담, 정산, 설정"),
            "icons": icons(b, "/admin", true),
        }),
        _ => return None,
    };
    m["background_color"] = json!("#0B1217");
    m["theme_color"] = json!(color);
    Some(m)
}

/// `GET /v1/brand/manifest/{site|customer|console}` (Caddy maps the manifest paths here).
pub(super) async fn get_manifest(State(s): State<AppState>, Path(app): Path<String>) -> ApiResult<Response> {
    let b = super::super::seller::cached_business(&s.db).await?;
    let m = manifest(&app, &b).ok_or(ApiError::NotFound)?;
    Ok((
        [(header::CONTENT_TYPE, "application/manifest+json"), (header::CACHE_CONTROL, "no-cache")],
        m.to_string(),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://127.0.0.1:8080";
    const PNG: &str = "/files/0123456789abcdef0123456789abcdef.png";

    fn clean(v: Value) -> Result<Map<String, Value>, ApiError> {
        let mut out = Map::new();
        clean_extra(BASE, &v, &mut out).map(|()| out)
    }

    #[test]
    fn new_text_fields_are_trimmed_and_capped() {
        let out = clean(json!({"name_en": " Dahae ", "short_name": "다해", "tagline": " 집안일 ", "tagline_en": ""})).unwrap();
        assert_eq!(out["name_en"], "Dahae");
        assert_eq!(out["tagline"], "집안일");
        assert!(!out.contains_key("tagline_en"), "blank is dropped");
        let err = |v: Value| clean(v).unwrap_err().to_string();
        assert!(err(json!({"short_name": "가".repeat(SHORT_MAX + 1)})).contains("field_too_long"));
        assert!(err(json!({"name_en": "x".repeat(61)})).contains("field_too_long"));
        assert!(err(json!({"tagline": 3})).contains("invalid_input"));
    }

    #[test]
    fn short_name_defaults_to_the_name_cut_to_twelve() {
        assert_eq!(short_of(&json!({"name": "아주아주아주긴청소서비스이름입니다"})), "아주아주아주긴청소서비스");
        assert_eq!(short_of(&json!({"name": "긴 이름", "short_name": "짧게"})), "짧게");
        assert_eq!(short_of(&json!({})), DEFAULT_NAME);
    }

    #[test]
    fn english_name_only_for_english() {
        let b = json!({"name": "반짝", "name_en": "Banjjak"});
        assert_eq!(name_for(&b, "en"), "Banjjak");
        assert_eq!(name_for(&b, "ko"), "반짝");
        assert_eq!(name_for(&json!({"name": "반짝"}), "en"), "반짝");
    }

    #[test]
    fn brand_assets_take_only_known_slots_and_own_pngs() {
        let out = clean(json!({"brand_assets": {"icon_192": format!("{BASE}/v1{PNG}")}})).unwrap();
        assert_eq!(out["brand_assets"]["icon_192"], PNG);
        let err = |v: Value| clean(json!({ "brand_assets": v })).unwrap_err().to_string();
        assert!(err(json!({"wallpaper": PNG})).contains("invalid_brand_asset"), "unknown slot");
        assert!(
            err(json!({"icon_192": "https://evil.example/a.png"})).contains("invalid_brand_asset"),
            "foreign URL"
        );
        assert!(
            err(json!({"icon_192": PNG.replace(".png", ".jpg")})).contains("invalid_brand_asset"),
            "not a PNG"
        );
        assert!(err(json!(["x"])).contains("invalid_brand_asset"));
    }

    fn header(w: u32, h: u32) -> Vec<u8> {
        let mut v = PNG_SIG.to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v
    }

    #[test]
    fn png_size_reads_the_ihdr_header() {
        assert_eq!(png_size(&header(1200, 630)), Some((1200, 630)));
        assert_eq!(png_size(&header(32, 32)[..20]), None, "short");
        let mut jpeg = header(32, 32);
        jpeg[0] = 0xFF;
        assert_eq!(png_size(&jpeg), None);
    }

    #[test]
    fn version_is_stable_and_follows_every_change() {
        let a = json!({"name": "다해", "logo_url": PNG});
        assert_eq!(brand_version(&a), brand_version(&a.clone()));
        assert_eq!(brand_version(&a).len(), VERSION_HEX);
        let logo = json!({"name": "다해", "logo_url": PNG.replace("0123", "4567")});
        assert_ne!(brand_version(&a), brand_version(&logo), "a new logo file is a new version");
        assert_ne!(brand_version(&a), brand_version(&json!({"name": "다해!", "logo_url": PNG})));
    }

    #[test]
    fn assets_are_absolute_public_urls_of_set_slots_only() {
        let b = json!({"logo_url": PNG, "brand_assets": {"og_1200": PNG}});
        let a = assets(BASE, &b);
        assert_eq!(a["logo"], format!("{BASE}/v1{PNG}"));
        assert_eq!(a["og_1200"], format!("{BASE}/v1{PNG}"));
        assert!(a.get("icon_192").is_none());
        assert_eq!(
            assets(BASE, &json!({"logo_url": "https://cdn.example/l.png"}))["logo"],
            "https://cdn.example/l.png"
        );
    }

    #[test]
    fn only_current_brand_files_are_public() {
        let b = json!({"logo_url": PNG, "brand_assets": {"icon_512": "/files/ffffffffffffffffffffffffffffffff.png"}});
        assert!(is_brand_file(&b, "0123456789abcdef0123456789abcdef.png"));
        assert!(is_brand_file(&b, "ffffffffffffffffffffffffffffffff.png"));
        assert!(!is_brand_file(&b, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee.png"));
        assert!(!is_brand_file(&json!({}), "0123456789abcdef0123456789abcdef.png"));
    }

    #[test]
    fn manifests_follow_the_profile_and_fall_back_to_built_in_icons() {
        let plain = manifest("customer", &json!({})).unwrap();
        assert_eq!(plain["name"], DEFAULT_NAME);
        assert_eq!(plain["id"], "/app/");
        assert_eq!(plain["theme_color"], DEFAULT_COLOR);
        assert_eq!(plain["icons"][0]["src"], "/app/icons/Icon-192.png");
        assert_eq!(plain["icons"][2]["purpose"], "maskable");
        let b = json!({"name": "반짝", "short_name": "반짝", "primary_color": "#112233", "tagline": "깨끗하게",
                       "brand_assets": {"icon_192": PNG, "maskable_512": PNG}});
        let c = manifest("console", &b).unwrap();
        assert_eq!(c["name"], "반짝 관리자");
        assert_eq!(c["id"], "malkkeum-console");
        assert_eq!(c["theme_color"], "#112233");
        assert_eq!(c["icons"][0]["src"], format!("/v1{PNG}"));
        assert_eq!(c["icons"][1]["src"], "/admin/icons/Icon-512.png");
        assert_eq!(c["icons"][2]["src"], format!("/v1{PNG}"));
        let site = manifest("site", &b).unwrap();
        assert_eq!(site["description"], "깨끗하게");
        assert_eq!(site["icons"].as_array().map(Vec::len), Some(2));
        assert!(manifest("partner", &b).is_none());
    }
}
