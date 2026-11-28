// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `GET /v1/admin/setup/catalog`: the adapters a KR install offers, by capability and in the pack's order, with
//! their card state, fields and docs links. Secrets are write-only on every setup response: accepted on save,
//! shown only as set, never returned in any form.

mod common;

use api::auth::Role;
use axum::http::StatusCode;
use common::{call, call_owner};
use serde_json::{Value, json};
use sqlx::PgPool;

const SEED: &[u8] = b"catalog-owner-seed-0001";

async fn owner(db: &PgPool) -> String {
    let (token, id) = common::staff_token(db, Role::StaffAdmin).await;
    common::enrol_owner(db, id, SEED).await;
    token
}

fn owner_code() -> String {
    format!(
        "{:06}",
        api::totp::code_at(SEED, (chrono::Utc::now().timestamp() / api::totp::STEP_SECS) as u64)
    )
}

async fn catalog(app: &axum::Router, token: &str) -> Value {
    let (st, b) = call(app, "GET", "/v1/admin/setup/catalog", Some(token), None).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    b
}

fn adapter<'a>(cat: &'a Value, id: &str) -> &'a Value {
    cat["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .flat_map(|s| s["adapters"].as_array().expect("adapters"))
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("{id} in the catalog"))
}

#[sqlx::test(migrations = "../../migrations")]
async fn catalog_lists_the_kr_adapters_by_capability_in_pack_order(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = owner(&pool).await;
    let cat = catalog(&app, &token).await;

    assert_eq!(cat["country"], "KR");
    let layout: Vec<(String, Vec<String>)> = cat["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let ids = s["adapters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["id"].as_str().unwrap().to_string())
                .collect();
            (s["capability"].as_str().unwrap().to_string(), ids)
        })
        .collect();
    let want = [
        ("payments", vec!["portone", "tosspayments"]),
        ("payouts", vec!["manual", "toss"]),
        ("messaging", vec!["solapi"]),
        ("login", vec!["kakao", "naver", "google", "apple"]),
        ("address", vec!["juso"]),
    ];
    let want: Vec<(String, Vec<String>)> = want
        .iter()
        .map(|(c, ids)| (c.to_string(), ids.iter().map(|i| i.to_string()).collect()))
        .collect();
    assert_eq!(layout, want, "Deprecated pg is never offered; recommended adapters come first");

    let portone = adapter(&cat, "portone");
    assert_eq!(portone["name"], "PortOne");
    assert_eq!(portone["section"], "portone");
    assert_eq!(portone["recommended"], true);
    assert_eq!(portone["configured"], false);
    assert_eq!(portone["source"], "none");
    assert_eq!(portone["owner_code"], true, "the first save of a gateway card needs the owner");
    assert_eq!(portone["docs"]["keys_url"], "https://admin.portone.io");
    assert_eq!(portone["webhook_url"], format!("{}/v1/webhooks/portone", common::BASE_URL));
    assert!(portone["caps"].as_array().unwrap().contains(&json!("partial_refund")));
    let names: Vec<&str> = portone["fields"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["store_id", "api_secret", "webhook_secret", "channel_keys", "cash_receipts"]);
    for f in portone["fields"].as_array().unwrap() {
        let mut keys: Vec<&String> = f.as_object().unwrap().keys().collect();
        keys.sort();
        if f["kind"] == "toggle" {
            assert_eq!(keys, ["kind", "name", "required", "set", "value"], "a switch shows its position: {f}");
            assert!(f["value"].is_boolean());
        } else {
            assert_eq!(keys, ["kind", "name", "required", "set"], "a field carries no value: {f}");
        }
    }

    let manual = adapter(&cat, "manual");
    assert_eq!((manual["source"].as_str(), manual["configured"].as_bool()), (Some("builtin"), Some(true)));
    assert_eq!(manual["fields"], json!([]));
    assert_eq!(adapter(&cat, "solapi")["webhook_url"], Value::Null);
    assert_eq!(adapter(&cat, "solapi")["owner_code"], false);
    assert_eq!(adapter(&cat, "toss")["webhook_url"], format!("{}/v1/webhooks/toss", common::BASE_URL));
}

#[sqlx::test(migrations = "../../migrations")]
async fn catalog_is_for_staff_admins_only(pool: PgPool) {
    let app = common::app(pool.clone());
    let (customer, _) = common::login(&app, "010-5000-0910", "customer").await;
    let (st, _) = call(&app, "GET", "/v1/admin/setup/catalog", Some(customer.as_str()), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call(&app, "GET", "/v1/admin/setup/catalog", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn saved_secrets_are_never_returned_and_show_only_as_set(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = owner(&pool).await;
    let t = Some(token.as_str());
    let secrets = [
        "solapi-key-SECRET-0001",
        "solapi-secret-SECRET-0002",
        "TPL-SECRET-0003",
        "juso-SECRET-0004",
        "naver-SECRET-0005",
        "portone-api-SECRET-0006",
    ];
    let mut replies = Vec::new();
    for (path, body) in [
        (
            "/v1/admin/setup/solapi",
            json!({"api_key": secrets[0], "api_secret": secrets[1], "sender": "0212345678", "templates": {"booking.confirmed": secrets[2]}}),
        ),
        ("/v1/admin/setup/juso", json!({"confm_key": secrets[3]})),
        ("/v1/admin/setup/naver", json!({"client_id": "naver-client", "client_secret": secrets[4]})),
    ] {
        let (st, b) = call(&app, "PUT", path, t, Some(body)).await;
        assert_eq!(st, StatusCode::OK, "{path}: {b}");
        replies.push(b);
    }
    let (st, b) = call_owner(
        &app,
        "PUT",
        "/v1/admin/setup/portone",
        &token,
        Some(&owner_code()),
        json!({"store_id": "store-1", "api_secret": secrets[5]}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    replies.push(b);

    let (st, overview) = call(&app, "GET", "/v1/admin/setup", t, None).await;
    assert_eq!(st, StatusCode::OK);
    replies.push(overview);
    let cat = catalog(&app, &token).await;
    replies.push(cat.clone());
    for reply in &replies {
        let text = reply.to_string();
        for s in secrets {
            assert!(!text.contains(s), "{s} came back: {text}");
        }
    }

    let solapi = adapter(&cat, "solapi");
    assert_eq!(solapi["configured"], true);
    let set: Vec<(&str, bool)> = solapi["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["name"].as_str().unwrap(), f["set"].as_bool().unwrap()))
        .collect();
    assert_eq!(
        set,
        [("api_key", true), ("api_secret", true), ("sender", true), ("pf_id", false), ("templates", true)]
    );
    let portone = adapter(&cat, "portone");
    assert_eq!((portone["locked"].as_bool(), portone["owner_code"].as_bool()), (Some(true), Some(false)));

    // Accepted on write: the stored value is the one sent.
    let stored = api::integrations::load_section(&pool, &common::test_config().data_key, api::integrations::Section::Juso)
        .await
        .expect("juso");
    assert_eq!(stored["confm_key"], secrets[3]);
}
