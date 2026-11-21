// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Brand and live-setting fragments for the website and legal pages: name, colour, icons, JSON-LD, the owner's
//! cancellation rules, retention days, which processors are on, and the published console legal texts.
//! Everything is escaped here; Caddy pastes it into the page as it is.

use crate::{error::ApiResult, integrations::Section, routes::setup::brand, seo::escape_html, settings_registry, state::AppState};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Mutex, time::Instant};

const USED: &str = "이용 중";
const NOT_USED: &str = "현재 이용하지 않음";
const DEFAULT_FAVICON: &str = "/assets/favicon.svg";
const DEFAULT_APPLE: &str = "/assets/icon-192.png";
const DEFAULT_OG: &str = "/assets/og-image.png";
const DEFAULT_ICON: &str = "/assets/icon-512.png";
const SITE_DESCRIPTION: &str = "검증된 청소 전문가가 욕실부터 이사 청소까지. 앱에서 가격을 확인하고 1분 만에 예약하세요.";

/// Every field this module answers. The website contract test checks pages against `all_fields`.
#[cfg(test)]
pub const FIELDS: [&str; 26] = [
    "name",
    "name_en",
    "short_name",
    "base_url",
    "primary_color",
    "logo_img",
    "favicon",
    "apple_icon",
    "og_image",
    "ld_json",
    "cancel_policy",
    "ai_chat_retention_days",
    "upload_orphan_days",
    "uses_portone",
    "uses_toss",
    "uses_solapi",
    "uses_fcm",
    "uses_ai",
    "uses_backup",
    "uses_juso",
    "legal_terms",
    "legal_privacy",
    "legal_location",
    // Kept apart from the seller FIELDS: these read the profile, not `company`.
    "tagline",
    "tagline_en",
    "privacy_email",
];

/// Caddy renders included text as a template again, so a `{{` typed into the console must never reach it.
pub fn defuse(html: &str) -> String {
    html.replace('{', "&#123;").replace('}', "&#125;")
}

fn text<'a>(b: &'a Value, key: &str) -> Option<&'a str> {
    b[key].as_str().map(str::trim).filter(|s| !s.is_empty())
}

/// A brand file's public path (`/v1/files/x`), or the built-in default.
fn asset_path(b: &Value, slot: &str, fallback: &str) -> String {
    b["brand_assets"][slot]
        .as_str()
        .map_or_else(|| fallback.to_owned(), |p| brand::public_url("", p))
}

/// Fields that need only the profile and the base URL.
pub fn brand_fragment(base: &str, b: &Value, field: &str) -> Option<String> {
    let base = base.trim_end_matches('/');
    Some(match field {
        "name" => escape_html(&brand::name_of(b)),
        "name_en" => escape_html(&brand::name_for(b, "en")),
        "short_name" => escape_html(&brand::short_of(b)),
        "tagline" | "tagline_en" => escape_html(text(b, field).unwrap_or_default()),
        "base_url" => escape_html(base),
        "primary_color" => text(b, "primary_color")
            .filter(|c| crate::routes::setup::is_hex_color(c))
            .unwrap_or(brand::DEFAULT_COLOR)
            .to_owned(),
        "logo_img" => text(b, "logo_url")
            .map(|l| format!("<img src=\"{}\" alt=\"\">", escape_html(&brand::public_url(base, l))))
            .unwrap_or_default(),
        "favicon" => escape_html(&asset_path(b, "favicon_32", DEFAULT_FAVICON)),
        "apple_icon" => escape_html(&asset_path(b, "apple_180", DEFAULT_APPLE)),
        "og_image" => escape_html(&format!("{base}{}", asset_path(b, "og_1200", DEFAULT_OG))),
        "ld_json" => ld_json(base, b),
        "privacy_email" => escape_html(text(&b["company"], "privacy_email").unwrap_or("[개인정보 보호 이메일]")),
        _ => return None,
    })
}

