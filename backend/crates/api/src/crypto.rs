// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Field-level encryption for sensitive values (bank account numbers).

use crate::entropy;
use aes_gcm::{
    Aes256Gcm, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};

const NONCE_LEN: usize = 12;
/// Prefix on every new ciphertext: format version 1 (AAD-bound). Values without it are the legacy
/// format (no AAD), still read so existing rows keep working until they are rewritten.
const V1_PREFIX: &str = "v1:";

/// Column labels bound as AAD. A ciphertext moved to another column fails to decrypt.
pub mod aad {
    pub const PAYOUT_ACCOUNT: &str = "payout_accounts.account_enc";
    pub const RECEIPT_IDENTITY: &str = "cash_receipts.identity_enc";
    pub const ACCESS_CODE: &str = "bookings.access_code_enc";
    pub const STAFF_TOTP: &str = "staff.totp_secret";
    pub const SOCIAL_TOKEN: &str = "social_tokens.refresh_token_enc";
    pub const INTEGRATION: &str = "integration_settings.secret";
    pub const BILLING_KEY: &str = "payment_methods.billing_key";
    pub const PRO_EMERGENCY_CONTACT: &str = "pro_profiles.emergency_contact_enc";
    pub const PRO_PAYEE_IDENTIFIER: &str = "pro_profiles.payee_identifier_enc";
}

pub fn encrypt(key: &[u8; 32], aad: &str, plaintext: &str) -> String {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce = [0u8; NONCE_LEN];
    entropy::fill(&mut nonce);
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .expect("AES-GCM encryption cannot fail for valid key");
    let mut out = nonce.to_vec();
    out.extend(ct);
    format!("{V1_PREFIX}{}", STANDARD.encode(out))
}

pub fn decrypt(key: &[u8; 32], aad: &str, encoded: &str) -> Option<String> {
    let (body, aad): (&str, &[u8]) = match encoded.strip_prefix(V1_PREFIX) {
        Some(body) => (body, aad.as_bytes()),
        None => (encoded, b""),
    };
    let raw = STANDARD.decode(body).ok()?;
    if raw.len() <= NONCE_LEN {
        return None;
    }
    let (nonce, ct) = raw.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let plain = cipher.decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad }).ok()?;
    String::from_utf8(plain).ok()
}

/// Derivation label for the referral-phone pepper. Changing it (or DATA_KEY) makes old digests unmatchable.
const PHONE_PEPPER_LABEL: &[u8] = b"malkkeum/referral-phone/v1";
/// Derivation label for the saved-card digest. Changing it (or DATA_KEY) breaks the one-card-per-key index.
const BILLING_PEPPER_LABEL: &[u8] = b"malkkeum/billing-key/v1";

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(msg);
    mac.finalize().into_bytes().into()
}

/// Derivation label for the payout key digest. Changing it (or DATA_KEY) makes stored digests stop matching.
const SECRET_DIGEST_LABEL: &[u8] = b"malkkeum/secret-digest/v1";

/// Keyed digest of a saved payment key (32 hex chars), kept with its test result to tell a changed key apart.
/// Unlike a plain hash it cannot be checked against a guessed key without DATA_KEY.
pub fn secret_digest(data_key: &[u8; 32], secret: &str) -> String {
    let pepper = hmac_sha256(data_key, SECRET_DIGEST_LABEL);
    hex::encode(&hmac_sha256(&pepper, secret.as_bytes())[..16])
}

/// Keyed, non-reversible digest of `msg` under a pepper derived from DATA_KEY and `label`, as hex.
fn peppered_digest(data_key: &[u8; 32], label: &[u8], msg: &str) -> String {
    let pepper = hmac_sha256(data_key, label);
    hex::encode(hmac_sha256(&pepper, msg.as_bytes()))
}

/// Keyed, non-reversible stand-in for a phone kept after account deletion (`hmac:<hex>`). The pepper is
/// derived from DATA_KEY and never stored, so the 10^8 Korean mobile numbers cannot be enumerated from the DB.
pub fn phone_digest(data_key: &[u8; 32], phone: &str) -> String {
    format!("hmac:{}", peppered_digest(data_key, PHONE_PEPPER_LABEL, phone))
}

/// Stored form of a PortOne billing key: AAD-bound ciphertext, plus the keyed digest the
/// one-live-card-per-key unique index runs on (ciphertext of one key differs on every write).
pub fn seal_billing_key(data_key: &[u8; 32], billing_key: &str) -> (String, String) {
    (encrypt(data_key, aad::BILLING_KEY, billing_key), billing_key_digest(data_key, billing_key))
}

/// Keyed digest of a billing key (`hmac:<hex>`). Never reversible, so it leaks nothing usable on its own.
pub fn billing_key_digest(data_key: &[u8; 32], billing_key: &str) -> String {
    format!("hmac:{}", peppered_digest(data_key, BILLING_PEPPER_LABEL, billing_key))
}

