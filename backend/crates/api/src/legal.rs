// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Legal templates from the country pack and the slot renderer. A template is `<scope>/legal/<doc>.<lang>.md`:
//! `key: value` front matter between two `---` lines, then the body (HTML, which Markdown allows) with `{space.key}`
//! slots. A pack without its own text for a document uses its parent scope's. Slots:
//! - `seller.<field>`: the company field or live setting `/v1/public/seller-info?field=` serves (already escaped);
//! - `pack.<path>`: a scalar from the merged pack, e.g. `pack.privacy.min_age`;
//! - `install.<key>`: an install fact (`country`, `currency`, `tz`);
//! - `doc.effective` / `doc.version`: from the front matter.
//!
//! `scripts/build_legal.py` renders the same KR templates into the website pages, turning `seller.*` into Caddy
//! includes, so both stay one text.

use crate::{
    error::{ApiError, ApiResult},
    i18n,
    pack::{self, Pack, PackError},
    privacy,
    routes::seller,
    seo::escape_html,
    settings_registry,
    state::AppState,
    util::LOCAL_TODAY,
};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use serde_json::Value;

/// Install facts a template may name as `install.<key>`.
pub const INSTALL_SLOTS: [&str; 3] = ["country", "currency", "tz"];

/// One document text in one language, as the pack ships it.
#[derive(Debug, Clone, Serialize)]
pub struct Template {
    pub doc: String,
    pub lang: String,
    pub title: String,
    /// The document version a publish records (`YYYY-MM-DD`, optionally `.N`); consents name it.
    pub version: String,
    pub effective_from: NaiveDate,
    /// Publishing this version asks every customer who agreed to an older one to agree again.
    pub requires_reconsent: bool,
    pub law_refs: Vec<String>,
    pub reviewed_by: String,
    #[serde(skip)]
    pub body: String,
}

/// Reads one template file. `doc` and `lang` come from its path and must match the front matter's `doc`.
pub fn parse(doc: &str, lang: &str, text: &str) -> Result<Template, String> {
    let rest = text.strip_prefix("---\n").ok_or("front matter must open the file")?;
    let (head, body) = rest.split_once("\n---\n").ok_or("front matter is not closed")?;
    let mut meta = std::collections::BTreeMap::new();
    for line in head.lines() {
        let (k, v) = line.split_once(": ").ok_or_else(|| format!("front matter line {line:?} is not key: value"))?;
        meta.insert(k, v.trim());
    }
    let get = |k: &str| meta.get(k).copied().filter(|v| !v.is_empty()).ok_or_else(|| format!("front matter needs {k}"));
    if get("doc")? != doc {
        return Err("front matter doc does not match the file name".into());
    }
    let version = get("version")?;
    if !valid_version(version) {
        return Err("version must be YYYY-MM-DD or YYYY-MM-DD.N".into());
    }
    Ok(Template {
        doc: doc.to_string(),
        lang: lang.to_string(),
        title: get("title")?.to_string(),
        version: version.to_string(),
        effective_from: get("effective_from")?.parse().map_err(|_| "effective_from must be YYYY-MM-DD")?,
        requires_reconsent: match get("requires_reconsent")? {
            "true" => true,
            "false" => false,
            _ => return Err("requires_reconsent must be true or false".into()),
        },
        law_refs: get("law_refs")?.split(", ").map(str::to_string).collect(),
        reviewed_by: get("reviewed_by")?.to_string(),
        body: body.to_string(),
    })
}

fn valid_version(v: &str) -> bool {
    version_key(v).is_some()
}

/// A document version as (date, counter) for ordering: as text `2026-12-01.10` would sort before `2026-12-01.9`.
/// None for a malformed version, which sorts before every real one.
pub fn version_key(v: &str) -> Option<(NaiveDate, u32)> {
    let (date, n) = v.split_once('.').map_or((v, None), |(d, n)| (d, Some(n)));
    let n = match n {
        None => 0,
        Some(n) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => n.parse().ok()?,
        Some(_) => return None,
    };
    (date.len() == 10)
        .then(|| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .flatten()
        .map(|d| (d, n))
}

/// The `admin_content` slug (and website page name) of a document: `location_terms` is `location-terms`.
pub fn slug(doc: &str) -> String {
    doc.replace('_', "-")
}

/// The published text the website and the app show in place of the pack's built-in body: the newest text of `doc`
/// in force, as it was published, so customers always read the text their consents name. Company fields or
/// processors changed since reach customers only through the next Publish (the inbox shows it pending). None while
/// nothing is published (the page keeps its built-in body, filled live).
pub async fn published_text(s: &AppState, doc: &str) -> ApiResult<Option<String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(&format!(
        "SELECT version, body_html FROM legal_documents
         WHERE doc = $1 AND lang = $2 AND NOT custom AND effective_at <= {LOCAL_TODAY}"
    ))
    .bind(doc)
    .bind(privacy::LEGAL_LANG)
    .fetch_all(&s.db)
    .await?;
    Ok(rows.into_iter().max_by_key(|(v, _)| version_key(v)).map(|(_, html)| html))
}

