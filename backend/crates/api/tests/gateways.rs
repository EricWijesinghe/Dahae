// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! PortOne, Toss Payments, both or neither: customers are offered what is set up, one gateway per method.

mod common;

use api::{auth::Role, config::PortOne};
use axum::http::StatusCode;
use common::{
    Customer, PORTONE_SECRET, TossEnv, call, customer, prod_app_gateways, seed_catalog, set_toss, staff_token,
    toss::{self, BILLING_SK, Shared, WINDOW_CK},
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// PortOne with secret and store and channel keys for `methods` (none: PortOne is not set up).
fn portone(methods: &[&str]) -> PortOne {
    if methods.is_empty() {
        return PortOne::default();
    }
    PortOne {
        api_secret: Some(PORTONE_SECRET.into()),
        store_id: Some("store-test".into()),
        channel_keys: methods.iter().map(|m| (m.to_string(), format!("channel-key-{m}"))).collect(),
        cash_receipts: true,
        ..PortOne::default()
    }
}

/// A production app (PortOne mock unreachable: no test here calls PortOne) with the Toss card when given.
async fn env(pool: &PgPool, phone: &str, po: PortOne, toss_card: Option<serde_json::Map<String, Value>>) -> (TossEnv, Shared, Customer) {
    seed_catalog(pool).await;
    let (toss_base, fake) = toss::start().await;
    let c = customer(&common::app(pool.clone()), pool, phone).await;
    let env = prod_app_gateways(pool.clone(), &toss_base, "http://127.0.0.1:9", po).await;
    if let Some(card) = toss_card {
        set_toss(&env, card).await;
    }
    (env, fake, c)
}

async fn checkout(env: &TossEnv, c: &Customer, method: &str) -> (StatusCode, Value) {
    let visit = api::util::local_today() + chrono::Duration::days(5);
    let body = json!({ "kind": "booking", "method": method,
        "booking": { "service_code": "studio", "address_id": c.address, "visit_date": visit, "start_hour": 9, "visit_notes": {} } });
    call(&env.app, "POST", "/v1/checkouts", Some(c.token.as_str()), Some(body)).await
}

/// What a current app sends: it opens both gateways' pages.
const BOTH: &str = "?gateways=portone,tosspayments";

/// `(code, flow, gateway)` of the public methods list, as a current app reads it.
async fn methods(env: &TossEnv) -> Vec<(String, String, String)> {
    methods_for(env, BOTH).await
}

/// The methods list for an app sending `query` ("" is a v1.5.1 app, which opens PortOne pages only).
async fn methods_for(env: &TossEnv, query: &str) -> Vec<(String, String, String)> {
    let (st, list) = call(&env.app, "GET", &format!("/v1/payments/methods{query}"), None, None).await;
    assert_eq!(st, StatusCode::OK, "{list}");
    let s = |v: &Value| v.as_str().expect("string").to_string();
    list.as_array()
        .expect("list")
        .iter()
        .map(|m| (s(&m["code"]), s(&m["flow"]), s(&m["gateway"])))
        .collect()
}

async fn setup(env: &TossEnv, c: &Customer) -> Value {
    setup_for(env, c, BOTH).await
}

async fn setup_for(env: &TossEnv, c: &Customer, query: &str) -> Value {
    let (st, out) = call(&env.app, "GET", &format!("/v1/me/payment-methods/setup{query}"), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{out}");
    out
}

async fn overview_gateways(env: &TossEnv) -> (Value, Value) {
    let (admin, _) = staff_token(&env.state.db, Role::StaffAdmin).await;
    let (st, o) = call(&env.app, "GET", "/v1/admin/setup", Some(admin.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{o}");
    (o["payment_gateways"].clone(), o["payment_gateway"].clone())
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_neither_gateway_payment_stays_unavailable(pool: PgPool) {
    // PortOne channel keys alone (no API secret or store) do not count: they could never be confirmed.
    let half = PortOne {
        channel_keys: [("card".to_string(), "channel-key-card".to_string())].into(),
        ..PortOne::default()
    };
    let (env, _, c) = env(&pool, "010-7700-0001", half, None).await;
    assert!(methods(&env).await.is_empty());
    let (st, out) = checkout(&env, &c, "card").await;
    assert_eq!((st, out["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("payment_method_disabled")));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM checkouts").fetch_one(&pool).await.expect("count");
    assert_eq!(rows, 0, "no row is written");
    assert_eq!(setup(&env, &c).await, json!({ "enabled": false }));
    let (_, features) = call(&env.app, "GET", "/v1/features", None, None).await;
    assert_eq!(features["card_billing"], false);
    assert_eq!(overview_gateways(&env).await, (json!([]), json!("portone")));
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_alone_serves_every_enabled_method(pool: PgPool) {
    let (env, _, c) = env(&pool, "010-7700-0002", portone(&[]), Some(toss::card(true))).await;
    let list = methods(&env).await;
    assert_eq!(list.len(), 10, "every method is on until an admin changes the list");
    assert!(list.iter().all(|(_, _, g)| g == "tosspayments"));
    let flow = |l: &[(String, String, String)], code: &str| l.iter().find(|m| m.0 == code).map(|m| m.1.clone());
    assert_eq!(flow(&list, "card").as_deref(), Some("billing"), "the 자동결제 pair saves cards");
    assert_eq!(flow(&list, "kakaopay").as_deref(), Some("checkout"));

    let (st, intent) = checkout(&env, &c, "card").await;
    assert_eq!(st, StatusCode::OK, "{intent}");
    assert_eq!(
        (intent["gateway"].as_str(), intent["store_id"].clone(), intent["channel_key"].as_str()),
        (Some("tosspayments"), Value::Null, Some(WINDOW_CK))
    );
    assert_eq!(setup(&env, &c).await["gateway"], "tosspayments");
    assert_eq!(overview_gateways(&env).await.0, json!([{ "code": "tosspayments", "mode": "test" }]));

    let mut no_pair = toss::card(true);
    no_pair.remove("billing_client_key");
    no_pair.remove("billing_secret_key");
    set_toss(&env, no_pair).await;
    assert_eq!(flow(&methods(&env).await, "card").as_deref(), Some("checkout"), "no pair: card is one-off");
    assert_eq!(setup(&env, &c).await, json!({ "enabled": false }));
    let (_, features) = call(&env.app, "GET", "/v1/features", None, None).await;
    assert_eq!(features["card_billing"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_both_each_method_goes_to_one_gateway_portone_first(pool: PgPool) {
    let (env, _, c) = env(&pool, "010-7700-0003", portone(&["transfer"]), Some(toss::card(true))).await;
    let list = methods(&env).await;
    let gw = |code: &str| list.iter().find(|m| m.0 == code).map(|m| m.2.clone());
    assert_eq!(gw("transfer").as_deref(), Some("portone"));
    assert_eq!((gw("card").as_deref(), gw("kakaopay").as_deref()), (Some("tosspayments"), Some("tosspayments")));
    let mut codes: Vec<&str> = list.iter().map(|m| m.0.as_str()).collect();
    codes.dedup();
    assert_eq!(codes.len(), 10, "each code once");

    let (st, intent) = checkout(&env, &c, "transfer").await;
    assert_eq!(st, StatusCode::OK, "{intent}");
    assert_eq!(
        (intent["gateway"].as_str(), intent["channel_key"].as_str()),
        (Some("portone"), Some("channel-key-transfer"))
    );
    assert_eq!(intent["cash_receipts"], true);
    let (_, intent) = checkout(&env, &c, "kakaopay").await;
    assert_eq!(intent["gateway"], "tosspayments");

    let (admin, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (st, admin_list) = call(&env.app, "GET", "/v1/admin/payment-methods", Some(admin.as_str()), None).await;
    assert_eq!(st, StatusCode::OK);
    let row = |code: &str| admin_list.as_array().expect("list").iter().find(|m| m["code"] == code).cloned().expect("row");
    assert_eq!(
        (row("transfer")["gateway"].clone(), row("transfer")["configured"].clone()),
        (json!("portone"), json!(true))
    );
    assert_eq!(row("card")["gateway"], "tosspayments");
    assert_eq!(
        overview_gateways(&env).await,
        (
            json!([{ "code": "portone", "mode": null }, { "code": "tosspayments", "mode": "test" }]),
            json!("portone")
        )
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn toss_switched_off_stops_only_new_toss_payments_and_cards(pool: PgPool) {
    let (env, fake, c) = env(&pool, "010-7700-0004", portone(&["transfer"]), Some(toss::card(true))).await;
    let tok = Some(c.token.as_str());
    let (_, open) = checkout(&env, &c, "card").await;
    assert_eq!(open["gateway"], "tosspayments");
    let issue = setup(&env, &c).await;
    assert_eq!(issue["gateway"], "tosspayments");
    let body = json!({ "provider": "card", "auth_key": "auth_1", "display_name": "토스 카드" });
    let (st, card) = call(&env.app, "POST", "/v1/me/payment-methods", tok, Some(body)).await;
    assert_eq!(st, StatusCode::OK, "{card}");

    set_toss(&env, toss::card(false)).await;
    assert_eq!(methods(&env).await, [("transfer".into(), "checkout".into(), "portone".into())]);
    let (st, out) = checkout(&env, &c, "card").await;
    assert_eq!((st, out["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("payment_method_disabled")));
    assert_eq!(setup(&env, &c).await, json!({ "enabled": false }), "no new Toss cards");

    // The open Toss checkout still captures.
    let id = open["id"].as_str().expect("id");
    let confirm = json!({ "payment_key": "pk_off", "amount": open["amount"] });
    let (st, out) = call(&env.app, "POST", &format!("/v1/checkouts/{id}/toss-confirm"), None, Some(confirm)).await;
    assert_eq!(st, StatusCode::OK, "{out}");

    // The saved Toss card still charges on the 자동결제 MID.
    let paying = Customer {
        card: card["id"].as_str().expect("id").parse().expect("uuid"),
        token: c.token.clone(),
        ..c
    };
    common::book(&env.app, &paying, 6, None).await;
    let charges = fake.lock().expect("fake").calls_of("charge");
    assert_eq!(charges.iter().map(|c| c.secret.as_str()).collect::<Vec<_>>(), [BILLING_SK]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn apps_from_before_toss_are_offered_portone_only(pool: PgPool) {
    let (env, _, c) = env(&pool, "010-7700-0005", portone(&["transfer"]), Some(toss::card(true))).await;
    assert_eq!(methods_for(&env, "").await, [("transfer".into(), "checkout".into(), "portone".into())]);
    assert_eq!(setup_for(&env, &c, "").await, json!({ "enabled": false }), "Toss card saving is left out");
    assert_eq!(methods(&env).await.len(), 10, "a current app sees both");
    assert_eq!(setup(&env, &c).await["gateway"], "tosspayments");
}

#[sqlx::test(migrations = "../../migrations")]
async fn easy_pay_on_portone_is_paid_once_and_card_shows_where_it_is_saved(pool: PgPool) {
    let po = PortOne {
        channel_keys: [("kakaopay", "channel-key-kakaopay"), ("card_billing", "channel-key-billing")]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .into(),
        ..portone(&["transfer"])
    };
    let (env, _, c) = env(&pool, "010-7700-0006", po, Some(toss::card(true))).await;
    let list = methods(&env).await;
    let row = |code: &str| list.iter().find(|m| m.0 == code).map(|m| (m.1.clone(), m.2.clone()));
    assert_eq!(row("kakaopay"), Some(("checkout".into(), "portone".into())), "a tile that could never be saved");
    assert_eq!(
        row("card"),
        Some(("billing".into(), "portone".into())),
        "card is saved on PortOne's billing channel"
    );
    assert_eq!(setup(&env, &c).await["gateway"], "portone");
    let legacy = methods_for(&env, "").await;
    assert!(
        legacy.iter().any(|m| m.0 == "card" && m.2 == "portone"),
        "v1.5.1 can save cards on PortOne: {legacy:?}"
    );

    let (admin, _) = staff_token(&pool, Role::StaffAdmin).await;
    let (_, admin_list) = call(&env.app, "GET", "/v1/admin/payment-methods", Some(admin.as_str()), None).await;
    let gateway = |code: &str| admin_list.as_array().expect("list").iter().find(|m| m["code"] == code).expect("row")["gateway"].clone();
    assert_eq!(
        (gateway("card"), gateway("kakaopay"), gateway("naverpay")),
        (json!("portone"), json!("portone"), json!("tosspayments"))
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn no_card_is_saved_once_no_gateway_saves_cards(pool: PgPool) {
    let (env, _, c) = env(&pool, "010-7700-0007", portone(&["transfer"]), Some(toss::card(true))).await;
    assert_eq!(setup(&env, &c).await["gateway"], "tosspayments", "the ticket is issued while Toss saves cards");
    set_toss(&env, toss::card(false)).await;
    let body = json!({ "provider": "card", "auth_key": "auth_1", "display_name": "카드" });
    let (st, out) = call(&env.app, "POST", "/v1/me/payment-methods", Some(c.token.as_str()), Some(body)).await;
    assert_eq!(
        (st, out["error"]["code"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("provider_not_configured")),
        "PortOne has its secret but no billing channel: {out}"
    );
}
