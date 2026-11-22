// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Integration keys the owner enters in the console setup wizard.
//!
//! Each wizard card ("section") is stored as one AES-GCM encrypted JSON object in
//! `integration_settings`. A section saved in the console replaces the matching env vars, so a
//! non-technical install never edits config files; env vars stay as a developer fallback.
//! Saving swaps the live keys in [`crate::providers::Providers`] without a restart.

use crate::{
    ai::{self, AiConfig, Caps, Mode},
    config::{AppleSignInKey, Config, PortOne, SolapiConfig},
    crypto,
    providers::{Gateway, fcm::ServiceAccount},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use std::{collections::HashMap, ops::RangeInclusive};

/// Live keys for every external service. Built from env, then overlaid with console sections.
#[derive(Clone, Debug, Default)]
pub struct Integrations {
    pub kakao_app_id: Option<String>,
    /// Kakao Admin key: unlinks the Kakao account on account deletion (providers/social.rs). Optional.
    pub kakao_admin_key: Option<String>,
    pub naver_client_id: Option<String>,
    /// Naver app secret: the server exchanges each Naver login code with it (providers/social.rs).
    pub naver_client_secret: Option<String>,
    pub google_client_ids: Vec<String>,
    pub apple_client_ids: Vec<String>,
    /// Sign in with Apple key: code exchange at sign-in and token revocation on deletion. Optional.
    pub apple_signin: Option<AppleSignInKey>,
    pub portone: PortOne,
    /// Toss Payments (direct). Console-only; while `active` it takes new payments next to PortOne (see `gateway_for`).
    pub toss_payments: Option<TossPayments>,
    pub solapi: Option<SolapiConfig>,
    pub fcm: Option<ServiceAccount>,
    /// AI support agent. Console-only (no env fallback).
    pub ai: Option<AiConfig>,
    pub backup: Option<S3Config>,
    /// 도로명주소 검색 승인키 (juso.go.kr `confmKey`). Console-only.
    pub juso_confm_key: Option<String>,
    /// Partner payout provider. None means `manual` (owner transfers and confirms).
    pub payout: Option<PayoutSettings>,
    /// True when Toss payouts may be enabled: env `TOSS_PAYOUTS_ENABLED=1` and the saved sandbox key passed
    /// the console test (see `toss_ready`). Set by the loader, never by the console.
    pub payout_toss_ready: bool,
}

/// The Toss Payments card: API 개별 연동 keys of the window MID, optionally of the 자동결제 MID, and the switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TossPayments {
    /// Public (`test_ck_` / `live_ck_`): the hosted page starts the SDK with it.
    pub client_key: String,
    pub secret_key: String,
    /// (client, secret) of the 자동결제 (billing) MID. Without it, saved cards cannot be added under Toss.
    pub billing: Option<(String, String)>,
    /// 보안 키 (64 hex): webhook signatures.
    pub security_key: Option<String>,
    /// 사용 (unset: on with live keys, off with test keys): Toss takes new payments next to PortOne. Off stops only new Toss checkouts and new
    /// saved cards; webhooks, confirms, refunds and saved-card charges keep running.
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayoutMode {
    Manual,
    Toss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayoutSettings {
    pub mode: PayoutMode,
    /// Toss payouts secret key (Basic auth). Required when mode is `toss`.
    pub secret_key: Option<String>,
    /// Toss security key (보안 키, 64 hex chars): the AES key for JWE request and response bodies.
    pub security_key: Option<String>,
}

/// Env switch that must be on before Toss payouts can be enabled from the console.
pub const TOSS_ENV: &str = "TOSS_PAYOUTS_ENABLED";
/// PortOne ids as the console issues them: store ids and channel keys carry these prefixes.
const PORTONE_STORE_PREFIX: &str = "store-";
const PORTONE_CHANNEL_PREFIX: &str = "channel-key-";
/// Toss secret key prefixes. Only these two kinds can pass the connection test that unlocks Toss payouts.
const TEST_SECRET_PREFIX: &str = "test_sk_";
const LIVE_SECRET_PREFIX: &str = "live_sk_";

/// Keyed digest of a secret, stored with the test result so a changed key must be re-tested.
/// Keyed with DATA_KEY: an unkeyed hash of a payment key could be checked against guesses offline.
pub fn key_fingerprint(data_key: &[u8; 32], secret: &str) -> String {
    crypto::secret_digest(data_key, secret)
}

/// The connection-test code a secret earns: `sandbox_connected` for a test key, `live_connected` for a live key.
pub fn connected_code(secret: &str) -> Option<&'static str> {
    if secret.starts_with(TEST_SECRET_PREFIX) {
        Some("sandbox_connected")
    } else if secret.starts_with(LIVE_SECRET_PREFIX) {
        Some("live_connected")
    } else {
        None
    }
}

/// `test` or `live` from the Toss secret key prefix, for the wizard badge. Never returns the key itself.
pub fn toss_key_mode(secret: &str) -> Option<&'static str> {
    if secret.starts_with(TEST_SECRET_PREFIX) {
        Some("test")
    } else if secret.starts_with(LIVE_SECRET_PREFIX) {
        Some("live")
    } else {
        None
    }
}

