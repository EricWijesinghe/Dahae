// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Legal documents (R3): the KR pack templates render the website's legal pages and a stable golden, the baseline
//! import, per-document consent versions, the re-consent flow over the API (old and new apps), the Legal inbox, and
//! that the previous release still runs on the new schema.

mod common;

use api::{
    auth::Role,
    build_state, legal, pack, privacy,
    routes::legal::{import_baseline, import_baseline_from},
};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use common::{call, call_owner, golden, test_config};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

const NEW: &str = "2026-12-01";
/// A pack template version newer than the texts customers agreed to, already in force.
const BUMP: &str = "2026-11-24";
const SEED: &[u8] = b"legal-test-owner-seed-0123456789";

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
        ["terms", "privacy", "location_terms"].map(|d| (d.to_string(), "ko".to_string(), privacy::CONSENT_VERSION.to_string(), false, day, None))
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

/// A release's newer text of `doc` (body `<p>{version}</p>`), as the inbox's publish stores it, in force since yesterday.
async fn publish_version(pool: &PgPool, doc: &str, version: &str, reconsent: bool) {
    publish_effective(pool, doc, version, reconsent, -1).await;
}

/// `publish_version` taking effect `days` from today.
async fn publish_effective(pool: &PgPool, doc: &str, version: &str, reconsent: bool, days: i32) {
    sqlx::query(
        "INSERT INTO legal_documents (doc, lang, version, template_version, requires_reconsent, body_md, body_html, body_sha256, effective_at)
         VALUES ($1, 'ko', $2, $2, $3, 'md', '<p>' || $2 || '</p>', repeat('a', 64), CURRENT_DATE + $4)",
    )
    .bind(doc)
    .bind(version)
    .bind(reconsent)
    .bind(days)
    .execute(pool)
    .await
    .expect("publish");
}