/// Every template of the pack's documents, each in every pack locale that has a text along the pack's scopes, all
/// slots checked. The pack's first locale is required for every document.
pub fn templates(p: &Pack, files: &[(&str, &str)]) -> Result<Vec<Template>, PackError> {
    let scopes = pack::scopes(&p.id, files)?;
    let mut out = Vec::new();
    for doc in &p.legal {
        for (i, lang) in p.locales.iter().enumerate() {
            let found = scopes.iter().find_map(|scope| {
                let path = format!("{scope}/legal/{doc}.{lang}.md");
                files.iter().find(|(f, _)| *f == path).map(|(_, text)| (path, *text))
            });
            let Some((path, text)) = found else {
                if i == 0 {
                    return Err(pack::err(&p.id, format!("legal document {doc} has no {lang} text")));
                }
                continue;
            };
            let t = parse(doc, lang, text).map_err(|e| pack::err(&p.id, format!("{path}: {e}")))?;
            check_slots(p, &t).map_err(|e| pack::err(&p.id, format!("{path}: {e}")))?;
            out.push(t);
        }
    }
    Ok(out)
}

fn check_slots(p: &Pack, t: &Template) -> Result<(), String> {
    let fields = seller::all_fields();
    for name in slots(&t.body)? {
        let ok = match name.split_once('.') {
            Some(("seller", field)) => fields.contains(&field),
            Some(("pack", path)) => pack_slot(p, path).is_some(),
            Some(("install", key)) => INSTALL_SLOTS.contains(&key),
            Some(("doc", key)) => ["effective", "version"].contains(&key),
            _ => false,
        };
        if !ok {
            return Err(format!("unknown slot {{{name}}}"));
        }
    }
    Ok(())
}

/// The distinct slot names in a body, in order of first use. Braces must pair up.
pub fn slots(body: &str) -> Result<Vec<&str>, String> {
    let mut out: Vec<&str> = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find(['{', '}']) {
        let close = rest[open..]
            .find('}')
            .map(|c| c + open)
            .filter(|_| rest.as_bytes()[open] == b'{')
            .ok_or("unbalanced braces")?;
        let name = &rest[open + 1..close];
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_' || b == b'.') {
            return Err(format!("malformed slot {{{name}}}"));
        }
        if !out.contains(&name) {
            out.push(name);
        }
        rest = &rest[close + 1..];
    }
    Ok(out)
}