/// Toss payouts open only when the env switch is on AND the last connection test recorded success for this key,
/// in the same mode the key is in (a test pass never unlocks a live key).
pub fn toss_ready(env_enabled: bool, last_test: Option<&Value>, data_key: &[u8; 32], secret: &str) -> bool {
    let Some(code) = connected_code(secret) else {
        return false;
    };
    let fp = key_fingerprint(data_key, secret);
    env_enabled && last_test.is_some_and(|t| t["ok"] == json!(true) && t["code"] == json!(code) && t["key_fp"] == json!(fp))
}

pub fn toss_env_enabled() -> bool {
    std::env::var(TOSS_ENV).is_ok_and(|v| v == "1")
}

/// Settings row of the owner's console switch for Toss payouts: `{"on":bool,"by":uuid,"at":rfc3339}`.
pub const TOSS_SWITCH_KEY: &str = "payout_toss_switch";

/// The env switch or the owner's console switch: either opens the Toss payouts gate.
pub async fn toss_switch_on(db: &PgPool) -> Result<bool, sqlx::Error> {
    if toss_env_enabled() {
        return Ok(true);
    }
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(TOSS_SWITCH_KEY)
        .fetch_optional(db)
        .await?;
    Ok(v.is_some_and(|v| v["on"] == json!(true)))
}

/// S3-compatible offsite backup target (AWS S3, Cloudflare R2, Naver Cloud Object Storage, Backblaze B2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub retention_days: u32,
    /// Set for the AWS template's bucket: credentials come from the instance metadata service at this base URL
    /// (IMDSv2) and the role cannot delete, so old copies go by the bucket's lifecycle. The key fields stay empty.
    pub instance_role: Option<String>,
}

impl S3Config {
    /// The AWS template's bucket, written to by the server's own role. None for a malformed bucket or region.
    pub fn instance_role(bucket: &str, region: &str) -> Option<Self> {
        let bucket_ok = (3..=63).contains(&bucket.len()) && bucket.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
        let region_ok = !region.is_empty() && region.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        (bucket_ok && region_ok).then(|| Self {
            endpoint: format!("https://s3.{region}.amazonaws.com"),
            region: region.to_string(),
            bucket: bucket.to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            retention_days: BACKUP_RETENTION_DEFAULT,
            instance_role: Some(crate::backup::IMDS_BASE.to_string()),
        })
    }
}

impl Integrations {
    pub fn from_config(c: &Config) -> Self {
        Self {
            kakao_app_id: c.kakao_app_id.clone(),
            kakao_admin_key: c.kakao_admin_key.clone(),
            naver_client_id: None,
            naver_client_secret: None,
            google_client_ids: c.google_client_ids.clone(),
            apple_client_ids: c.apple_client_ids.clone(),
            apple_signin: c.apple_signin.clone(),
            portone: c.portone.clone(),
            toss_payments: None,
            solapi: c.solapi.clone(),
            fcm: c.fcm.clone(),
            ai: None,
            backup: c.instance_backup.clone(),
            payout: None,
            payout_toss_ready: false,
            juso_confm_key: None,
        }
    }

    /// PortOne takes payments: API secret and store id (env or console).
    fn portone_ready(&self) -> bool {
        self.has(Section::Portone)
    }

    /// The Toss card while it takes new payments: saved, valid, 사용 on (by default only with live keys).
    pub fn toss_offered(&self) -> Option<&TossPayments> {
        self.toss_payments.as_ref().filter(|t| t.active)
    }

    /// Where a new one-off payment with `method` goes: PortOne when it has that method's channel key, else Toss
    /// when offered. None: no gateway serves the method, so it is not shown.
    pub fn gateway_for(&self, method: &str) -> Option<Gateway> {
        if self.portone_ready() && crate::routes::payments::channel_key(&self.portone.channel_keys, method).is_some() {
            return Some(Gateway::Portone);
        }
        self.toss_offered().map(|_| Gateway::TossPayments)
    }

    /// Where a new saved card goes: PortOne when it can save cards, else Toss when offered with its 자동결제 pair.
    pub fn billing_gateway(&self) -> Option<Gateway> {
        if crate::providers::portone::billing_ready(&self.portone) {
            return Some(Gateway::Portone);
        }
        self.toss_offered().filter(|t| t.billing.is_some()).map(|_| Gateway::TossPayments)
    }

