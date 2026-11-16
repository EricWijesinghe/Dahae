// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Toss webhook signature. HMAC-SHA256 over `{rawBody}:{transmission-time}`, keyed with the 보안 키 (security key),
//! base64 encoded, sent as `v1:<sig>` in `tosspayments-webhook-signature`. Several signatures may be space separated.
//! shortcut: the key bytes are the security key text as given; confirm against one live test webhook before go-live.

use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub const SIGNATURE_HEADER: &str = "tosspayments-webhook-signature";
pub const TRANSMISSION_TIME_HEADER: &str = "tosspayments-webhook-transmission-time";

/// True when one `v1:` signature in `header` matches the body and transmission time. Constant-time compare.
pub fn verify(security_key: &str, transmission_time: &str, header: &str, body: &[u8]) -> bool {
    if transmission_time.is_empty() || security_key.is_empty() {
        return false;
    }
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(security_key.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.update(b":");
    mac.update(transmission_time.as_bytes());
    let expected = STANDARD.encode(mac.finalize().into_bytes());
    header
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter_map(|s| s.strip_prefix("v1:"))
        .any(|s| bool::from(s.as_bytes().ct_eq(expected.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const BODY: &[u8] = br#"{"eventType":"payout.changed","data":{"id":"p1"}}"#;

    fn sign(key: &str, time: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("key");
        mac.update(body);
        mac.update(b":");
        mac.update(time.as_bytes());
        format!("v1:{}", STANDARD.encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn valid_signature_verifies() {
        let sig = sign(KEY, "1800000000", BODY);
        assert!(verify(KEY, "1800000000", &sig, BODY));
    }

    #[test]
    fn rotated_list_matches_any_v1() {
        let sig = sign(KEY, "1800000000", BODY);
        let header = format!("v1:bogus {sig}");
        assert!(verify(KEY, "1800000000", &header, BODY));
    }

    #[test]
    fn tampered_body_time_key_or_missing_prefix_is_refused() {
        let sig = sign(KEY, "1800000000", BODY);
        assert!(!verify(KEY, "1800000000", &sig, br#"{"eventType":"payout.changed"}"#), "body changed");
        assert!(!verify(KEY, "1800000001", &sig, BODY), "time changed");
        assert!(!verify("another-key", "1800000000", &sig, BODY), "wrong key");
        assert!(!verify(KEY, "1800000000", &sig[3..], BODY), "no v1 prefix");
        assert!(!verify(KEY, "", &sig, BODY), "no transmission time");
        assert!(!verify(KEY, "1800000000", "", BODY), "no signature");
        assert!(!verify("", "1800000000", &sign("", "1800000000", BODY), BODY), "no key");
    }
}