/// The website's JSON-LD block (WebSite, LocalBusiness, Service) with absolute URLs and the logo.
pub fn ld_json(base: &str, b: &Value) -> String {
    let name = brand::name_of(b);
    let icon = format!("{base}{}", asset_path(b, "icon_512", DEFAULT_ICON));
    let mut local = json!({
        "@type": "LocalBusiness", "@id": format!("{base}/#business"), "name": name, "url": format!("{base}/"),
        "image": icon, "logo": icon, "description": SITE_DESCRIPTION,
        "areaServed": {"@type": "AdministrativeArea", "name": "서울특별시"}, "priceRange": "₩₩",
    });
    if let Some(p) = text(b, "support_phone") {
        local["telephone"] = json!(p);
    }
    let doc = json!({"@context": "https://schema.org", "@graph": [
        {"@type": "WebSite", "@id": format!("{base}/#website"), "name": name, "url": format!("{base}/"), "inLanguage": ["ko", "en"]},
        local,
        {"@type": "Service", "name": "홈클리닝", "serviceType": "가정 청소", "areaServed": {"@type": "AdministrativeArea", "name": "서울특별시"},
         "provider": {"@id": format!("{base}/#business")}, "url": format!("{base}/#services")},
    ]});
    // `<` cannot close the element; `{{` never occurs between JSON tokens, only inside strings, where `{` is the same text.
    let body = serde_json::to_string(&doc)
        .unwrap_or_default()
        .replace('<', "\\u003c")
        .replace("{{", "{\\u007b");
    format!("<script type=\"application/ld+json\">{body}</script>")
}

/// The cancellation rules as `<li>` rows, worded as the terms page has always worded them.
pub fn cancel_rows(policy: &domain::cancel_policy::CancelPolicy) -> String {
    let mut rows = String::new();
    for t in &policy.tiers {
        let when = match t.min_days {
            0 => "방문 당일 취소".to_owned(),
            d if d > 0 => format!("방문일 {d}일 전까지 취소"),
            _ => "방문 후".to_owned(),
        };
        let what = match t.percent {
            None => "앱에서 취소 불가, 상담팀 확인 후 처리".to_owned(),
            Some(0) => "전액 환불".to_owned(),
            Some(p) => format!("결제 금액의 {p}% 취소 수수료 (최소 {})", crate::seo::krw(policy.min_fee)),
        };
        rows.push_str(&format!("<li>{when}: {what}</li>"));
    }
    rows
}

/// A published legal text (`body.text`, Korean, as the apps show it) as escaped paragraphs under its title;
/// empty when there is none.
pub fn legal_html(title: &Value, body: &Value) -> String {
    let ko = |v: &Value| v.as_str().or_else(|| v["ko"].as_str()).map(str::trim).unwrap_or_default().to_owned();
    let text = ko(&body["text"]);
    if text.is_empty() {
        return String::new();
    }
    let paras: Vec<String> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", escape_html(p).replace('\n', "<br>")))
        .collect();
    format!("\n    <h1>{}</h1>\n    {}\n", escape_html(&ko(title)), paras.join("\n    "))
}

async fn legal(s: &AppState, slug: &str) -> ApiResult<String> {
    let row: Option<(Value, Value)> = sqlx::query_as(
        "SELECT title, body FROM content_items
         WHERE kind = 'legal' AND slug = $1 AND active AND (starts_at IS NULL OR starts_at <= now()) AND (ends_at IS NULL OR ends_at > now())
         ORDER BY sort_order, created_at DESC LIMIT 1",
    )
    .bind(slug)
    .fetch_optional(&s.db)
    .await?;
    Ok(row.map(|(t, b)| legal_html(&t, &b)).unwrap_or_default())
}

fn used(s: &AppState, sec: Section) -> String {
    (if s.providers.integ().has(sec) { USED } else { NOT_USED }).to_owned()
}