    /// Gateways a new payment can reach with the `enabled` methods, PortOne first.
    pub fn payment_gateways(&self, enabled: &[String]) -> Vec<Gateway> {
        let billing = self.billing_gateway();
        [Gateway::Portone, Gateway::TossPayments]
            .into_iter()
            .filter(|g| billing == Some(*g) || enabled.iter().any(|m| self.gateway_for(m) == Some(*g)))
            .collect()
    }

    /// Whether the env (not the console) already provides this section.
    pub fn has(&self, s: Section) -> bool {
        match s {
            Section::Portone => self.portone.api_secret.is_some() && self.portone.store_id.is_some(),
            Section::TossPayments => self.toss_payments.is_some(),
            Section::Solapi => self.solapi.is_some(),
            Section::Push => self.fcm.is_some(),
            Section::Kakao => self.kakao_app_id.is_some(),
            Section::Naver => self.naver_client_id.is_some() && self.naver_client_secret.is_some(),
            Section::Google => !self.google_client_ids.is_empty(),
            Section::Apple => !self.apple_client_ids.is_empty(),
            Section::Ai => self.ai.is_some(),
            Section::Backup => self.backup.is_some(),
            Section::Payout => self.payout.is_some(),
            Section::Juso => self.juso_confm_key.is_some(),
        }
    }
}

/// How a field is entered and shown in the wizard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One line of text.
    Text,
    /// Shown masked (last 4 characters) once saved.
    Secret,
    /// Comma separated list.
    List,
    /// JSON object (method -> channel key, event -> template id).
    Map,
    /// Pasted file contents (Firebase key JSON). Secret.
    File,
    /// On or off (JSON boolean). Unsaved, it shows the env default.
    Toggle,
}

pub struct Field {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
}

const fn f(name: &'static str, kind: Kind, required: bool) -> Field {
    Field { name, kind, required }
}

/// One card in the setup wizard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Portone,
    TossPayments,
    Solapi,
    Push,
    Kakao,
    Naver,
    Google,
    Apple,
    Ai,
    Backup,
    Payout,
    Juso,
}

impl Section {
    pub const ALL: [Section; 12] = [
        Self::Portone,
        Self::TossPayments,
        Self::Solapi,
        Self::Push,
        Self::Kakao,
        Self::Naver,
        Self::Google,
        Self::Apple,
        Self::Ai,
        Self::Backup,
        Self::Payout,
        Self::Juso,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Portone => "portone",
            Self::TossPayments => crate::providers::TOSS,
            Self::Solapi => "solapi",
            Self::Push => "push",
            Self::Kakao => "kakao",
            Self::Naver => "naver",
            Self::Google => "google",
            Self::Apple => "apple",
            Self::Ai => "ai",
            Self::Backup => "backup",
            Self::Payout => "payout",
            Self::Juso => "juso",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.code() == code)
    }

    pub fn fields(self) -> &'static [Field] {
        match self {
            Self::Portone => &PORTONE_FIELDS,
            Self::TossPayments => &TOSS_PAYMENTS_FIELDS,
            Self::Solapi => &SOLAPI_FIELDS,
            Self::Push => &PUSH_FIELDS,
            Self::Kakao => &KAKAO_FIELDS,
            Self::Naver => &NAVER_FIELDS,
            Self::Google => &CLIENT_ID_FIELDS,
            Self::Apple => &APPLE_FIELDS,
            Self::Ai => &AI_FIELDS,
            Self::Backup => &BACKUP_FIELDS,
            Self::Payout => &PAYOUT_FIELDS,
            Self::Juso => &JUSO_FIELDS,
        }
    }
}

