// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Website fragments (`/v1/public/seller-info?field=`) over the real router: brand fields, JSON-LD, live
//! settings, provider status and published legal texts.

mod common;

use api::auth::Role;
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use common::call;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;

const FIELDS: [&str; 26] = [
    "name",
    "name_en",
    "short_name",
    "tagline",
    "tagline_en",
    "base_url",
    "primary_color",
    "logo_img",
    "favicon",
    "apple_icon",
    "og_image",
    "ld_json",
    "privacy_email",
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
];

async fn fragment(app: &Router, field: &str) -> (StatusCode, String) {
    let mut req = Request::builder()
        .uri(format!("/v1/public/seller-info?field={field}"))
        .body(Body::empty())
        .expect("request");
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let resp = app.clone().oneshot(req).await.expect("router");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
    (status, String::from_utf8(bytes.to_vec()).expect("utf-8"))
}

async fn admin(pool: &PgPool) -> String {
    common::staff_token(pool, Role::StaffAdmin).await.0
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_new_field_answers_and_is_escaped(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = admin(&pool).await;
    let evil = "<b>반짝</b> {{env \"DATA_KEY\"}}";
    let profile = json!({"name": evil, "name_en": evil, "short_name": "반짝", "tagline": evil,
                         "company": {"privacy_email": "<x@y>"}});
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(profile)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    for f in FIELDS {
        let (st, html) = fragment(&app, f).await;
        assert_eq!(st, StatusCode::OK, "{f}");
        assert!(!html.contains("<b>") && !html.contains("<x@y>"), "{f} not escaped: {html}");
        assert!(!html.contains("{{"), "{f} would be run by Caddy's template engine: {html}");
    }
    let (_, name) = fragment(&app, "name").await;
    assert_eq!(name, "&lt;b&gt;반짝&lt;/b&gt; &#123;&#123;env &quot;DATA_KEY&quot;&#125;&#125;");
    let (_, base) = fragment(&app, "base_url").await;
    assert_eq!(base, common::BASE_URL);
    let (st, _) = fragment(&app, "password").await;
    assert_eq!(st, StatusCode::NOT_FOUND, "unknown fields stay 404");
}

#[sqlx::test(migrations = "../../migrations")]
async fn ld_json_is_json_and_cannot_close_the_script(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = admin(&pool).await;
    call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(json!({"name": "</script>반짝"}))).await;
    let (st, html) = fragment(&app, "ld_json").await;
    assert_eq!(st, StatusCode::OK);
    let inner = html
        .strip_prefix("<script type=\"application/ld+json\">")
        .and_then(|h| h.strip_suffix("</script>"))
        .expect("script element");
    assert!(!inner.contains("</"), "{inner}");
    let v: Value = serde_json::from_str(inner).expect("JSON");
    assert_eq!(v["@graph"][0]["name"], "</script>반짝");
    assert_eq!(v["@graph"][0]["url"], format!("{}/", common::BASE_URL));
}

#[sqlx::test(migrations = "../../migrations")]
async fn legal_text_is_empty_until_published_then_escaped(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = admin(&pool).await;
    let (_, html) = fragment(&app, "legal_location").await;
    assert_eq!(html, "", "no published text: the page keeps its built-in body");
    let item = json!({"kind": "legal", "slug": "terms", "title": {"ko": "이용약관"}, "body": {"text": {"ko": "제1조 <script>\n\n제2조"}},
                      "image_url": null, "link": null, "sort_order": 0, "starts_at": null, "ends_at": null, "active": true});
    let (st, b) = call(&app, "POST", "/v1/admin/content", Some(&token), Some(item)).await;
    assert_eq!((st, b["error"]["code"].clone()), (StatusCode::CONFLICT, json!("legal_use_inbox")), "{b}");
    // A replacement stored before the Legal inbox took them over stays readable.
    sqlx::query(r#"INSERT INTO content_items (kind, slug, title, body) VALUES ('legal', 'terms', '{"ko": "이용약관"}', $1)"#)
        .bind(json!({"text": {"ko": "제1조 <script>\n\n제2조"}}))
        .execute(&pool)
        .await
        .expect("stored replacement");
    let (_, html) = fragment(&app, "legal_terms").await;
    assert!(
        html.contains("<h1>이용약관</h1>") && html.contains("<p>제1조 &lt;script&gt;</p>") && html.contains("<p>제2조</p>"),
        "{html}"
    );
    let (_, other) = fragment(&app, "legal_privacy").await;
    assert_eq!(other, "", "other slugs unaffected");
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_policy_and_retention_follow_the_console(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = admin(&pool).await;
    let (_, rows) = fragment(&app, "cancel_policy").await;
    assert!(rows.starts_with("<li>방문일 2일 전까지 취소: 전액 환불</li>"), "built-in default: {rows}");
    let policy = json!({"tiers": [{"min_days": 5, "percent": 0}, {"min_days": 0, "percent": 50}], "min_fee": 5000, "reschedule_min_days": 2});
    let (st, b) = call(&app, "PUT", "/v1/admin/cancel-policy", Some(&token), Some(policy)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, rows) = fragment(&app, "cancel_policy").await;
    assert_eq!(
        rows,
        "<li>방문일 5일 전까지 취소: 전액 환불</li><li>방문 당일 취소: 결제 금액의 50% 취소 수수료 (최소 5,000원)</li>"
    );
    let (_, days) = fragment(&app, "ai_chat_retention_days").await;
    assert_eq!(days, "180");
}

#[sqlx::test(migrations = "../../migrations")]
async fn provider_status_flips_with_its_card(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = admin(&pool).await;
    let (_, before) = fragment(&app, "uses_ai").await;
    assert_eq!(before, "현재 이용하지 않음");
    let (st, b) = call(
        &app,
        "PUT",
        "/v1/admin/setup/ai",
        Some(&token),
        Some(json!({"api_key": "sk-ant-test-key", "mode": "always_first"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, after) = fragment(&app, "uses_ai").await;
    assert_eq!(after, "이용 중");
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_meta_carries_saved_search_tokens_for_the_home_page(pool: PgPool) {
    let app = common::app(pool.clone());
    let (st, empty) = fragment(&app, "verify_meta").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(empty, "", "no token saved: nothing in <head>");
    let token = admin(&pool).await;
    let profile = json!({"name": "우집사", "google_site_verification": "g0ogLe-Tok_en",
                         "naver_site_verification": "e64f527b44d14ffc7bd4612e454ff068565064ce"});
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/business", Some(&token), Some(profile)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (_, html) = fragment(&app, "verify_meta").await;
    assert!(html.contains("<meta name=\"google-site-verification\" content=\"g0ogLe-Tok_en\">"), "{html}");
    assert!(
        html.contains("<meta name=\"naver-site-verification\" content=\"e64f527b44d14ffc7bd4612e454ff068565064ce\">"),
        "{html}"
    );
}