/// Database-read fragments by (database, field). A page view includes many and pages are not rate limited, so a
/// page flood must not become a database flood; edits show within the same 60 s as the business row.
static DB_FRAGMENTS: Mutex<BTreeMap<(String, String), (Instant, String)>> = Mutex::new(BTreeMap::new());

fn fresh_fragment(db: &str, field: &str) -> Option<String> {
    let c = DB_FRAGMENTS.lock().ok()?;
    c.get(&(db.to_owned(), field.to_owned()))
        .filter(|(t, _)| t.elapsed() < super::BUSINESS_TTL)
        .map(|(_, v)| v.clone())
}

fn remember_fragment(db: &str, field: &str, html: &str) {
    if let Ok(mut c) = DB_FRAGMENTS.lock() {
        c.insert((db.to_owned(), field.to_owned()), (Instant::now(), html.to_owned()));
    }
}

/// Forget every cached fragment; called when the owner saves legal texts, the cancel policy or a settings group.
pub(crate) fn invalidate_fragments() {
    if let Ok(mut c) = DB_FRAGMENTS.lock() {
        c.clear();
    }
}

async fn from_db(s: &AppState, field: &str) -> ApiResult<String> {
    let db = s.db.connect_options().get_database().unwrap_or_default().to_string();
    if let Some(html) = fresh_fragment(&db, field) {
        return Ok(html);
    }
    let html = match field {
        "cancel_policy" => cancel_rows(&super::super::bookings::load_cancel_policy(&s.db).await?.0),
        "legal_terms" => legal(s, "terms").await?,
        "legal_privacy" => legal(s, "privacy").await?,
        "legal_location" => legal(s, "location-terms").await?,
        _ => settings_registry::get(&s.db, &s.config, field).await?.to_string(),
    };
    remember_fragment(&db, field, &html);
    Ok(html)
}

