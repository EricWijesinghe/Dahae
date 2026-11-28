// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Setup-wizard steps 0 and 1 (languages, country with its preview card and diff), the go-live gates (D10) and
//! preview mode: an install that chose its country in the wizard stays in preview until the hard gates are green,
//! and refuses checkouts on live keys meanwhile; an install from before the country flow never changes. Also the
//! language columns and their N-1 safety. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{auth::Role, pack, routes::setup::install::first_boot};
use axum::{Router, http::StatusCode};
use common::{call, staff_token};
use serde_json::{Value, json};
use sqlx::PgPool;

const SEED: &[u8] = b"golive-owner-seed-00001";

async fn owner(db: &PgPool) -> String {
    let (token, id) = staff_token(db, Role::StaffAdmin).await;
    common::enrol_owner(db, id, SEED).await;
    token
}

async fn stored(db: &PgPool, key: &str) -> Option<Value> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(format!("reg.{key}"))
        .fetch_optional(db)
        .await
        .expect("settings read")
}

async fn set(db: &PgPool, key: &str, value: Value) {
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = $2")
        .bind(key)
        .bind(value)
        .execute(db)
        .await
        .expect("settings write");
}

async fn get(app: &Router, uri: &str, token: &str) -> Value {
    let (st, b) = call(app, "GET", uri, Some(token), None).await;
    assert_eq!(st, StatusCode::OK, "{uri}: {b}");
    b
}

fn red(gates: &Value) -> Vec<String> {
    gates["gates"]
        .as_array()
        .expect("gates")
        .iter()
        .filter(|g| g["hard"] == true && g["ok"] == false)
        .map(|g| g["id"].as_str().expect("id").to_string())
        .collect()
}