const PORTONE_FIELDS: [Field; 5] = [
    f("store_id", Kind::Text, true),
    f("api_secret", Kind::Secret, true),
    f("webhook_secret", Kind::Secret, false),
    f("channel_keys", Kind::Map, false),
    f("cash_receipts", Kind::Toggle, false),
];
const TOSS_PAYMENTS_FIELDS: [Field; 6] = [
    f("client_key", Kind::Text, true),
    f("secret_key", Kind::Secret, true),
    f("billing_client_key", Kind::Text, false),
    f("billing_secret_key", Kind::Secret, false),
    f("security_key", Kind::Secret, false),
    f("active", Kind::Toggle, false),
];
const SOLAPI_FIELDS: [Field; 5] = [
    f("api_key", Kind::Secret, true),
    f("api_secret", Kind::Secret, true),
    f("sender", Kind::Text, true),
    f("pf_id", Kind::Text, false),
    f("templates", Kind::Map, false),
];
const PUSH_FIELDS: [Field; 1] = [f("service_account", Kind::File, true)];
const KAKAO_FIELDS: [Field; 2] = [f("app_id", Kind::Text, true), f("admin_key", Kind::Secret, false)];
const NAVER_FIELDS: [Field; 2] = [f("client_id", Kind::Text, true), f("client_secret", Kind::Secret, true)];
const CLIENT_ID_FIELDS: [Field; 1] = [f("client_ids", Kind::List, true)];
/// The key fields are optional together: without them sign-in works but deletion cannot revoke the Apple token.
const APPLE_FIELDS: [Field; 4] = [
    f("client_ids", Kind::List, true),
    f("team_id", Kind::Text, false),
    f("key_id", Kind::Text, false),
    f("private_key", Kind::File, false),
];
const AI_FIELDS: [Field; 6] = [
    f("api_key", Kind::Secret, true),
    f("model", Kind::Text, false),
    f("mode", Kind::Text, false),
    f("thread_hourly_cap", Kind::Text, false),
    f("member_daily_cap", Kind::Text, false),
    f("daily_cap", Kind::Text, false),
];
const BACKUP_FIELDS: [Field; 6] = [
    f("endpoint", Kind::Text, true),
    f("region", Kind::Text, true),
    f("bucket", Kind::Text, true),
    f("access_key", Kind::Secret, true),
    f("secret_key", Kind::Secret, true),
    f("retention_days", Kind::Text, false),
];
const PAYOUT_FIELDS: [Field; 3] = [
    f("mode", Kind::Text, true),
    f("secret_key", Kind::Secret, false),
    f("security_key", Kind::Secret, false),
];
const JUSO_FIELDS: [Field; 1] = [f("confm_key", Kind::Secret, true)];
const BACKUP_RETENTION_DEFAULT: u32 = 30;
const BACKUP_RETENTION_MAX: u32 = 3650;

/// A wizard value that fails validation. The code maps to a Korean message in the console.
#[derive(Debug, PartialEq, Eq)]
pub struct Invalid(pub &'static str);

fn text(v: &Map<String, Value>, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// A JSON boolean switch. Absent or null means "use the default"; any other type is refused.
fn toggle(v: &Map<String, Value>, k: &str) -> Result<Option<bool>, Invalid> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(Invalid("invalid_toggle")),
    }
}

fn required(v: &Map<String, Value>, k: &str) -> Result<String, Invalid> {
    text(v, k).ok_or(Invalid("missing_field"))
}

fn list(v: &Map<String, Value>, k: &str) -> Vec<String> {
    match v.get(k) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(Value::String(s)) => s.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        _ => Vec::new(),
    }
}

fn string_map(v: &Map<String, Value>, k: &str) -> Result<HashMap<String, String>, Invalid> {
    let raw = match v.get(k) {
        None | Some(Value::Null) => return Ok(HashMap::new()),
        Some(Value::String(s)) if s.trim().is_empty() => return Ok(HashMap::new()),
        Some(Value::String(s)) => serde_json::from_str::<Value>(s).map_err(|_| Invalid("invalid_map"))?,
        Some(other) => other.clone(),
    };
    let obj = raw.as_object().ok_or(Invalid("invalid_map"))?;
    obj.iter()
        .map(|(key, val)| val.as_str().map(|s| (key.clone(), s.trim().to_string())).ok_or(Invalid("invalid_map")))
        .collect()
}

/// An optional reply cap. Blank or missing keeps the default; a number outside `range` is refused.
fn cap(v: &Map<String, Value>, k: &str, default: u32, range: RangeInclusive<u32>) -> Result<u32, Invalid> {
    let raw = match v.get(k) {
        None | Some(Value::Null) => return Ok(default),
        Some(Value::String(s)) if s.trim().is_empty() => return Ok(default),
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(_) => return Err(Invalid("invalid_cap")),
    };
    raw.parse::<u32>().ok().filter(|n| range.contains(n)).ok_or(Invalid("invalid_cap"))
}

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// Channel slots the PortOne card takes: every checkout method, plus billing, identity and cash receipts.
fn known_channel_slot(k: &str) -> bool {
    matches!(k, "card_billing" | "identity" | "cash_receipt") || crate::routes::payments::METHODS.iter().any(|m| m.0 == k)
}

/// PortOne's secret format is not documented here, so only values that cannot be a key are refused.
fn check_api_secret(s: &str) -> Result<(), Invalid> {
    if s.chars().any(char::is_whitespace) || !(16..=200).contains(&s.chars().count()) {
        return Err(Invalid("invalid_api_secret"));
    }
    Ok(())
}