/// A scalar of the merged pack at a dotted path, as text; None for a missing path, an object, a list or null.
pub fn pack_slot(p: &Pack, path: &str) -> Option<String> {
    let v = serde_json::to_value(p).ok()?;
    let pointer = format!("/{}", path.replace('.', "/"));
    match v.pointer(&pointer)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The effective date as the document's language writes it.
pub fn effective_label(t: &Template) -> String {
    let d = t.effective_from;
    match t.lang.as_str() {
        "ko" => format!("{}년 {}월 {}일", d.year(), d.month(), d.day()),
        _ => d.format("%Y-%m-%d").to_string(),
    }
}

/// The body with every slot replaced by its value (values are inserted as they are: escape them first).
pub fn render(t: &Template, values: &[(String, String)]) -> Result<String, i18n::TemplateError> {
    let refs: Vec<(&str, String)> = values.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    i18n::fill(&t.body, &refs)
}

/// This install's value for every slot of `t`, escaped (and defused like the website includes).
pub async fn slot_values(s: &AppState, t: &Template) -> ApiResult<Vec<(String, String)>> {
    let business = seller::cached_business(&s.db).await?;
    let mut out = Vec::new();
    for name in slots(&t.body).map_err(ApiError::Internal)? {
        let value = match name.split_once('.') {
            Some(("seller", field)) => seller::any_fragment(s, &business, Some(field)).await?,
            Some(("pack", path)) => pack_slot(&s.pack, path).map(|v| escape_html(&v)),
            Some(("install", key)) if INSTALL_SLOTS.contains(&key) => {
                Some(escape_html(&settings_registry::get_text(&s.db, &s.config, &format!("install.{key}")).await?))
            }
            Some(("doc", "effective")) => Some(effective_label(t)),
            Some(("doc", "version")) => Some(t.version.clone()),
            _ => None,
        };
        let value = value.ok_or_else(|| ApiError::Internal(format!("legal slot {name} has no value")))?;
        out.push((name.to_string(), seller::live::defuse(&value)));
    }
    Ok(out)
}

/// `t` rendered for this install, with the slot values it used.
pub async fn render_live(s: &AppState, t: &Template) -> ApiResult<(String, Vec<(String, String)>)> {
    let values = slot_values(s, t).await?;
    let html = render(t, &values).map_err(|e| ApiError::Internal(format!("legal template {}: {}", t.doc, e.code())))?;
    Ok((html, values))
}

/// The active pack's templates (validated at boot, so this only fails on a build that never booted).
pub fn active_templates(p: &Pack) -> ApiResult<Vec<Template>> {
    templates(p, pack::EMBEDDED).map_err(|e| ApiError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str =
        "---\ndoc: terms\ntitle: T\nversion: 2026-11-17\neffective_from: 2026-11-17\nrequires_reconsent: true\nlaw_refs: A, B\nreviewed_by: X\n---\n";

    #[test]
    fn front_matter_is_read_and_the_body_kept_exactly() {
        let t = parse("terms", "ko", &format!("{HEAD}\n  <h1>{{seller.name}}</h1>\n")).expect("parse");
        assert_eq!((t.version.as_str(), t.requires_reconsent), ("2026-11-17", true));
        assert_eq!(t.law_refs, ["A", "B"]);
        assert_eq!(t.body, "\n  <h1>{seller.name}</h1>\n");
        assert_eq!(effective_label(&t), "2026년 11월 17일");
    }

    #[test]
    fn malformed_front_matter_is_refused() {
        assert!(parse("privacy", "ko", HEAD).is_err(), "doc must match the file");
        assert!(parse("terms", "ko", &HEAD.replace("2026-11-17\neff", "v2\neff")).is_err(), "version format");
        assert!(parse("terms", "ko", &HEAD.replace("true", "yes")).is_err());
        assert!(parse("terms", "ko", "no front matter").is_err());
    }

    #[test]
    fn versions_are_dates_with_an_optional_counter() {
        assert!(valid_version("2026-11-17") && valid_version("2026-11-17.2"));
        assert!(!valid_version("2026-11-17.") && !valid_version("2026-13-01") && !valid_version("2026.11.0"));
        assert!(!valid_version("2026-11-17.1.2") && !valid_version("2026-11-17.-1"));
    }

    #[test]
    fn versions_order_by_date_then_counter() {
        assert!(version_key("2026-12-01.10") > version_key("2026-12-01.9"), "not as text");
        assert!(version_key("2026-12-01") < version_key("2026-12-01.1"));
        assert!(version_key("2026-12-02") > version_key("2026-12-01.99"));
        assert!(version_key("junk") < version_key("2000-01-01"));
        assert_eq!(slug("location_terms"), "location-terms");
    }

    #[test]
    fn slots_are_listed_once_and_braces_must_pair() {
        assert_eq!(slots("{a.b} x {c} {a.b}").expect("slots"), ["a.b", "c"]);
        assert!(slots("a } b").is_err());
        assert!(slots("{open").is_err());
        assert!(slots("{Bad}").is_err());
    }

    #[test]
    fn render_fills_every_slot_and_refuses_a_missing_one() {
        let t = parse("terms", "en", &format!("{HEAD}<p>{{seller.name}} · {{doc.effective}}</p>")).expect("parse");
        let values = vec![
            ("seller.name".to_string(), "A &amp; B".to_string()),
            ("doc.effective".to_string(), effective_label(&t)),
        ];
        assert_eq!(render(&t, &values).expect("render"), "<p>A &amp; B · 2026-11-17</p>");
        assert!(render(&t, &values[..1]).is_err());
    }

    #[test]
    fn pack_slots_read_scalars_only() {
        let kr = pack::load("KR").expect("KR");
        assert_eq!(pack_slot(&kr, "privacy.min_age").as_deref(), Some("14"));
        assert_eq!(pack_slot(&kr, "currency.code").as_deref(), Some("KRW"));
        assert_eq!(pack_slot(&kr, "privacy"), None, "an object is not a value");
        assert_eq!(pack_slot(&kr, "privacy.nope"), None);
    }
}
