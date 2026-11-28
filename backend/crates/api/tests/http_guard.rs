// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `HttpGuard` against a local server: idempotent requests are retried on a busy upstream, others are sent once,
//! redirects are not followed, and errors never carry the URL. The address rules are unit tests in the module.

use api::adapters::http_guard::{Hosts, HttpGuard};
use axum::{Router, http::StatusCode, response::Redirect, routing::get};
use reqwest::Method;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    base
}

#[tokio::test]
async fn idempotent_requests_retry_a_busy_upstream_and_others_do_not() {
    let hits = Arc::new(AtomicU32::new(0));
    let h = hits.clone();
    let app = Router::new().route(
        "/busy",
        get(move || {
            let n = h.fetch_add(1, Ordering::SeqCst);
            async move {
                if n.is_multiple_of(2) {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::OK
                }
            }
        })
        .post(|| async { StatusCode::SERVICE_UNAVAILABLE }),
    );
    let base = serve(app).await;
    let g = HttpGuard::allow_local_for_tests(Hosts::Fixed(&[]), &base).expect("guard");
    let once = g
        .send(g.request(Method::POST, &format!("{base}/busy")).expect("request"), false)
        .await
        .expect("sent");
    assert_eq!(once.status(), StatusCode::SERVICE_UNAVAILABLE, "a call that may have taken effect is sent once");
    let retried = g
        .send(g.request(Method::GET, &format!("{base}/busy")).expect("request"), true)
        .await
        .expect("sent");
    assert_eq!(retried.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 2, "one retry after the 503");
    assert!(g.request(Method::GET, "http://127.0.0.2:1/").is_err(), "only the one local origin is allowed");
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let app = Router::new().route("/away", get(|| async { Redirect::temporary("http://169.254.169.254/latest/meta-data") }));
    let base = serve(app).await;
    let g = HttpGuard::allow_local_for_tests(Hosts::Fixed(&[]), &base).expect("guard");
    let r = g
        .send(g.request(Method::GET, &format!("{base}/away")).expect("request"), true)
        .await
        .expect("sent");
    assert_eq!(r.status(), StatusCode::TEMPORARY_REDIRECT, "the redirect is returned, never followed");
}

#[tokio::test]
async fn transport_errors_do_not_carry_the_url() {
    // A port nothing listens on: every attempt fails to connect.
    let base = "http://127.0.0.1:1";
    let g = HttpGuard::allow_local_for_tests(Hosts::Fixed(&[]), base).expect("guard");
    let err = g
        .send(
            g.request(Method::GET, &format!("{base}/v1/billing/secret-key?token=abc")).expect("request"),
            true,
        )
        .await
        .expect_err("nothing listens");
    let text = err.to_string();
    assert!(!text.contains("secret-key") && !text.contains("token=abc"), "{text}");
}