fn check_webhook_secret(s: &str) -> Result<(), Invalid> {
    let raw = s.strip_prefix("whsec_").ok_or(Invalid("invalid_webhook_secret"))?;
    STANDARD.decode(raw).map(|_| ()).map_err(|_| Invalid("invalid_webhook_secret"))
}

/// Backup targets are public object stores; refuse loopback/private/link-local hosts so a saved
/// endpoint can't be used to probe services inside the server's network.
fn public_host(endpoint: &str) -> bool {
    let rest = endpoint.trim_start_matches("https://");
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.split_once(']'))
        .map_or_else(|| host.rsplit_once(':').map_or(host, |(h, _)| h), |(h, _)| h);
    let lower = host.to_ascii_lowercase();
    if lower.is_empty() || lower == "localhost" || lower.ends_with(".localhost") || lower.ends_with(".internal") || lower.ends_with(".local") {
        return false;
    }
    match lower.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => !(ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast()),
        Ok(std::net::IpAddr::V6(ip)) => {
            !(ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80)
        }
        Err(_) => true,
    }
}

fn s3_config(v: &Map<String, Value>) -> Result<S3Config, Invalid> {
    let endpoint = required(v, "endpoint")?.trim_end_matches('/').to_string();
    if !endpoint.starts_with("https://") || endpoint.contains(char::is_whitespace) || !public_host(&endpoint) {
        return Err(Invalid("invalid_endpoint"));
    }
    let region = required(v, "region")?;
    if !region.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(Invalid("invalid_region"));
    }
    let bucket = required(v, "bucket")?;
    let bucket_ok = (3..=63).contains(&bucket.len()) && bucket.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
    if !bucket_ok {
        return Err(Invalid("invalid_bucket"));
    }
    let retention_days = match text(v, "retention_days") {
        None => BACKUP_RETENTION_DEFAULT,
        Some(d) => d
            .parse::<u32>()
            .ok()
            .filter(|d| (1..=BACKUP_RETENTION_MAX).contains(d))
            .ok_or(Invalid("invalid_retention"))?,
    };
    Ok(S3Config {
        endpoint,
        region,
        bucket,
        access_key: required(v, "access_key")?,
        secret_key: required(v, "secret_key")?,
        retention_days,
        instance_role: None,
    })
}

fn payout_settings(v: &Map<String, Value>, toss_ready: bool) -> Result<PayoutSettings, Invalid> {
    let mode = match text(v, "mode").as_deref() {
        Some("manual") => PayoutMode::Manual,
        Some("toss") => PayoutMode::Toss,
        _ => return Err(Invalid("invalid_mode")),
    };
    let secret_key = text(v, "secret_key");
    let security_key = text(v, "security_key");
    if mode == PayoutMode::Toss {
        let (Some(secret), Some(security)) = (&secret_key, &security_key) else {
            return Err(Invalid("missing_field"));
        };
        if connected_code(secret).is_none() {
            return Err(Invalid("invalid_secret_key"));
        }
        crate::providers::toss_jwe::key_bytes(security).map_err(|_| Invalid("invalid_security_key"))?;
        if !toss_ready {
            return Err(Invalid("toss_unverified"));
        }
    }
    Ok(PayoutSettings {
        mode,
        secret_key,
        security_key,
    })
}

/// `test` or `live` for a Toss API 개별 연동 key of `kind` (`ck` client, `sk` secret). Widget keys (`gck`/`gsk`) and
/// anything with whitespace are refused.
fn toss_key(key: &str, kind: &str) -> Option<&'static str> {
    let body = |mode: &str| {
        key.strip_prefix(&format!("{mode}_{kind}_"))
            .filter(|r| !r.is_empty() && !r.contains(char::is_whitespace))
    };
    if body("test").is_some() {
        Some("test")
    } else if body("live").is_some() {
        Some("live")
    } else {
        None
    }
}

fn toss_payments(v: &Map<String, Value>) -> Result<TossPayments, Invalid> {
    let client_key = required(v, "client_key")?;
    let secret_key = required(v, "secret_key")?;
    let mut modes = vec![
        toss_key(&client_key, "ck").ok_or(Invalid("invalid_client_key"))?,
        toss_key(&secret_key, "sk").ok_or(Invalid("invalid_secret_key"))?,
    ];
    let billing = match (text(v, "billing_client_key"), text(v, "billing_secret_key")) {
        (None, None) => None,
        (Some(ck), Some(sk)) => {
            modes.push(toss_key(&ck, "ck").ok_or(Invalid("invalid_client_key"))?);
            modes.push(toss_key(&sk, "sk").ok_or(Invalid("invalid_secret_key"))?);
            Some((ck, sk))
        }
        _ => return Err(Invalid("missing_field")),
    };
    if modes.iter().any(|m| *m != modes[0]) {
        return Err(Invalid("key_mode_mismatch"));
    }
    let security_key = text(v, "security_key");
    if let Some(k) = &security_key {
        crate::providers::toss_jwe::key_bytes(k).map_err(|_| Invalid("invalid_security_key"))?;
    }
    // Not switched yet: live keys take payments, test keys wait for the owner (a live server must not take bookings
    // on test keys just because the keys were saved).
    let active = toggle(v, "active")?.unwrap_or(toss_key_mode(&secret_key) == Some("live"));
    Ok(TossPayments {
        client_key,
        secret_key,
        billing,
        security_key,
        active,
    })
}

