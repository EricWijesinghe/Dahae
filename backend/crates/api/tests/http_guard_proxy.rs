// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `HttpGuard` ignores proxy environment variables: a proxy would resolve the target itself, past the public-address
//! check. Its own test binary, because it sets process environment.

use api::adapters::http_guard::{Hosts, HttpGuard};
use reqwest::Method;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[tokio::test]
async fn a_proxy_in_the_environment_does_not_carry_a_loopback_target() {
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = proxy.local_addr().expect("addr");
    let contacted = Arc::new(AtomicBool::new(false));
    let seen = contacted.clone();
    // Accepts and drops each connection, so a guard that used the proxy fails fast and the flag tells.
    tokio::spawn(async move {
        while proxy.accept().await.is_ok() {
            seen.store(true, Ordering::SeqCst);
        }
    });
    let url = format!("http://{addr}");
    // SAFETY: this binary has one test, and the variables are set before any client reads them.
    unsafe {
        std::env::set_var("HTTPS_PROXY", &url);
        std::env::set_var("ALL_PROXY", &url);
    }
    let g = HttpGuard::new(Hosts::Owner("localhost".into())).expect("guard");
    let err = g
        .send(g.request(Method::GET, "https://localhost/").expect("request"), false)
        .await
        .expect_err("localhost resolves to loopback only");
    assert!(!contacted.load(Ordering::SeqCst), "the proxy was used: {err}");
}
