// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The identity-verification port (KR: PASS / 본인인증). Today's PortOne identity code still runs through
//! `Providers::identity`.

use super::Fut;
pub use crate::providers::IdentityInfo;
use serde_json::Value;

/// Who is verified, as far as we know before the provider asks.
pub struct Subject {
    pub phone_e164: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdPurpose {
    Signup,
    Payout,
    AgeCheck,
}

/// A verification the customer completes at the provider.
pub struct IdSession {
    pub id: String,
    pub redirect_url: Option<String>,
    pub sdk_params: Option<Value>,
}

pub trait IdentityVerifier: Send + Sync {
    fn start(&self, s: Subject, purpose: IdPurpose) -> Fut<'_, IdSession>;
    /// The result for a session id. `ci` is the subject reference.
    fn result<'a>(&'a self, id: &'a str) -> Fut<'a, IdentityInfo>;
}