/// A fresh install (no staff at first boot) picks KR in the wizard: preview card, diff, the owner's PUT, then the
/// gates are enforced and the public site reports preview.
#[sqlx::test(migrations = "../../migrations")]
async fn a_fresh_install_chooses_its_country_and_stays_in_preview(pool: PgPool) {
    let kr = pack::load("KR").expect("KR");
    assert!(!first_boot(&pool, &kr).await.expect("first boot"), "fresh: nothing set");
    let app = common::app(pool.clone());
    let token = owner(&pool).await;

    let packs = get(&app, "/v1/admin/setup/packs", &token).await;
    assert_eq!(packs["current"]["country"], Value::Null);
    assert_eq!(packs["current"]["locked"], false);
    assert_eq!(packs["default"], "KR");
    let card = packs["packs"]
        .as_array()
        .expect("packs")
        .iter()
        .find(|p| p["id"] == "KR")
        .expect("KR card")
        .clone();
    assert_eq!((card["currency"].clone(), card["draft"].clone()), (json!("KRW"), json!(false)));
    assert_eq!(card["legal"], json!(["terms", "privacy", "location_terms"]));
    assert_eq!(card["recommended"]["payments"], json!(["portone", "tosspayments"]));
    assert!(!card["recommended"].to_string().contains("stripe"), "Dark adapters are never offered");

    let preview = get(&app, "/v1/admin/setup/country/preview?country=KR", &token).await;
    let keys: Vec<&str> = preview["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|c| c["key"].as_str().expect("key"))
        .collect();
    assert_eq!(keys, ["country", "currency", "tz", "locales"]);
    assert_eq!(preview["allowed"], true);
    let (st, b) = call(&app, "GET", "/v1/admin/setup/country/preview?country=XX", Some(&token), None).await;
    assert_eq!((st, b["error"]["code"].clone()), (StatusCode::BAD_REQUEST, json!("unknown_country")), "{b}");

    // Only the owner sets the country.
    let (admin, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (st, _) = call(&app, "PUT", "/v1/admin/setup/country", Some(&admin), Some(json!({ "country": "KR" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/country", Some(&token), Some(json!({ "country": "GB" }))).await;
    assert_eq!(
        (st, b["error"]["code"].clone()),
        (StatusCode::BAD_REQUEST, json!("unknown_country")),
        "no GB pack yet: {b}"
    );

    let (st, b) = call(&app, "PUT", "/v1/admin/setup/country", Some(&token), Some(json!({ "country": "KR" }))).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["current"]["country"], "KR");
    assert_eq!(stored(&pool, "install.currency").await, Some(json!("KRW")));
    assert_eq!(stored(&pool, "install.tz").await, Some(json!("Asia/Seoul")));
    assert_eq!(stored(&pool, "install.locales").await, Some(json!(["ko", "en"])));
    assert_eq!(stored(&pool, "install.pack_version_applied").await, Some(json!(kr.version)));
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'install.country'")
        .fetch_one(&pool)
        .await
        .expect("audit");
    assert_eq!(audited, 1);
    let again = get(&app, "/v1/admin/setup/country/preview?country=KR", &token).await;
    assert_eq!(again["changes"], json!([]), "nothing left to change");

    // Dev mode, no legal text published, no business details: preview, every red gate naming its step.
    let gates = get(&app, "/v1/admin/setup/gates", &token).await;
    assert_eq!((gates["enforced"].clone(), gates["preview"].clone()), (json!(true), json!(true)), "{gates}");
    assert_eq!(red(&gates), ["business_fields", "payment_live_keys", "legal_published"]);
    assert_eq!(gates["blocking"], json!(["business_fields", "legal_published"]));
    let steps: Vec<(&str, &str)> = gates["gates"]
        .as_array()
        .expect("gates")
        .iter()
        .map(|g| (g["id"].as_str().expect("id"), g["step"].as_str().expect("step")))
        .collect();
    assert!(
        steps.contains(&("legal_published", "legal")) && steps.contains(&("business_fields", "business")),
        "{steps:?}"
    );
    let (_, public) = call(&app, "GET", "/v1/setup", None, None).await;
    assert_eq!(public["preview"], true, "{public}");

    // Business details turn their gate green.
    set(
        &pool,
        "business",
        json!({ "company": { "legal_name": "다해", "ceo": "대표", "registration_no": "1234567891", "address": "서울" } }),
    )
    .await;
    assert_eq!(red(&get(&app, "/v1/admin/setup/gates", &token).await), ["payment_live_keys", "legal_published"]);

    // After the first live payment the country is locked.
    set(&pool, "reg.install.locked_at", json!("2026-11-28T00:00:00Z")).await;
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/country", Some(&token), Some(json!({ "country": "KR" }))).await;
    assert_eq!((st, b["error"]["code"].clone()), (StatusCode::CONFLICT, json!("country_locked")), "{b}");
    let locked = get(&app, "/v1/admin/setup/country/preview?country=KR", &token).await;
    assert_eq!((locked["allowed"].clone(), locked["reason"].clone()), (json!(false), json!("country_locked")));
}

/// An install set to KR at first boot keeps its behaviour: gates are reported, never enforced, no preview banner.
#[sqlx::test(migrations = "../../migrations")]
async fn an_existing_install_is_never_put_in_preview(pool: PgPool) {
    let token = owner(&pool).await;
    assert!(
        first_boot(&pool, &pack::load("KR").expect("KR")).await.expect("first boot"),
        "existing: auto KR"
    );
    let app = common::app(pool.clone());
    let gates = get(&app, "/v1/admin/setup/gates", &token).await;
    assert_eq!(gates["enforced"], false, "{gates}");
    let (_, public) = call(&app, "GET", "/v1/setup", None, None).await;
    assert_eq!(public["preview"], false, "{public}");
}

/// "How you sell": a model the pack lists; the split model keeps the site in preview without a split gateway.
#[sqlx::test(migrations = "../../migrations")]
async fn the_marketplace_model_is_checked_against_the_pack(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = owner(&pool).await;
    let (st, b) = call(
        &app,
        "PUT",
        "/v1/admin/setup/marketplace",
        Some(&token),
        Some(json!({ "model": "pros_hold_money" })),
    )
    .await;
    assert_eq!(
        (st, b["error"]["code"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_marketplace_model")),
        "{b}"
    );
    let (st, b) = call(
        &app,
        "PUT",
        "/v1/admin/setup/marketplace",
        Some(&token),
        Some(json!({ "model": "split_by_psp" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert!(red(&b).contains(&"marketplace_model".to_string()), "no KR gateway splits: {b}");
    let (_, b) = call(
        &app,
        "PUT",
        "/v1/admin/setup/marketplace",
        Some(&token),
        Some(json!({ "model": "merchant_of_record" })),
    )
    .await;
    assert!(!red(&b).contains(&"marketplace_model".to_string()), "{b}");
}

/// A draft pack needs the owner's tick before going live; the tick is recorded with the country.
#[sqlx::test(migrations = "../../migrations")]
async fn a_draft_pack_needs_the_owners_acknowledgement(pool: PgPool) {
    let app = common::app_with_pack(pool.clone(), |v| v["maturity"] = json!({ "level": "draft" }));
    let token = owner(&pool).await;
    let gates = get(&app, "/v1/admin/setup/gates", &token).await;
    assert!(red(&gates).contains(&"draft_pack_ack".to_string()), "{gates}");
    let packs = get(&app, "/v1/admin/setup/packs", &token).await;
    assert_eq!(packs["packs"][0]["draft"], true, "the card shows the running pack: {packs}");
    let (st, _) = call(&app, "PUT", "/v1/admin/setup/country", Some(&token), Some(json!({ "country": "KR" }))).await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        red(&get(&app, "/v1/admin/setup/gates", &token).await).contains(&"draft_pack_ack".to_string()),
        "no tick yet"
    );
    let (st, _) = call(
        &app,
        "PUT",
        "/v1/admin/setup/country",
        Some(&token),
        Some(json!({ "country": "KR", "draft_ack": true })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        stored(&pool, "install.draft_ack")
            .await
            .is_some_and(|v| v.as_str().is_some_and(|t| !t.is_empty()))
    );
    assert!(!red(&get(&app, "/v1/admin/setup/gates", &token).await).contains(&"draft_pack_ack".to_string()));
}

/// Preview mode refuses a checkout on live keys while a hard gate is red, and only for an install that chose its
/// country in the wizard; test keys (dev mode here) keep taking test payments.
#[sqlx::test(migrations = "../../migrations")]
async fn preview_mode_refuses_live_checkouts_until_the_gates_are_green(pool: PgPool) {
    common::seed_catalog(&pool).await;
    let dev = common::app(pool.clone());
    let c = common::customer(&dev, &pool, "01070000881").await;
    let live = common::app_real_gateway_with_channels(pool.clone(), &["card"]);
    let checkout = |app: Router, days: i64| {
        let token = c.token.clone();
        let address = c.address;
        async move {
            let visit = api::util::local_today() + chrono::Duration::days(days);
            let body = json!({ "kind": "booking", "method": "card",
                "booking": { "service_code": "studio", "address_id": address, "visit_date": visit, "start_hour": 9, "visit_notes": {} } });
            call(&app, "POST", "/v1/checkouts", Some(token.as_str()), Some(body)).await
        }
    };
    // Before the country flow: no change.
    let (st, b) = checkout(live.clone(), 5).await;
    assert_eq!(st, StatusCode::OK, "{b}");

    // Chosen in the wizard (fresh marker, country set): live keys refused while legal texts are unpublished.
    set(&pool, "install.first_boot", json!({ "auto_country": null })).await;
    set(&pool, "reg.install.country", json!("KR")).await;
    let (st, b) = checkout(live.clone(), 6).await;
    assert_eq!((st, b["error"]["code"].clone()), (StatusCode::CONFLICT, json!("site_in_preview")), "{b}");
    let (st, b) = checkout(dev.clone(), 7).await;
    assert_eq!(st, StatusCode::OK, "test payments still work in preview: {b}");

    // Business details and the published texts turn the gates green: live checkout goes through.
    set(
        &pool,
        "business",
        json!({ "company": { "legal_name": "다해", "ceo": "대표", "registration_no": "1234567891", "address": "서울" } }),
    )
    .await;
    for doc in ["terms", "privacy", "location_terms"] {
        sqlx::query(
            "INSERT INTO legal_documents (doc, lang, version, requires_reconsent, body_md, body_html, body_sha256, effective_at)
             VALUES ($1, 'ko', '2026-01-01', false, 'x', 'x', repeat('a', 64), '2026-01-01')",
        )
        .bind(doc)
        .execute(&pool)
        .await
        .expect("published text");
    }
    let (st, b) = checkout(live, 8).await;
    assert_eq!(st, StatusCode::OK, "{b}");
}

/// Languages (step 0): the install's console and customer languages, and a staff member's own.
#[sqlx::test(migrations = "../../migrations")]
async fn languages_are_shipped_locales_and_staff_keep_their_own(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = owner(&pool).await;
    let l = get(&app, "/v1/admin/setup/languages", &token).await;
    let codes: Vec<&str> = l["shipped"]
        .as_array()
        .expect("shipped")
        .iter()
        .map(|c| c["code"].as_str().expect("code"))
        .collect();
    assert_eq!(codes, ["ko", "en", "ar"]);
    assert_eq!(l["shipped"][2]["rtl"], true);
    assert_eq!(l["my_locale"], Value::Null);

    let (st, b) = call(
        &app,
        "PUT",
        "/v1/admin/setup/languages",
        Some(&token),
        Some(json!({ "console_locale": "en", "locales": ["en", "ar"], "my_locale": "ar" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(
        (b["console_locale"].clone(), b["locales"].clone(), b["my_locale"].clone()),
        (json!("en"), json!(["en", "ar"]), json!("ar"))
    );
    for bad in [
        json!({ "console_locale": "xx" }),
        json!({ "locales": [] }),
        json!({ "locales": ["ko", "ko"] }),
        json!({ "my_locale": "xx" }),
    ] {
        let (st, b) = call(&app, "PUT", "/v1/admin/setup/languages", Some(&token), Some(bad.clone())).await;
        assert_eq!((st, b["error"]["code"].clone()), (StatusCode::BAD_REQUEST, json!("invalid_locale")), "{bad}");
    }
    assert_eq!(
        stored(&pool, "install.console_locale").await,
        Some(json!("en")),
        "a refused save changes nothing"
    );

    // A consultant sets their own language, never the install's.
    let (consultant, id) = staff_token(&pool, Role::StaffConsultant).await;
    let (st, _) = call(
        &app,
        "PUT",
        "/v1/admin/setup/languages",
        Some(&consultant),
        Some(json!({ "console_locale": "ko" })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, b) = call(&app, "PUT", "/v1/admin/setup/languages", Some(&consultant), Some(json!({ "my_locale": "ko" }))).await;
    assert_eq!((st, b["my_locale"].clone()), (StatusCode::OK, json!("ko")), "{b}");
    let (_, b) = call(&app, "PUT", "/v1/admin/setup/languages", Some(&consultant), Some(json!({ "my_locale": "" }))).await;
    assert_eq!(b["my_locale"], Value::Null, "cleared: follows the install");
    let row: Option<String> = sqlx::query_scalar("SELECT locale FROM staff WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("staff");
    assert_eq!(row, None);
}

/// N-1 on N's schema: the previous release inserts staff without a language and reads any stored customer
/// language as Korean or English; the new column refuses garbage.
#[sqlx::test(migrations = "../../migrations")]
async fn language_columns_are_safe_for_the_previous_release(pool: PgPool) {
    sqlx::query("INSERT INTO staff (email, name, password_hash, role) VALUES ('n1@test.local', 'n-1', 'x', 'admin')")
        .execute(&pool)
        .await
        .expect("an N-1 staff insert names no language");
    let bad = sqlx::query("UPDATE staff SET locale = 'english' WHERE email = 'n1@test.local'")
        .execute(&pool)
        .await;
    assert!(bad.is_err(), "the column takes a locale code only");
    let user: uuid::Uuid = sqlx::query_scalar("INSERT INTO users (phone, name, locale) VALUES ('01070000882', 'ar', 'ar') RETURNING id")
        .fetch_one(&pool)
        .await
        .expect("an Arabic customer");
    let stored: String = sqlx::query_scalar("SELECT locale FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .expect("read");
    // N-1's `i18n::shipped` maps every code but ko to English, so an Arabic account gets English server text there.
    assert_eq!(api::i18n::shipped(&stored), "en");
}