/// Fields that read live settings. `None` for a field this module does not know.
pub async fn live_fragment(s: &AppState, field: &str) -> ApiResult<Option<String>> {
    Ok(Some(match field {
        "cancel_policy" | "ai_chat_retention_days" | "upload_orphan_days" | "legal_terms" | "legal_privacy" | "legal_location" => from_db(s, field).await?,
        "uses_portone" => used(s, Section::Portone),
        "uses_toss" => used(s, Section::TossPayments),
        "uses_solapi" => used(s, Section::Solapi),
        "uses_fcm" => used(s, Section::Push),
        "uses_ai" => used(s, Section::Ai),
        "uses_backup" => used(s, Section::Backup),
        "uses_juso" => used(s, Section::Juso),
        _ => return Ok(None),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://shop.example";

    #[test]
    fn database_fragments_are_cached_per_database() {
        assert_eq!(fresh_fragment("live-db", "legal_terms"), None);
        remember_fragment("live-db", "legal_terms", "<h1>약관</h1>");
        assert_eq!(fresh_fragment("live-db", "legal_terms").as_deref(), Some("<h1>약관</h1>"));
        assert_eq!(fresh_fragment("other-db", "legal_terms"), None, "another database never reads this entry");
        assert_eq!(fresh_fragment("live-db", "legal_privacy"), None);
    }

    #[test]
    fn brand_fields_are_escaped_and_fall_back() {
        let b = json!({"name": "반짝<b>", "name_en": "Shine & Co", "primary_color": "#112233"});
        assert_eq!(brand_fragment(BASE, &b, "name").unwrap(), "반짝&lt;b&gt;");
        assert_eq!(brand_fragment(BASE, &b, "name_en").unwrap(), "Shine &amp; Co");
        assert_eq!(brand_fragment(BASE, &json!({}), "name").unwrap(), brand::DEFAULT_NAME);
        assert_eq!(brand_fragment(BASE, &b, "primary_color").unwrap(), "#112233");
        assert_eq!(
            brand_fragment(BASE, &json!({"primary_color": "red;}"}), "primary_color").unwrap(),
            brand::DEFAULT_COLOR
        );
        assert_eq!(brand_fragment(BASE, &json!({}), "logo_img").unwrap(), "");
        assert_eq!(
            brand_fragment(BASE, &json!({}), "og_image").unwrap(),
            "https://shop.example/assets/og-image.png"
        );
        assert_eq!(brand_fragment(BASE, &json!({}), "favicon").unwrap(), DEFAULT_FAVICON);
        assert_eq!(brand_fragment(BASE, &json!({}), "privacy_email").unwrap(), "[개인정보 보호 이메일]");
        assert!(brand_fragment(BASE, &b, "password").is_none());
    }

    #[test]
    fn brand_files_become_public_paths() {
        let b = json!({"logo_url": "/files/a.png", "brand_assets": {"favicon_32": "/files/f.png", "og_1200": "/files/o.png"}});
        assert_eq!(
            brand_fragment(BASE, &b, "logo_img").unwrap(),
            "<img src=\"https://shop.example/v1/files/a.png\" alt=\"\">"
        );
        assert_eq!(brand_fragment(BASE, &b, "favicon").unwrap(), "/v1/files/f.png");
        assert_eq!(brand_fragment(BASE, &b, "og_image").unwrap(), "https://shop.example/v1/files/o.png");
    }

    #[test]
    fn ld_json_is_valid_json_that_cannot_close_its_element() {
        let b = json!({"name": "</script><b>{{env \"X\"}}", "support_phone": "02-1"});
        let html = ld_json(BASE, &b);
        let inner = html
            .strip_prefix("<script type=\"application/ld+json\">")
            .and_then(|h| h.strip_suffix("</script>"))
            .unwrap();
        assert!(!inner.contains("</") && !inner.contains("{{"), "{inner}");
        let v: Value = serde_json::from_str(inner).expect("valid JSON");
        assert_eq!(v["@graph"][1]["name"], "</script><b>{{env \"X\"}}");
        assert_eq!(v["@graph"][1]["logo"], "https://shop.example/assets/icon-512.png");
        assert_eq!(v["@graph"][0]["url"], "https://shop.example/");
        assert_eq!(v["@graph"][1]["telephone"], "02-1");
    }

    #[test]
    fn template_braces_never_reach_caddy() {
        assert_eq!(defuse("{{env \"DATA_KEY\"}}"), "&#123;&#123;env \"DATA_KEY\"&#125;&#125;");
    }

    #[test]
    fn cancel_rows_follow_the_policy() {
        let p: domain::cancel_policy::CancelPolicy = serde_json::from_value(json!({
            "tiers": [{"min_days": 3, "percent": 0}, {"min_days": 1, "percent": 30}, {"min_days": 0, "percent": null}],
            "min_fee": 10000, "reschedule_min_days": 2
        }))
        .unwrap();
        assert_eq!(
            cancel_rows(&p),
            "<li>방문일 3일 전까지 취소: 전액 환불</li><li>방문일 1일 전까지 취소: 결제 금액의 30% 취소 수수료 (최소 10,000원)</li><li>방문 당일 취소: 앱에서 취소 불가, 상담팀 확인 후 처리</li>"
        );
    }

    #[test]
    fn legal_text_is_escaped_paragraphs_or_empty() {
        assert_eq!(legal_html(&json!({"ko": "약관"}), &json!({"text": {"ko": "  "}})), "");
        assert_eq!(legal_html(&json!({"ko": "약관"}), &json!({})), "");
        let html = legal_html(&json!({"ko": "이용약관"}), &json!({"text": {"ko":"제1조 <목적>\n줄바꿈\n\n제2조"}}));
        assert!(html.contains("<h1>이용약관</h1>"), "{html}");
        assert!(html.contains("<p>제1조 &lt;목적&gt;<br>줄바꿈</p>"), "{html}");
        assert!(html.contains("<p>제2조</p>"), "{html}");
    }
}
