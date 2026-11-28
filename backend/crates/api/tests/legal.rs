// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Legal documents (R3): the KR pack templates render the website's legal pages and a stable golden, the baseline
//! import, per-document consent versions, the re-consent flow over the API (old and new apps), the Legal inbox, and
//! that the previous release still runs on the new schema.

mod common;

use api::{auth::Role, build_state, legal, pack, privacy, routes::legal::import_baseline};
use axum::{Router, http::StatusCode};
use common::{call, golden, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const NEW: &str = "2026-12-01";

fn kr_templates() -> Vec<legal::Template> {
    legal::templates(&pack::load("KR").expect("KR"), pack::EMBEDDED).expect("templates")
}

/// Rust's renderer, given the Caddy include as each seller value, yields exactly the body build_legal.py put in the
/// website page (inside its published-text switch), so the website and the inbox show one text.
#[test]
fn kr_templates_render_the_website_pages_byte_for_byte() {
    let kr = pack::load("KR").expect("KR");
    let site = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../website");
    for t in kr_templates() {
        let values: Vec<(String, String)> = legal::slots(&t.body)
            .expect("slots")
            .into_iter()
            .map(|name| {
                let value = match name.split_once('.') {
                    Some(("seller", f)) => format!("{{{{httpInclude \"/v1/public/seller-info?field={f}\"}}}}"),
                    Some(("pack", path)) => legal::pack_slot(&kr, path).expect("pack value"),
                    Some(("doc", "effective")) => legal::effective_label(&t),
                    _ => panic!("{name} has no website value"),
                };
                (name.to_string(), value)
            })
            .collect();
        let body = legal::render(&t, &values).expect("render");
        let page = match t.doc.as_str() {
            "location_terms" => "location-terms.html".to_string(),
            doc => format!("{doc}.html"),
        };
        let html = std::fs::read_to_string(site.join(&page)).expect("website page").replace("\r\n", "\n");
        assert!(
            html.contains(&format!("{{{{else}}}}{body}{{{{end}}}}")),
            "{page} differs from the {} template",
            t.doc
        );
    }
}

fn kr_business() -> Value {
    json!({
        "name": "다해클린",
        "support_phone": "1588-0000",
        "support_email": "help@example.co.kr",
        "company": {
            "legal_name": "주식회사 다해클린",
            "ceo": "홍길동",
            "registration_no": "220-81-62517",
            "mail_order_no": "제2026-서울강남-0001호",
            "address": "서울특별시 강남구 테헤란로 1, 10층",
            "privacy_officer": "김보호",
            "hosting_provider": "예시호스팅(주)",
            "privacy_email": "privacy@example.co.kr"
        }
    })
}

async fn configured(pool: &PgPool) -> (Router, String, Uuid) {
    let app = common::app(pool.clone());
    let (token, staff) = common::staff_token(pool, Role::StaffAdmin).await;
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(kr_business())).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    (app, token, staff)
}

/// Each KR text rendered for a configured install: the renderer golden.
#[sqlx::test(migrations = "../../migrations")]
async fn kr_documents_render_for_an_install(pool: PgPool) {
    let _ = configured(&pool).await;
    let state = build_state(pool.clone(), test_config()).expect("state");
    let mut out = Vec::new();
    for t in kr_templates() {
        let (html, values) = legal::render_live(&state, &t).await.expect("render");
        assert!(!html.contains("{seller.") && !html.contains("{pack.") && !html.contains("{doc."), "{}", t.doc);
        out.push(json!({ "doc": t.doc, "lang": t.lang, "version": t.version, "title": t.title, "slots": values.len(), "html": html }));
    }
    golden("legal_documents", &Value::Array(out));
}

