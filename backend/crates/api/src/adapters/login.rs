// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The social-login port. KR shim (Kakao, Naver, Google, Apple): `social.rs`.

use super::Fut;
pub use crate::providers::social::SocialUser;

pub trait SocialVerifier: Send + Sync {
    fn verify<'a>(&'a self, token: &'a str, nonce: Option<&'a str>) -> Fut<'a, SocialUser>;
    /// Unlinks the sign-in at the provider (account deletion).
    fn revoke<'a>(&'a self, u: &'a SocialUser) -> Fut<'a, ()>;
}
