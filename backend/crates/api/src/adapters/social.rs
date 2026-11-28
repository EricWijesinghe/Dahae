// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `SocialVerifier` over today's sign-in code for its four codes: `kakao`, `naver`, `google` and `apple`. Every
//! call goes through `providers::social` (dev tokens, key caches, nonce rules), so behaviour is unchanged.

use super::login::{SocialUser, SocialVerifier};
use super::{AnyAdapter, Fut};
use crate::error::ApiError;
use crate::providers::{ProviderError, Providers, social};
use std::sync::Arc;

pub struct Social {
    p: Providers,
    code: &'static str,
}

pub fn build(p: &Providers, code: &'static str) -> AnyAdapter {
    AnyAdapter::Login(Arc::new(Social { p: p.clone(), code }))
}

/// The route's answer as a provider answer: a refused token is a decline, a missing key `NotConfigured`.
fn provider_error(e: ApiError) -> ProviderError {
    match e {
        ApiError::BadRequest(code) => ProviderError::Declined(code.into()),
        ApiError::Unauthorized => ProviderError::Declined("unauthorized".into()),
        ApiError::Unavailable("provider_not_configured") => ProviderError::NotConfigured("social"),
        other => ProviderError::Upstream(other.to_string()),
    }
}

impl SocialVerifier for Social {
    fn verify<'a>(&'a self, token: &'a str, nonce: Option<&'a str>) -> Fut<'a, SocialUser> {
        Box::pin(async move { social::verify(&self.p, self.code, token, nonce).await.map_err(provider_error) })
    }

    /// Apple revokes the stored token for the app it was issued to (`audience`), Kakao unlinks the user id, Naver
    /// deletes the stored refresh token. Google keeps no token, so it has nothing to revoke yet.
    fn revoke<'a>(&'a self, u: &'a SocialUser) -> Fut<'a, ()> {
        Box::pin(async move {
            social::unlink_one(&self.p, self.code, &u.provider_user_id, u.audience.as_deref(), u.refresh_token.as_deref())
                .await
                .map_err(|reason| match reason {
                    "not_configured" => ProviderError::NotConfigured("social"),
                    other => ProviderError::Upstream(other.into()),
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_errors_map_to_provider_errors() {
        assert!(matches!(provider_error(ApiError::BadRequest("invalid_token")), ProviderError::Declined(m) if m == "invalid_token"));
        assert!(matches!(provider_error(ApiError::Unauthorized), ProviderError::Declined(_)));
        assert!(matches!(
            provider_error(ApiError::Unavailable("provider_not_configured")),
            ProviderError::NotConfigured(_)
        ));
        assert!(matches!(
            provider_error(ApiError::Unavailable("provider_unreachable")),
            ProviderError::Upstream(_)
        ));
    }
}