#[sqlx::test(migrations = "../../migrations")]
async fn baseline_import_records_todays_texts_once_on_an_existing_install(pool: PgPool) {
    let _ = configured(&pool).await;
    let state = build_state(pool.clone(), test_config()).expect("state");
    assert_eq!(import_baseline(&state).await.expect("import"), 3);
    let rows: Vec<(String, String, String, bool, chrono::NaiveDate, Option<Uuid>)> =
        sqlx::query_as("SELECT doc, lang, version, requires_reconsent, effective_at, published_by FROM legal_documents ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("rows");
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 17).expect("date");
    assert_eq!(
        rows,
        ["terms", "privacy", "location_terms"].map(|d| (d.to_string(), "ko".to_string(), privacy::CONSENT_VERSION.to_string(), true, day, None))
    );
    assert_eq!(import_baseline(&state).await.expect("again"), 0, "runs once");
    let versions = privacy::consent_versions(&pool).await.expect("versions");
    assert!(
        privacy::CONSENTS.iter().all(|(k, _)| versions.of(k) == privacy::CONSENT_VERSION),
        "imported texts keep today's meaning"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_fresh_install_imports_nothing_even_after_it_creates_staff(pool: PgPool) {
    let state = build_state(pool.clone(), test_config()).expect("state");
    assert_eq!(import_baseline(&state).await.expect("import"), 0);
    common::staff_token(&pool, Role::StaffAdmin).await;
    assert_eq!(import_baseline(&state).await.expect("later boot"), 0, "the owner publishes from the inbox");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM legal_documents")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 0);
}

/// A release's newer text of `doc`, as the inbox's publish stores it.
async fn publish_version(pool: &PgPool, doc: &str, version: &str, reconsent: bool) {
    sqlx::query(
        "INSERT INTO legal_documents (doc, lang, version, template_version, requires_reconsent, body_md, body_html, body_sha256, effective_at)
         VALUES ($1, 'ko', $2, $2, $3, 'md', '<p>new</p>', repeat('a', 64), $2::date)",
    )
    .bind(doc)
    .bind(version)
    .bind(reconsent)
    .execute(pool)
    .await
    .expect("publish");
}

#[sqlx::test(migrations = "../../migrations")]
async fn consent_versions_follow_the_published_documents(pool: PgPool) {
    publish_version(&pool, "privacy", "2026-11-20", false).await;
    let v = privacy::consent_versions(&pool).await.expect("versions");
    assert_eq!(
        v.of("privacy"),
        privacy::CONSENT_VERSION,
        "a correction that does not ask again changes nothing"
    );
    publish_version(&pool, "privacy", NEW, true).await;
    let v = privacy::consent_versions(&pool).await.expect("versions");
    for (kind, want) in [
        ("terms", privacy::CONSENT_VERSION),
        ("privacy", NEW),
        ("overseas_transfer", NEW),
        ("marketing", NEW),
        ("location", privacy::CONSENT_VERSION),
    ] {
        assert_eq!(v.of(kind), want, "{kind}");
    }
    assert_eq!(v.bundle(), NEW);
}

fn pending_kinds(me: &Value) -> Vec<&str> {
    me["pending_consents"]
        .as_array()
        .expect("pending_consents")
        .iter()
        .map(|p| p["kind"].as_str().expect("kind"))
        .collect()
}

/// A new privacy text asks again. An old app answers through PUT /me/consents with the single version it reads there;
/// a new app accepts the pending kinds named on GET /me. Until then the optional purposes count as withdrawn.
#[sqlx::test(migrations = "../../migrations")]
async fn reconsent_after_a_new_required_document_version(pool: PgPool) {
    let app = common::app(pool.clone());
    let old_app = common::customer(&app, &pool, "010-8200-0001").await;
    let new_app = common::customer(&app, &pool, "010-8200-0002").await;
    let marketing = json!({ "consents": [{ "kind": "marketing", "granted": true, "version": privacy::CONSENT_VERSION }] });
    let (st, _) = call(&app, "PUT", "/v1/me/consents", Some(new_app.token.as_str()), Some(marketing)).await;
    assert_eq!(st, StatusCode::OK);
    let (_, me) = call(&app, "GET", "/v1/me", Some(new_app.token.as_str()), None).await;
    assert!(pending_kinds(&me).is_empty(), "{me}");
    assert!(me["phone"].is_string(), "the profile fields stay: {me}");

    publish_version(&pool, "privacy", NEW, true).await;

    let (_, me) = call(&app, "GET", "/v1/me", Some(new_app.token.as_str()), None).await;
    assert_eq!(pending_kinds(&me), ["privacy", "overseas_transfer", "marketing"], "{me}");
    assert_eq!(
        me["pending_consents"][0],
        json!({ "kind": "privacy", "doc": "privacy", "version": NEW, "required": true })
    );
    assert!(
        !privacy::overseas_allowed(&pool, new_app.user).await.expect("overseas"),
        "push and AI wait for the new consent"
    );
    assert!(!privacy::marketing_allowed(&pool, new_app.user).await.expect("marketing"));

    // Old app: GET /me/consents asks again; its consent screen sends every item with the version reported there.
    let (_, s) = call(&app, "GET", "/v1/me/consents", Some(old_app.token.as_str()), None).await;
    assert_eq!((s["reconsent_required"].clone(), s["version"].clone()), (json!(true), json!(NEW)), "{s}");
    let items: Vec<Value> = ["terms", "privacy", "overseas_transfer", "marketing", "location"]
        .iter()
        .map(|k| {
            let granted = ["terms", "privacy", "overseas_transfer"].contains(k);
            json!({ "kind": k, "granted": granted, "version": s["version"] })
        })
        .collect();
    let (st, s) = call(&app, "PUT", "/v1/me/consents", Some(old_app.token.as_str()), Some(json!({ "consents": items }))).await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert_eq!(s["reconsent_required"], false, "{s}");
    let terms: String = sqlx::query_scalar("SELECT version FROM user_consents WHERE user_id = $1 AND kind = 'terms' ORDER BY id DESC LIMIT 1")
        .bind(old_app.user)
        .fetch_one(&pool)
        .await
        .expect("terms row");
    assert_eq!(terms, privacy::CONSENT_VERSION, "each kind records its own document's version");
    assert!(privacy::overseas_allowed(&pool, old_app.user).await.expect("overseas"));

    // New app: accepts what GET /me lists, one call per screen if it likes.
    let accept = |kinds: &[&str], version: &str| json!({ "consents": kinds.iter().map(|k| json!({ "kind": k, "version": version })).collect::<Vec<_>>() });
    let (st, b) = call(
        &app,
        "POST",
        "/v1/me/consents/accept",
        Some(new_app.token.as_str()),
        Some(accept(&["privacy"], "2026-11-17")),
    )
    .await;
    assert_eq!(
        (st, b["error"]["code"].clone()),
        (StatusCode::BAD_REQUEST, json!("stale_consent_version")),
        "{b}"
    );
    let (st, b) = call(
        &app,
        "POST",
        "/v1/me/consents/accept",
        Some(new_app.token.as_str()),
        Some(accept(&["privacy"], NEW)),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(pending_kinds(&b), ["overseas_transfer", "marketing"]);
    let (st, b) = call(
        &app,
        "POST",
        "/v1/me/consents/accept",
        Some(new_app.token.as_str()),
        Some(accept(&["overseas_transfer", "marketing"], NEW)),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert!(pending_kinds(&b).is_empty(), "{b}");
    assert!(privacy::marketing_allowed(&pool, new_app.user).await.expect("marketing"));

    // An app still built with the old text signs up, and is asked again at once.
    let late = common::customer(&app, &pool, "010-8200-0003").await;
    let (_, me) = call(&app, "GET", "/v1/me", Some(late.token.as_str()), None).await;
    assert_eq!(pending_kinds(&me), ["privacy", "overseas_transfer"], "{me}");
}

fn doc<'a>(inbox: &'a Value, name: &str) -> &'a Value {
    inbox["documents"]
        .as_array()
        .expect("documents")
        .iter()
        .find(|d| d["doc"] == name)
        .expect("document")
}

/// The inbox lists the KR texts; after the baseline import they are published and the draft is the same text.
#[sqlx::test(migrations = "../../migrations")]
async fn the_legal_inbox_lists_previews_and_publishes(pool: PgPool) {
    let (app, token, staff) = configured(&pool).await;
    let (st, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "{inbox}");
    assert_eq!(inbox["documents"].as_array().map(Vec::len), Some(3));
    assert_eq!(doc(&inbox, "privacy")["pending"], true, "nothing published yet");

    let state = build_state(pool.clone(), test_config()).expect("state");
    import_baseline(&state).await.expect("import");
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    for name in ["terms", "privacy", "location_terms"] {
        let d = doc(&inbox, name);
        assert_eq!(d["pending"], false, "{name}");
        assert_eq!(d["published"]["version"], privacy::CONSENT_VERSION);
        assert_eq!(d["published"]["body_html"], d["draft_html"], "{name}: the inbox shows the same text");
        assert_eq!(d["template"]["requires_reconsent"], true);
    }
    assert!(doc(&inbox, "terms")["draft_html"].as_str().expect("html").contains("주식회사 다해클린"));

    let (consultant, _) = common::staff_token(&pool, Role::StaffConsultant).await;
    let body = json!({ "doc": "terms", "lang": "ko" });
    let (st, _) = call(&app, "POST", "/v1/admin/legal/publish", Some(&consultant), Some(body.clone())).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call(
        &app,
        "POST",
        "/v1/admin/legal/publish",
        Some(&token),
        Some(json!({ "doc": "terms", "lang": "en" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "no English KR text");
    let (st, published) = call(&app, "POST", "/v1/admin/legal/publish", Some(&token), Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{published}");
    assert_eq!(published["version"], privacy::CONSENT_VERSION);
    let id = published["id"].as_i64().expect("id");
    let by: Option<Uuid> = sqlx::query_scalar("SELECT published_by FROM legal_documents WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(by, Some(staff));
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'legal.publish' AND target = 'terms'")
        .fetch_one(&pool)
        .await
        .expect("audit");
    assert_eq!(audited, 1);

    let (st, v) = call(&app, "GET", &format!("/v1/admin/legal/versions/{id}"), Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["body_html"], doc(&inbox, "terms")["draft_html"]);
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    assert_eq!(doc(&inbox, "terms")["versions"].as_array().map(Vec::len), Some(2), "newest first, both kept");
    let (st, _) = call(&app, "GET", "/v1/admin/legal/versions/999999", Some(&token), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

/// N-1 on the new schema: it writes today's five kinds, reads an unknown future kind as not granted (the state lists
/// only known kinds), and never touches legal_documents. Published rows are immutable.
#[sqlx::test(migrations = "../../migrations")]
async fn the_previous_release_runs_on_the_new_schema(pool: PgPool) {
    let app = common::app(pool.clone());
    let c = common::customer(&app, &pool, "010-8200-0009").await;
    for kind in ["terms", "privacy", "overseas_transfer", "marketing", "location", "cookies"] {
        sqlx::query("INSERT INTO user_consents (user_id, kind, version, granted) VALUES ($1, $2, $3, true)")
            .bind(c.user)
            .bind(kind)
            .bind(privacy::CONSENT_VERSION)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{kind}: {e}"));
    }
    let bad = sqlx::query("INSERT INTO user_consents (user_id, kind, version, granted) VALUES ($1, 'Bad-Kind', 'x', true)")
        .bind(c.user)
        .execute(&pool)
        .await;
    assert!(bad.is_err(), "the same-name CHECK still guards the format");
    let validated: bool =
        sqlx::query_scalar("SELECT convalidated FROM pg_constraint WHERE conrelid = 'user_consents'::regclass AND conname = 'user_consents_kind_check'")
            .fetch_one(&pool)
            .await
            .expect("constraint");
    assert!(validated, "0516 validated it under the same name");
    let (st, s) = call(&app, "GET", "/v1/me/consents", Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert!(s["items"].as_array().expect("items").iter().all(|i| i["kind"] != "cookies"));

    publish_version(&pool, "terms", NEW, true).await;
    let changed = sqlx::query("UPDATE legal_documents SET body_html = 'x'").execute(&pool).await;
    assert!(changed.is_err(), "published texts are append-only");
    // N-1 still checks consents against its constant; a grant at it keeps holding there.
    let n1: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_consents WHERE user_id = $1 AND kind = 'terms' AND granted AND version = $2)")
        .bind(c.user)
        .bind(privacy::CONSENT_VERSION)
        .fetch_one(&pool)
        .await
        .expect("n-1 read");
    assert!(n1);
}
