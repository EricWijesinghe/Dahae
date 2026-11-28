// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `IdentityVerifier` over today's PortOne identity verification (PASS / 본인인증): the session the app opens the
//! PASS page with (`routes::me::identity_session`, which also records the id for the customer) and the result read
//! through `Providers::identity` (dev fake and health records included).

use super::identity::{IdPurpose, IdSession, IdentityInfo, IdentityVerifier, Subject};
use super::payments::ready;
use super::{AnyAdapter, Fut};
use crate::providers::{ProviderError, Providers};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub struct PortOneIdentity {
    p: Providers,
}

pub fn build(p: &Providers, _code: &'static str) -> AnyAdapter {
    AnyAdapter::Identity(Arc::new(PortOneIdentity { p: p.clone() }))
}

impl IdentityVerifier for PortOneIdentity {
    /// A fresh verification id and the SDK's store and `identity` channel key. Dev ids keep the `dev:` prefix the
    /// dev provider accepts. PASS asks for everything itself, so the subject and purpose are not sent.
    fn start(&self, _s: Subject, _purpose: IdPurpose) -> Fut<'_, IdSession> {
        let portone = self.p.integ().portone.clone();
        let channel = portone.channel_keys.get("identity").cloned();
        if !self.p.dev() && (portone.store_id.is_none() || channel.is_none()) {
            return ready(Err(ProviderError::NotConfigured("portone_identity")));
        }
        let prefix = if self.p.dev() { "dev:" } else { "idv-" };
        ready(Ok(IdSession {
            id: format!("{prefix}{}", Uuid::now_v7().simple()),
            redirect_url: None,
            sdk_params: Some(json!({
                "store_id": portone.store_id.unwrap_or_default(),
                "channel_key": channel.unwrap_or_default(),
            })),
        }))
    }

    fn result<'a>(&'a self, id: &'a str) -> Fut<'a, IdentityInfo> {
        Box::pin(self.p.identity(id))
    }
}