/// Team id, key id and .p8 key go together: all three or none.
fn apple_key(v: &Map<String, Value>) -> Result<Option<AppleSignInKey>, Invalid> {
    match (text(v, "team_id"), text(v, "key_id"), text(v, "private_key")) {
        (None, None, None) => Ok(None),
        (Some(team), Some(key), Some(pem)) => AppleSignInKey::new(&team, &key, &pem).map(Some).map_err(Invalid),
        _ => Err(Invalid("missing_field")),
    }
}

fn service_account(v: &Map<String, Value>) -> Result<ServiceAccount, Invalid> {
    let parsed = match v.get("service_account") {
        Some(Value::String(s)) => serde_json::from_str::<ServiceAccount>(s.trim()),
        Some(obj @ Value::Object(_)) => serde_json::from_value::<ServiceAccount>(obj.clone()),
        _ => return Err(Invalid("missing_field")),
    };
    let acc = parsed.map_err(|_| Invalid("invalid_service_account"))?;
    if !acc.private_key.contains("PRIVATE KEY") {
        return Err(Invalid("invalid_service_account"));
    }
    Ok(acc)
}

/// True when the env, not the saved card, supplies this field. The card shows such a field as set.
pub fn env_provides(base: &Integrations, section: Section, field: &str) -> bool {
    match (section, field) {
        (Section::Portone, "webhook_secret") => base.portone.webhook_secret.is_some(),
        (Section::Portone, "channel_keys") => !base.portone.channel_keys.is_empty(),
        _ => false,
    }
}

/// Overlays one console section onto `base`. Fails when a value is missing or malformed.
pub fn apply(base: &Integrations, section: Section, v: &Map<String, Value>) -> Result<Integrations, Invalid> {
    let mut out = base.clone();
    match section {
        Section::Portone => {
            let webhook_secret = text(v, "webhook_secret");
            if let Some(w) = &webhook_secret {
                check_webhook_secret(w)?;
            }
            let store_id = required(v, "store_id")?;
            if !store_id.starts_with(PORTONE_STORE_PREFIX) {
                return Err(Invalid("invalid_store_id"));
            }
            let section_keys = string_map(v, "channel_keys")?;
            if section_keys.keys().any(|k| !known_channel_slot(k)) {
                return Err(Invalid("unknown_channel_key"));
            }
            if section_keys.values().any(|k| !k.starts_with(PORTONE_CHANNEL_PREFIX)) {
                return Err(Invalid("invalid_channel_key"));
            }
            // Keys the card does not set (env-provided) stay; the card's own keys win.
            let mut channel_keys = base.portone.channel_keys.clone();
            channel_keys.extend(section_keys);
            let api_secret = required(v, "api_secret")?;
            check_api_secret(&api_secret)?;
            out.portone = PortOne {
                api_secret: Some(api_secret),
                webhook_secret: webhook_secret.or_else(|| base.portone.webhook_secret.clone()),
                store_id: Some(store_id),
                channel_keys,
                cash_receipts: toggle(v, "cash_receipts")?.unwrap_or(base.portone.cash_receipts),
            };
        }
        Section::Solapi => {
            let sender = digits(&required(v, "sender")?);
            if !(8..=12).contains(&sender.len()) {
                return Err(Invalid("invalid_sender"));
            }
            let templates = string_map(v, "templates")?;
            // A misspelled key would silently fall back to SMS for that alert, so refuse it here.
            if templates.keys().any(|k| !crate::providers::sms::TEMPLATE_KEYS.contains(&k.as_str())) {
                return Err(Invalid("unknown_template_key"));
            }
            out.solapi = Some(SolapiConfig {
                api_key: required(v, "api_key")?,
                api_secret: required(v, "api_secret")?,
                sender,
                pf_id: text(v, "pf_id"),
                templates,
            });
        }
        Section::TossPayments => out.toss_payments = Some(toss_payments(v)?),
        Section::Push => out.fcm = Some(service_account(v)?),
        Section::Backup => out.backup = Some(s3_config(v)?),
        Section::Payout => out.payout = Some(payout_settings(v, base.payout_toss_ready)?),
        Section::Kakao => {
            out.kakao_app_id = Some(required(v, "app_id")?);
            out.kakao_admin_key = text(v, "admin_key");
        }
        Section::Naver => {
            out.naver_client_id = Some(required(v, "client_id")?);
            out.naver_client_secret = Some(required(v, "client_secret")?);
        }
        Section::Juso => out.juso_confm_key = Some(required(v, "confm_key")?),
        Section::Ai => {
            let mode = Mode::parse(text(v, "mode").as_deref()).ok_or(Invalid("invalid_mode"))?;
            let model = text(v, "model").unwrap_or_else(|| ai::DEFAULT_MODEL.to_string());
            if model.len() > 64 || !model.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
                return Err(Invalid("invalid_model"));
            }
            let caps = Caps {
                thread_hourly: cap(v, "thread_hourly_cap", ai::DEFAULT_THREAD_HOURLY_CAP, ai::THREAD_HOURLY_RANGE)?,
                member_daily: cap(v, "member_daily_cap", ai::DEFAULT_MEMBER_DAILY_CAP, ai::MEMBER_DAILY_RANGE)?,
                daily: cap(v, "daily_cap", ai::DEFAULT_DAILY_CAP, ai::DAILY_RANGE)?,
            };
            out.ai = Some(AiConfig {
                api_key: required(v, "api_key")?,
                model,
                mode,
                caps,
            });
        }
        Section::Google | Section::Apple => {
            let ids = list(v, "client_ids");
            if ids.is_empty() {
                return Err(Invalid("missing_field"));
            }
            if section == Section::Google {
                out.google_client_ids = ids;
            } else {
                out.apple_client_ids = ids;
                out.apple_signin = apple_key(v)?;
            }
        }
    }
    Ok(out)
}