/// Plaintext billing key from its stored form. Only the v1 (AAD-bound) format is accepted: a value
/// without the prefix is either plaintext or the legacy format, and neither may be charged.
pub fn open_billing_key(data_key: &[u8; 32], stored: &str) -> Option<String> {
    if !stored.starts_with(V1_PREFIX) {
        return None;
    }
    decrypt(data_key, aad::BILLING_KEY, stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tamper() {
        let key = [7u8; 32];
        let enc = encrypt(&key, aad::PAYOUT_ACCOUNT, "110-123-456789");
        assert!(enc.starts_with(V1_PREFIX));
        assert_eq!(decrypt(&key, aad::PAYOUT_ACCOUNT, &enc).as_deref(), Some("110-123-456789"));
        assert_eq!(decrypt(&[8u8; 32], aad::PAYOUT_ACCOUNT, &enc), None);
    }

    #[test]
    fn ciphertext_is_bound_to_its_column() {
        let key = [7u8; 32];
        let enc = encrypt(&key, aad::PAYOUT_ACCOUNT, "secret");
        assert_eq!(decrypt(&key, aad::ACCESS_CODE, &enc), None, "moved to another column must not decrypt");
        let mut flipped = enc.clone();
        flipped.push('A');
        assert_eq!(decrypt(&key, aad::PAYOUT_ACCOUNT, &flipped), None, "trailing garbage is rejected");
    }

    #[test]
    fn payee_identifier_is_bound_to_its_own_column() {
        let key = [7u8; 32];
        let enc = encrypt(&key, aad::PRO_PAYEE_IDENTIFIER, "9001011234568");
        assert_eq!(decrypt(&key, aad::PRO_PAYEE_IDENTIFIER, &enc).as_deref(), Some("9001011234568"));
        assert_eq!(decrypt(&key, aad::PRO_EMERGENCY_CONTACT, &enc), None, "must not open as the emergency contact");
    }

    #[test]
    fn legacy_values_without_prefix_still_decrypt() {
        use aes_gcm::aead::Aead;
        let key = [7u8; 32];
        let nonce = [1u8; NONCE_LEN];
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let ct = cipher.encrypt(Nonce::from_slice(&nonce), b"old-value".as_ref()).expect("encrypt");
        let legacy = STANDARD.encode([nonce.to_vec(), ct].concat());
        assert_eq!(decrypt(&key, aad::ACCESS_CODE, &legacy).as_deref(), Some("old-value"));
        assert_eq!(decrypt(&key, aad::ACCESS_CODE, "v1:!!"), None, "malformed v1 value is rejected");
    }

    #[test]
    fn phone_digest_is_keyed_and_stable() {
        use sha2::{Digest, Sha256};
        let d = phone_digest(&[7u8; 32], "01012345678");
        assert_eq!(d, phone_digest(&[7u8; 32], "01012345678"));
        assert_ne!(d, phone_digest(&[8u8; 32], "01012345678"), "depends on the server key");
        assert!(d.starts_with("hmac:") && d.len() == 5 + 64);
        assert!(!d.contains(&hex::encode(Sha256::digest(b"01012345678"))), "not a plain hash");
    }

    #[test]
    fn billing_key_is_sealed_bound_and_keyed() {
        let key = [7u8; 32];
        let (sealed, digest) = seal_billing_key(&key, "bk_live_1");
        assert!(!sealed.contains("bk_live_1"), "stored form is not the plaintext key");
        assert_eq!(open_billing_key(&key, &sealed).as_deref(), Some("bk_live_1"));
        assert_eq!(digest, billing_key_digest(&key, "bk_live_1"), "digest is deterministic");
        assert_ne!(digest, billing_key_digest(&[8u8; 32], "bk_live_1"), "digest depends on the server key");
        assert_ne!(digest, phone_digest(&key, "bk_live_1"), "digest is separated from the phone pepper");
        assert_ne!(sealed, seal_billing_key(&key, "bk_live_1").0, "random nonce per write");
    }

    #[test]
    fn billing_key_rejects_plaintext_legacy_and_other_column() {
        let key = [7u8; 32];
        assert_eq!(open_billing_key(&key, "bk_live_1"), None, "plaintext is never charged");
        let legacy = encrypt(&key, aad::BILLING_KEY, "bk").replacen(V1_PREFIX, "", 1);
        assert_eq!(open_billing_key(&key, &legacy), None, "prefix-less value is refused");
        let payout = encrypt(&key, aad::PAYOUT_ACCOUNT, "bk_live_1");
        assert_eq!(open_billing_key(&key, &payout), None, "ciphertext moved from another column fails");
    }
}

#[cfg(test)]
mod known_answers {
    use super::*;

    // Pinned from an independent HMAC-SHA256 reference (Python hmac/hashlib) with key [7; 32]; a hash or HMAC
    // crate upgrade must not change any of these, since stored digests and signatures depend on them.
    const KEY: [u8; 32] = [7; 32];

    #[test]
    fn hmac_sha256_rfc4231_case2() {
        assert_eq!(
            hex::encode(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn keyed_digests_match_pinned_outputs() {
        assert_eq!(secret_digest(&KEY, "pay_key_test_123"), "d2a1bf30338821262d45d52e364c7f76");
        assert_eq!(
            phone_digest(&KEY, "01012345678"),
            "hmac:e07a48f2eb29265caab22ef7d97889c69da6103a2b4d73c557b4595a2748688a"
        );
        assert_eq!(
            billing_key_digest(&KEY, "billing_key_abc"),
            "hmac:dde70c934a43f2f4045a6bca77b45e4d3c2719953a4f017c1fe494bf591cccdc"
        );
    }
}