/// Publishes with a fresh owner code (each code is spent once, so the last step is cleared first).
async fn owner_publish(app: &Router, pool: &PgPool, token: &str, body: Value) -> (StatusCode, Value) {
    sqlx::query("UPDATE staff SET totp_last_step = NULL WHERE is_owner")
        .execute(pool)
        .await
        .expect("reset step");
    let code = format!(
        "{:06}",
        api::totp::code_at(SEED, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    );
    call_owner(app, "POST", "/v1/admin/legal/publish", token, Some(&code), body).await
}

/// A website fragment as the page includes it.
async fn fragment(app: &Router, field: &str) -> String {
    let mut req = Request::builder()
        .uri(format!("/v1/public/seller-info?field={field}"))
        .body(Body::empty())
        .expect("request");
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let resp = app.clone().oneshot(req).await.expect("router");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

/// Signs a customer up with the three required kinds at `version`.
async fn signup(app: &Router, phone: &str, version: &str) -> (StatusCode, Value) {
    let (_, sent) = call(app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": phone }))).await;
    let consents: Vec<Value> = ["terms", "privacy", "overseas_transfer"]
        .iter()
        .map(|k| json!({ "kind": k, "granted": true, "version": version }))
        .collect();
    let body = json!({ "phone": phone, "code": sent["dev_code"], "audience": "customer", "consents": consents });
    call(app, "POST", "/v1/auth/verify", None, Some(body)).await
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
    assert_eq!((v.current("privacy"), v.bundle()), ("2026-11-20", "2026-11-20"), "but it is the text shown");
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
    let (st, _) = owner_publish(&app, &pool, &token, body.clone()).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "an admin who is not the owner");
    common::enrol_owner(&pool, staff, SEED).await;
    let (st, b) = call_owner(&app, "POST", "/v1/admin/legal/publish", &token, None, body.clone()).await;
    assert_eq!((st, b["error"]["code"].clone()), (StatusCode::BAD_REQUEST, json!("totp_required")), "{b}");
    let (st, _) = owner_publish(&app, &pool, &token, json!({ "doc": "terms", "lang": "en" })).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "no English KR text");
    let (st, b) = owner_publish(&app, &pool, &token, body.clone()).await;
    assert_eq!(
        (st, b["error"]["code"].clone()),
        (StatusCode::CONFLICT, json!("legal_already_published")),
        "one version is one text: {b}"
    );

    // A company field changes the rendered text: the inbox offers it again, under the next counter, without re-consent.
    let mut business = kr_business();
    business["company"]["legal_name"] = json!("주식회사 다해클린서비스");
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(business)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, changed) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    assert_eq!(doc(&changed, "terms")["pending"], true, "{}", doc(&changed, "terms"));
    let (st, published) = owner_publish(&app, &pool, &token, body.clone()).await;
    assert_eq!(st, StatusCode::OK, "{published}");
    let counter = format!("{}.1", privacy::CONSENT_VERSION);
    assert_eq!(
        (published["version"].clone(), published["requires_reconsent"].clone()),
        (json!(counter), json!(false))
    );
    let (st, _) = owner_publish(&app, &pool, &token, body).await;
    assert_eq!(st, StatusCode::CONFLICT, "nothing new to publish");
    let id = published["id"].as_i64().expect("id");
    let by: Option<Uuid> = sqlx::query_scalar("SELECT published_by FROM legal_documents WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(by, Some(staff));
    let audited: Vec<Value> = sqlx::query_scalar("SELECT data FROM audit_log WHERE action = 'legal.publish' AND target = 'terms'")
        .fetch_all(&pool)
        .await
        .expect("audit");
    assert_eq!(audited.len(), 1);
    assert!(audited[0]["body_sha256"].as_str().is_some_and(|h| h.len() == 64), "{}", audited[0]);

    let (st, v) = call(&app, "GET", &format!("/v1/admin/legal/versions/{id}"), Some(&token), None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["body_html"], doc(&changed, "terms")["draft_html"]);
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

/// An install that skipped a release boots one whose privacy text is newer (`reconsent` as its template says): the
/// baseline records only the texts customers agreed to, the newer one waits in the inbox for the owner's Publish, and
/// every grant (marketing, overseas transfer for push and the AI chat) keeps holding meanwhile.
async fn baseline_with_a_newer_privacy_text(pool: &PgPool, reconsent: bool) -> (Router, common::Customer) {
    let (app, token, _) = configured(pool).await;
    let c = common::customer(&app, pool, "010-8200-0041").await;
    let marketing = json!({ "consents": [{ "kind": "marketing", "granted": true, "version": privacy::CONSENT_VERSION }] });
    let (st, _) = call(&app, "PUT", "/v1/me/consents", Some(c.token.as_str()), Some(marketing)).await;
    assert_eq!(st, StatusCode::OK);
    let state = build_state(pool.clone(), test_config()).expect("state");
    let templates: Vec<legal::Template> = kr_templates()
        .into_iter()
        .map(|mut t| {
            if t.doc == "privacy" {
                t.version = BUMP.into();
                t.requires_reconsent = reconsent;
            }
            t
        })
        .collect();
    assert_eq!(import_baseline_from(&state, templates).await.expect("import"), 2);
    let recorded: Vec<(String, String, bool)> = sqlx::query_as("SELECT doc, version, requires_reconsent FROM legal_documents ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("rows");
    let want = ["terms", "location_terms"].map(|d| (d.to_string(), privacy::CONSENT_VERSION.to_string(), false));
    assert_eq!(recorded, want, "the agreed texts, asking nobody again");
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    assert_eq!(
        (doc(&inbox, "privacy")["pending"].clone(), doc(&inbox, "terms")["pending"].clone()),
        (json!(true), json!(false))
    );
    assert_grants_hold(&app, pool, &c).await;
    (app, c)
}

async fn assert_grants_hold(app: &Router, pool: &PgPool, c: &common::Customer) {
    let v = privacy::consent_versions(pool).await.expect("versions");
    assert!(privacy::CONSENTS.iter().all(|(k, _)| v.of(k) == privacy::CONSENT_VERSION), "{v:?}");
    let (_, me) = call(app, "GET", "/v1/me", Some(c.token.as_str()), None).await;
    assert!(pending_kinds(&me).is_empty(), "{me}");
    assert!(privacy::overseas_allowed(pool, c.user).await.expect("overseas"), "push and AI keep running");
    assert!(privacy::marketing_allowed(pool, c.user).await.expect("marketing"));
}

/// A newer template that asks again changes nothing until the owner publishes it (and a text published for a later
/// day nothing before that day); then the consents behind it are asked again.
#[sqlx::test(migrations = "../../migrations")]
async fn a_skipped_release_asks_again_only_after_the_owner_publishes(pool: PgPool) {
    let (app, c) = baseline_with_a_newer_privacy_text(&pool, true).await;
    publish_effective(&pool, "privacy", NEW, true, 10).await;
    assert_grants_hold(&app, &pool, &c).await;
    publish_version(&pool, "privacy", BUMP, true).await;
    let (_, me) = call(&app, "GET", "/v1/me", Some(c.token.as_str()), None).await;
    assert_eq!(pending_kinds(&me), ["privacy", "overseas_transfer", "marketing"], "{me}");
    assert_eq!(me["pending_consents"][0]["version"], BUMP);
    assert!(!privacy::overseas_allowed(&pool, c.user).await.expect("overseas"));
}

/// A newer correction asks nobody again, before or after the owner publishes it, and is then what new consents record.
#[sqlx::test(migrations = "../../migrations")]
async fn a_skipped_release_correction_keeps_every_grant(pool: PgPool) {
    let (app, c) = baseline_with_a_newer_privacy_text(&pool, false).await;
    publish_version(&pool, "privacy", BUMP, false).await;
    assert_grants_hold(&app, &pool, &c).await;
    assert_eq!(privacy::consent_versions(&pool).await.expect("versions").current("privacy"), BUMP);
}

/// A replacement the owner stored before the Legal inbox existed.
async fn stored_replacement(pool: &PgPool, slug: &str, text: &str) -> Uuid {
    sqlx::query_scalar(r#"INSERT INTO content_items (kind, slug, title, body) VALUES ('legal', $1, '{"ko": "개인정보처리방침"}', $2) RETURNING id"#)
        .bind(slug)
        .bind(json!({ "text": { "ko": text } }))
        .fetch_one(pool)
        .await
        .expect("stored replacement")
}

/// A page the owner replaced is what customers read: the baseline records that text (custom), the inbox offers no
/// pack text for it as a new draft, and Publish (the owner's code, a recorded version) brings the pack text back.
#[sqlx::test(migrations = "../../migrations")]
async fn an_owner_replaced_page_is_the_baseline_until_the_owner_publishes(pool: PgPool) {
    let (app, token, staff) = configured(&pool).await;
    common::enrol_owner(&pool, staff, SEED).await;
    stored_replacement(&pool, "privacy", "우리 회사의 방침").await;
    let state = build_state(pool.clone(), test_config()).expect("state");
    assert_eq!(import_baseline(&state).await.expect("import"), 3);
    let (custom, template, html): (bool, Option<String>, String) =
        sqlx::query_as("SELECT custom, template_version, body_html FROM legal_documents WHERE doc = 'privacy'")
            .fetch_one(&pool)
            .await
            .expect("row");
    assert!(custom && template.is_none(), "recorded as the replaced text");
    assert!(html.contains("우리 회사의 방침"), "{html}");
    assert_eq!(fragment(&app, "legal_privacy").await, html, "the inbox and the website show one text");
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    assert_eq!(
        (doc(&inbox, "privacy")["custom"].clone(), doc(&inbox, "privacy")["pending"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(doc(&inbox, "terms")["custom"], false);
    let (st, b) = owner_publish(&app, &pool, &token, json!({ "doc": "privacy", "lang": "ko" })).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let counter = format!("{}.1", privacy::CONSENT_VERSION);
    assert_eq!(
        (b["version"].clone(), b["requires_reconsent"].clone()),
        (json!(counter), json!(true)),
        "after the replacement"
    );
    let live: i64 = sqlx::query_scalar("SELECT count(*) FROM content_items WHERE kind = 'legal' AND active")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(live, 0, "the replacement stopped with the publish");
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    let privacy_doc = doc(&inbox, "privacy");
    assert_eq!((privacy_doc["custom"].clone(), privacy_doc["pending"].clone()), (json!(false), json!(false)));
    assert_eq!(fragment(&app, "legal_privacy").await, privacy_doc["draft_html"].as_str().expect("draft"));
    assert_eq!(privacy::consent_versions(&pool).await.expect("versions").of("privacy"), counter);
}

/// Admin content no longer writes legal pages (the owner gate and the version record would be bypassed): the owner
/// replaces a page from the Legal inbox, which records it as a custom version with its audit entry.
#[sqlx::test(migrations = "../../migrations")]
async fn legal_replacements_go_through_the_inbox(pool: PgPool) {
    let (app, token, staff) = configured(&pool).await;
    common::enrol_owner(&pool, staff, SEED).await;
    let stored = stored_replacement(&pool, "terms", "예전 약관").await;
    let item = json!({"kind": "legal", "slug": "privacy", "title": {"ko": "개인정보처리방침"}, "body": {"text": {"ko": "몰래 바꾼 방침"}},
                      "image_url": null, "link": null, "sort_order": 0, "starts_at": null, "ends_at": null, "active": true});
    let refused = |b: &Value| b["error"]["code"] == "legal_use_inbox";
    let (st, b) = call(&app, "POST", "/v1/admin/content", Some(&token), Some(item.clone())).await;
    assert!(st == StatusCode::CONFLICT && refused(&b), "{b}");
    let mut notice = item.clone();
    notice["kind"] = json!("notice");
    let (st, b) = call(&app, "PUT", &format!("/v1/admin/content/{stored}"), Some(&token), Some(notice)).await;
    assert!(
        st == StatusCode::CONFLICT && refused(&b),
        "a stored legal page cannot be turned into another kind: {b}"
    );
    let (st, b) = call(&app, "DELETE", &format!("/v1/admin/content/{stored}"), Some(&token), None).await;
    assert!(st == StatusCode::CONFLICT && refused(&b), "{b}");
    assert!(fragment(&app, "legal_terms").await.contains("예전 약관"), "a stored replacement stays readable");

    let state = build_state(pool.clone(), test_config()).expect("state");
    import_baseline(&state).await.expect("import");
    let body = json!({ "doc": "privacy", "lang": "ko", "replacement": "우리 회사의 방침\n\n제2조", "requires_reconsent": false });
    let (st, _) = call(&app, "POST", "/v1/admin/legal/publish", Some(&token), Some(body.clone())).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "the owner's code is required");
    let (st, b) = owner_publish(&app, &pool, &token, json!({ "doc": "privacy", "lang": "ko", "replacement": "  " })).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    let (st, b) = owner_publish(&app, &pool, &token, body).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["requires_reconsent"], false);
    let version = b["version"].as_str().expect("version");
    assert!(legal::version_key(version) > legal::version_key(privacy::CONSENT_VERSION), "{version}");
    let (custom, template, html): (bool, Option<String>, String) =
        sqlx::query_as("SELECT custom, template_version, body_html FROM legal_documents WHERE id = $1")
            .bind(b["id"].as_i64().expect("id"))
            .fetch_one(&pool)
            .await
            .expect("row");
    assert!(custom && template.is_none());
    assert!(html.contains("<p>우리 회사의 방침</p>") && html.contains("<p>제2조</p>"), "{html}");
    assert_eq!(fragment(&app, "legal_privacy").await, html, "the website shows the recorded replacement");
    let audited: Vec<Value> = sqlx::query_scalar("SELECT data FROM audit_log WHERE action = 'legal.publish' AND target = 'privacy'")
        .fetch_all(&pool)
        .await
        .expect("audit");
    assert_eq!(audited.len(), 1);
    assert_eq!(audited[0]["custom"], true);
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    assert_eq!(doc(&inbox, "privacy")["custom"], true);
}

/// Company fields and processors reach customers only through Publish: the website keeps the published snapshot and
/// the inbox shows the new draft as pending.
#[sqlx::test(migrations = "../../migrations")]
async fn live_values_wait_for_publish(pool: PgPool) {
    let (app, token, staff) = configured(&pool).await;
    common::enrol_owner(&pool, staff, SEED).await;
    let state = build_state(pool.clone(), test_config()).expect("state");
    import_baseline(&state).await.expect("import");
    let mut business = kr_business();
    business["company"]["legal_name"] = json!("주식회사 새이름");
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(business)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let shown = fragment(&app, "legal_terms").await;
    assert!(shown.contains("주식회사 다해클린") && !shown.contains("주식회사 새이름"), "{shown}");
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    let terms = doc(&inbox, "terms");
    assert_eq!(terms["pending"], true);
    assert!(terms["draft_html"].as_str().expect("draft").contains("주식회사 새이름"));
    let (st, b) = owner_publish(&app, &pool, &token, json!({ "doc": "terms", "lang": "ko" })).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert!(fragment(&app, "legal_terms").await.contains("주식회사 새이름"), "published now");
}

/// One version is one text, also in the database.
#[sqlx::test(migrations = "../../migrations")]
async fn a_version_is_recorded_once(pool: PgPool) {
    publish_version(&pool, "privacy", NEW, true).await;
    let again = sqlx::query(
        "INSERT INTO legal_documents (doc, lang, version, requires_reconsent, body_md, body_html, body_sha256, effective_at)
         VALUES ('privacy', 'ko', $1, false, 'md', 'other', repeat('b', 64), CURRENT_DATE)",
    )
    .bind(NEW)
    .execute(&pool)
    .await;
    assert!(again.is_err(), "a second text under one version");
}

/// A publish the server refuses (unknown document, nothing new) leaves the owner's code unspent.
#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_publish_keeps_the_owner_code(pool: PgPool) {
    let (app, token, staff) = configured(&pool).await;
    common::enrol_owner(&pool, staff, SEED).await;
    let code = format!(
        "{:06}",
        api::totp::code_at(SEED, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    );
    let publish = |body: Value| call_owner(&app, "POST", "/v1/admin/legal/publish", &token, Some(&code), body);
    let (st, _) = publish(json!({ "doc": "terms", "lang": "en" })).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, b) = publish(json!({ "doc": "terms", "lang": "ko" })).await;
    assert_eq!(st, StatusCode::OK, "the same code still works: {b}");
    let (st, b) = publish(json!({ "doc": "terms", "lang": "ko" })).await;
    assert_eq!(st, StatusCode::CONFLICT, "spent: {b}");
}

/// The website (and the app, which opens it) shows the newest published text whenever it is not this release's pack
/// text, so the version a customer accepts is the text they could read.
#[sqlx::test(migrations = "../../migrations")]
async fn customers_read_the_text_their_consent_names(pool: PgPool) {
    let (app, _, _) = configured(&pool).await;
    let state = build_state(pool.clone(), test_config()).expect("state");
    import_baseline(&state).await.expect("import");
    let baseline: String = sqlx::query_scalar("SELECT body_html FROM legal_documents WHERE doc = 'terms'")
        .fetch_one(&pool)
        .await
        .expect("terms");
    assert_eq!(
        legal::published_text(&state, "terms").await.expect("lookup"),
        Some(baseline.clone()),
        "the published snapshot, not the live pack text"
    );
    let c = common::customer(&app, &pool, "010-8200-0011").await;
    publish_version(&pool, "privacy", NEW, true).await;
    assert_eq!(
        fragment(&app, "legal_privacy").await,
        format!("<p>{NEW}</p>"),
        "the website shows the published text"
    );
    assert_eq!(fragment(&app, "legal_terms").await, baseline, "terms keep their published text");
    let (_, me) = call(&app, "GET", "/v1/me", Some(c.token.as_str()), None).await;
    let shown = me["pending_consents"][0]["version"].clone();
    assert_eq!(shown, json!(NEW));
    let accept = json!({ "consents": [{ "kind": "privacy", "version": shown }] });
    let (st, b) = call(&app, "POST", "/v1/me/consents/accept", Some(c.token.as_str()), Some(accept)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let recorded: String = sqlx::query_scalar("SELECT version FROM user_consents WHERE user_id = $1 AND kind = 'privacy' ORDER BY id DESC LIMIT 1")
        .bind(c.user)
        .fetch_one(&pool)
        .await
        .expect("row");
    assert_eq!(format!("<p>{recorded}</p>"), fragment(&app, "legal_privacy").await);
}

/// A correction (no re-consent) is the text shown: signing up, updating and accepting with its version all work, and
/// nobody is asked again.
#[sqlx::test(migrations = "../../migrations")]
async fn a_correction_changes_nothing_for_customers(pool: PgPool) {
    let app = common::app(pool.clone());
    let fix = "2026-11-20";
    publish_version(&pool, "privacy", fix, false).await;
    // An app sending the single version GET /me/consents reports signs up on it; terms record their own version.
    let (st, tokens) = signup(&app, "010-8200-0021", fix).await;
    assert_eq!(st, StatusCode::OK, "{tokens}");
    let (_, me) = call(&app, "GET", "/v1/me", tokens["access_token"].as_str(), None).await;
    assert!(pending_kinds(&me).is_empty(), "{me}");
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT kind, version FROM user_consents WHERE user_id = $1::uuid ORDER BY kind")
        .bind(tokens["subject"].as_str().expect("subject"))
        .fetch_all(&pool)
        .await
        .expect("rows");
    let want = [("overseas_transfer", fix), ("privacy", fix), ("terms", privacy::CONSENT_VERSION)];
    assert_eq!(rows, want.map(|(k, v)| (k.to_string(), v.to_string())));
    let c = common::customer(&app, &pool, "010-8200-0022").await;
    let (_, s) = call(&app, "GET", "/v1/me/consents", Some(c.token.as_str()), None).await;
    assert_eq!((s["version"].clone(), s["reconsent_required"].clone()), (json!(fix), json!(false)), "{s}");
    let put = json!({ "consents": [{ "kind": "privacy", "granted": true, "version": fix }, { "kind": "marketing", "granted": true, "version": fix }] });
    let (st, s) = call(&app, "PUT", "/v1/me/consents", Some(c.token.as_str()), Some(put)).await;
    assert_eq!(st, StatusCode::OK, "{s}");
    let accept = json!({ "consents": [{ "kind": "overseas_transfer", "version": fix }] });
    let (st, b) = call(&app, "POST", "/v1/me/consents/accept", Some(c.token.as_str()), Some(accept)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert!(pending_kinds(&b).is_empty(), "{b}");
    assert!(privacy::marketing_allowed(&pool, c.user).await.expect("marketing"));
}

/// After a rollback the older release never offers its older text as new, and refuses to publish it over the newer one.
#[sqlx::test(migrations = "../../migrations")]
async fn a_rolled_back_release_never_publishes_an_older_text(pool: PgPool) {
    let (app, token, staff) = configured(&pool).await;
    common::enrol_owner(&pool, staff, SEED).await;
    let state = build_state(pool.clone(), test_config()).expect("state");
    import_baseline(&state).await.expect("import");
    publish_version(&pool, "privacy", NEW, true).await;
    let (_, inbox) = call(&app, "GET", "/v1/admin/legal", Some(&token), None).await;
    assert_eq!(doc(&inbox, "privacy")["pending"], false, "{}", doc(&inbox, "privacy"));
    let (st, b) = owner_publish(&app, &pool, &token, json!({ "doc": "privacy", "lang": "ko" })).await;
    assert_eq!((st, b["error"]["code"].clone()), (StatusCode::CONFLICT, json!("legal_version_older")), "{b}");
    assert_eq!(privacy::consent_versions(&pool).await.expect("versions").of("privacy"), NEW);
    assert_eq!(
        fragment(&app, "legal_privacy").await,
        format!("<p>{NEW}</p>"),
        "the newer text stays the one shown"
    );
}

/// A text published to take effect later moves nothing until its day: old grants keep holding meanwhile.
#[sqlx::test(migrations = "../../migrations")]
async fn a_text_announced_for_later_asks_nothing_until_it_takes_effect(pool: PgPool) {
    let app = common::app(pool.clone());
    let c = common::customer(&app, &pool, "010-8200-0031").await;
    publish_effective(&pool, "privacy", NEW, true, 10).await;
    let v = privacy::consent_versions(&pool).await.expect("versions");
    assert_eq!((v.of("privacy"), v.current("privacy")), (privacy::CONSENT_VERSION, privacy::CONSENT_VERSION));
    let (_, me) = call(&app, "GET", "/v1/me", Some(c.token.as_str()), None).await;
    assert!(pending_kinds(&me).is_empty(), "{me}");
    assert!(privacy::overseas_allowed(&pool, c.user).await.expect("overseas"));
    assert_eq!(fragment(&app, "legal_privacy").await, "", "the website keeps the text in force");
}