/// New stored values: `incoming` over `current`. A blank or missing field keeps the saved value
/// (the wizard never sends secrets back); `null` clears it. Unknown fields are dropped.
pub fn merge(section: Section, current: &Map<String, Value>, incoming: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for field in section.fields() {
        let next = match incoming.get(field.name) {
            Some(Value::Null) => None,
            Some(v) if field.kind == Kind::Map => merge_map(current.get(field.name), v),
            Some(Value::String(s)) if s.trim().is_empty() => current.get(field.name).cloned(),
            Some(v) => Some(v.clone()),
            None => current.get(field.name).cloned(),
        };
        if let Some(v) = next {
            out.insert(field.name.to_string(), v);
        }
    }
    // The Toss switch is stored on save, so the console shows what runs and a later key change keeps it.
    if section == Section::TossPayments && !out.contains_key("active") {
        let live = out.get("secret_key").and_then(Value::as_str).and_then(toss_key_mode) == Some("live");
        out.insert("active".into(), json!(live));
    }
    out
}

/// A map field's saved entries overlaid with the incoming ones, so saving one slot keeps the others.
/// `null` removes an entry and a blank keeps it. A value that is not a map passes through for `apply` to refuse.
fn merge_map(current: Option<&Value>, incoming: &Value) -> Option<Value> {
    let Some(next) = map_entries(incoming) else { return Some(incoming.clone()) };
    let mut out = current.and_then(map_entries).unwrap_or_default();
    for (key, val) in next {
        match val {
            Value::Null => {
                out.remove(&key);
            }
            Value::String(s) if s.trim().is_empty() => {}
            val => {
                out.insert(key, val);
            }
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

/// A map field as an object: a JSON object, or the same object as text (how the wizard sends channel slots).
fn map_entries(v: &Value) -> Option<Map<String, Value>> {
    match v {
        Value::Object(o) => Some(o.clone()),
        Value::String(s) if s.trim().is_empty() => Some(Map::new()),
        Value::String(s) => serde_json::from_str::<Value>(s).ok()?.as_object().cloned(),
        _ => None,
    }
}

const MASK: &str = "••••";

/// What the wizard shows for a saved value. Secrets and map values (channel keys, template ids) show only the mask,
/// so they cannot be read back. Identifiers and switches (store id, toss mode, sender) show in full, so the owner can
/// check which account is connected.
pub fn preview(kind: Kind, v: &Value) -> Value {
    match kind {
        Kind::Text | Kind::List if v.is_null() => Value::Null,
        Kind::Text | Kind::List => v.clone(),
        Kind::Secret if v.is_null() => Value::Null,
        Kind::Secret => json!(MASK),
        Kind::Map => match v {
            Value::Object(o) => Value::Object(o.keys().map(|k| (k.clone(), json!(MASK))).collect()),
            _ => v.clone(),
        },
        Kind::File => {
            let email = match v {
                Value::String(s) => serde_json::from_str::<Value>(s).ok().and_then(|j| j["client_email"].as_str().map(String::from)),
                Value::Object(o) => o.get("client_email").and_then(Value::as_str).map(String::from),
                _ => None,
            };
            json!(email.unwrap_or_else(|| MASK.to_string()))
        }
        _ => v.clone(),
    }
}

/// The value a toggle shows when nothing is saved: the env default for that switch.
pub fn toggle_default(base: &Integrations, section: Section, name: &str) -> bool {
    match (section, name) {
        (Section::Portone, "cash_receipts") => base.portone.cash_receipts,
        (Section::TossPayments, "active") => true,
        _ => false,
    }
}

/// Saved console values for one section, decrypted. Empty when nothing was saved.
pub async fn load_section(db: &PgPool, key: &[u8; 32], section: Section) -> Result<Map<String, Value>, sqlx::Error> {
    let row: Option<String> = sqlx::query_scalar("SELECT secret FROM integration_settings WHERE section = $1")
        .bind(section.code())
        .fetch_optional(db)
        .await?;
    Ok(row.and_then(|enc| decode(key, &enc)).unwrap_or_default())
}

fn decode(key: &[u8; 32], enc: &str) -> Option<Map<String, Value>> {
    let plain = crypto::decrypt(key, crypto::aad::INTEGRATION, enc)?;
    serde_json::from_str::<Value>(&plain).ok()?.as_object().cloned()
}

/// Where a saved card stands: a saved card is locked unless an owner unlock is still inside its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    Unsaved,
    Locked,
    /// Open until the instant, for the owner with this staff id.
    Unlocked(DateTime<Utc>, uuid::Uuid),
}

pub async fn lock_state(db: &PgPool, section: Section) -> Result<LockState, sqlx::Error> {
    let row: Option<(Option<DateTime<Utc>>, Option<uuid::Uuid>)> =
        sqlx::query_as("SELECT CASE WHEN unlocked_until > now() THEN unlocked_until END, unlocked_by FROM integration_settings WHERE section = $1")
            .bind(section.code())
            .fetch_optional(db)
            .await?;
    Ok(match row {
        None => LockState::Unsaved,
        Some((Some(until), Some(by))) => LockState::Unlocked(until, by),
        // An open window with no owner recorded fails closed.
        Some(_) => LockState::Locked,
    })
}

/// Saves the card and locks it again. Returns false unless `by` holds an open unlock of the card: the condition
/// is in the statement, so two saves racing inside one unlock window cannot both pass. An empty map clears the card.
pub async fn save_section(db: &PgPool, key: &[u8; 32], section: Section, values: &Map<String, Value>, by: uuid::Uuid) -> Result<bool, sqlx::Error> {
    let enc = crypto::encrypt(key, crypto::aad::INTEGRATION, &Value::Object(values.clone()).to_string());
    sqlx::query(
        "INSERT INTO integration_settings (section, secret, updated_by) VALUES ($1, $2, $3)
         ON CONFLICT (section) DO UPDATE SET secret = EXCLUDED.secret, updated_by = EXCLUDED.updated_by, updated_at = now(), last_test = NULL, last_check = NULL,
             unlocked_until = NULL, unlocked_by = NULL
         WHERE integration_settings.unlocked_until > now() AND integration_settings.unlocked_by = EXCLUDED.updated_by",
    )
    .bind(section.code())
    .bind(enc)
    .bind(by)
    .execute(db)
    .await
    .map(|r| r.rows_affected() == 1)
}

/// Env keys overlaid with every saved console section. A section that no longer validates
/// (e.g. after DATA_KEY rotation) is skipped with a warning so the server still starts.
pub async fn load(db: &PgPool, config: &Config) -> Result<Integrations, sqlx::Error> {
    let rows: Vec<(String, String, Option<Value>)> = sqlx::query_as("SELECT section, secret, last_test FROM integration_settings")
        .fetch_all(db)
        .await?;
    let mut out = Integrations::from_config(config);
    let toss_on = toss_switch_on(db).await?;
    for (code, enc, last_test) in rows {
        let Some(section) = Section::parse(&code) else { continue };
        let Some(values) = decode(&config.data_key, &enc) else {
            tracing::warn!(section = code, "setup section cannot be decrypted; ignored");
            continue;
        };
        // A cleared card keeps its row, locked, with no values: it counts as not saved.
        if values.is_empty() {
            continue;
        }
        if section == Section::Payout {
            let secret = values.get("secret_key").and_then(Value::as_str).unwrap_or_default();
            out.payout_toss_ready = toss_ready(toss_on, last_test.as_ref(), &config.data_key, secret);
        }
        match apply(&out, section, &values) {
            Ok(next) => out = next,
            Err(Invalid(reason)) => tracing::warn!(section = code, reason, "setup section invalid; ignored"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
