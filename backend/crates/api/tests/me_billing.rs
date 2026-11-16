// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Saved-card billing keys at rest: a registered card is stored sealed with a keyed digest, the charge path
//! opens the key before the gateway sees it, legacy plaintext rows are sealed at startup (rerun-safe), and one
//! live card per billing key is enforced on the digest. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{
    billing_keys, build_state,
    crypto::{billing_key_digest, open_billing_key, seal_billing_key},
    providers::{FAKE_DECLINE_KEY, ProviderError},
};
use axum::http::StatusCode;
use common::{app, call, customer, prod_app, seed_catalog, test_config};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// Stored billing key and digest of one saved card.
async fn stored_row(pool: &PgPool, card: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT billing_key, billing_key_digest FROM payment_methods WHERE id = $1")
        .bind(card)
        .fetch_one(pool)
        .await
        .expect("card row")
}

#[sqlx::test(migrations = "../../migrations")]
async fn registered_card_is_stored_sealed_with_digest(pool: PgPool) {
    seed_catalog(&pool).await;
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7000-0101").await;
    sqlx::query("INSERT INTO billing_issues (id, user_id, expires_at) VALUES ($1, $2, now() + interval '30 minutes')")
        .bind(Uuid::now_v7())
        .bind(c.user)
        .execute(&pool)
        .await
        .expect("ticket");
    let (st, body) = call(
        &dev,
        "POST",
        "/v1/me/payment-methods",
        Some(c.token.as_str()),
        Some(json!({ "provider": "card", "auth_key": "auth-1", "display_name": "new card" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let id: Uuid = body["id"].as_str().expect("id").parse().expect("uuid");

    let (stored, digest) = stored_row(&pool, id).await;
    let key = test_config().data_key;
    let plain = open_billing_key(&key, &stored).expect("opens with DATA_KEY");
    assert!(plain.starts_with("bk_"), "dev billing key is opened back to the plaintext it was issued as");
    assert!(!stored.contains(&plain), "stored value is not the plaintext key");
    assert!(stored.starts_with("v1:"));
    assert_eq!(digest, Some(billing_key_digest(&key, &plain)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn charge_opens_the_stored_key_before_the_gateway_check(pool: PgPool) {
    let state = build_state(pool.clone(), test_config()).expect("state");
    let key = test_config().data_key;
    let user = Uuid::now_v7();
    let (declined, _) = seal_billing_key(&key, FAKE_DECLINE_KEY);
    assert!(matches!(
        state.providers.charge_billing(&declined, 1000, "pid-decline", "order", user).await,
        Err(ProviderError::Declined(ref m)) if m == "card_declined"
    ));
    let (ok, _) = seal_billing_key(&key, "bk_live_ok");
    assert!(state.providers.charge_billing(&ok, 1000, "pid-ok", "order", user).await.is_ok());
    assert!(matches!(
        state.providers.charge_billing("bk_plaintext", 1000, "pid-plain", "order", user).await,
        Err(ProviderError::Declined(ref m)) if m == "billing_key_unreadable"
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_billing_key_cannot_back_two_live_cards(pool: PgPool) {
    seed_catalog(&pool).await;
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7000-0102").await;
    let (sealed, digest) = seal_billing_key(&test_config().data_key, &format!("bk_test_{}", c.user));
    let dup = sqlx::query("INSERT INTO payment_methods (user_id, provider, billing_key, billing_key_digest, display_name) VALUES ($1, 'pg', $2, $3, 'dup')")
        .bind(c.user)
        .bind(sealed)
        .bind(digest)
        .execute(&pool)
        .await;
    assert!(dup.is_err(), "the digest index keeps one live card per billing key");
}

#[sqlx::test(migrations = "../../migrations")]
async fn startup_seals_legacy_plaintext_keys_once(pool: PgPool) {
    seed_catalog(&pool).await;
    let dev = app(pool.clone());
    let c = customer(&dev, &pool, "010-7000-0103").await;
    let legacy = Uuid::now_v7();
    sqlx::query("INSERT INTO payment_methods (id, user_id, provider, billing_key, display_name) VALUES ($1, $2, 'pg', 'bk_legacy_1', 'old card')")
        .bind(legacy)
        .bind(c.user)
        .execute(&pool)
        .await
        .expect("legacy row");
    let key = test_config().data_key;

    assert_eq!(
        billing_keys::seal_stored(&pool, &key).await.expect("seal"),
        1,
        "only the plaintext row is sealed"
    );
    let (stored, digest) = stored_row(&pool, legacy).await;
    assert_eq!(open_billing_key(&key, &stored).as_deref(), Some("bk_legacy_1"));
    assert_eq!(digest, Some(billing_key_digest(&key, "bk_legacy_1")));
    assert_eq!(billing_keys::seal_stored(&pool, &key).await.expect("rerun"), 0, "a rerun changes nothing");
}

/// PortOne stand-in for the billing-key delete: records each request path and answers `status`.
async fn gateway_stand_in(status: axum::http::StatusCode) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = seen.clone();
    let app = axum::Router::new().fallback(move |req: axum::extract::Request| {
        let log = log.clone();
        async move {
            log.lock().expect("log").push(format!("{} {}", req.method(), req.uri().path()));
            status
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    (format!("http://{addr}"), seen)
}

/// A customer with one saved card whose sealed billing key is `bk_live_card`.
async fn card_with_live_key(pool: &PgPool) -> common::Customer {
    let dev = app(pool.clone());
    let c = customer(&dev, pool, "010-7100-0400").await;
    let (sealed, digest) = seal_billing_key(&test_config().data_key, "bk_live_card");
    sqlx::query("UPDATE payment_methods SET billing_key = $2, billing_key_digest = $3 WHERE id = $1")
        .bind(c.card)
        .bind(sealed)
        .bind(digest)
        .execute(pool)
        .await
        .expect("live key");
    c
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_saved_card_revokes_its_billing_key_at_the_gateway(pool: PgPool) {
    let c = card_with_live_key(&pool).await;
    let (base, seen) = gateway_stand_in(StatusCode::OK).await;
    let prod = prod_app(pool.clone(), &base);
    let (st, body) = call(&prod, "DELETE", &format!("/v1/me/payment-methods/{}", c.card), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(
        seen.lock().expect("log").as_slice(),
        ["DELETE /billing-keys/bk_live_card"],
        "PortOne told to drop the key"
    );
    let (stored, _) = stored_row(&pool, c.card).await;
    assert_eq!(stored, "", "the local copy is erased after the revoke");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_saved_card_fails_closed_when_the_gateway_refuses(pool: PgPool) {
    let c = card_with_live_key(&pool).await;
    let (base, _seen) = gateway_stand_in(StatusCode::INTERNAL_SERVER_ERROR).await;
    let prod = prod_app(pool.clone(), &base);
    let (st, body) = call(&prod, "DELETE", &format!("/v1/me/payment-methods/{}", c.card), Some(c.token.as_str()), None).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let (stored, _) = stored_row(&pool, c.card).await;
    assert!(!stored.is_empty(), "the key stays on file so the removal can be retried");
}
