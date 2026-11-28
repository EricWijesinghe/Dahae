// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The adapter registry: every external service this image can talk to, one `AdapterSpec` each, in one explicit
//! list (`ADAPTERS`), so a grep shows exactly what the image contains. One trait per capability (`payments`,
//! `payouts`, `messaging`, `login`, `address`, `invoice`, `identity`); adapters return `BoxFuture`s.
//!
//! Codes are forever (D12): a code stored in a row (`payments.provider`, `payment_methods.gateway`,
//! `refund_queue.provider`, `payouts.provider`) is never removed or renamed. Read before write (D11): a new adapter
//! ships `Dark` (readable, never offered or written) one release before it turns `Available`, so a rollback target
//! always knows every code a live row can hold. By convention adapter modules touch neither `sqlx` nor `axum`.

pub mod address;
pub mod fake;
pub mod fake_ports;
pub mod http_guard;
pub mod identity;
pub mod invoice;
pub mod juso;
pub mod login;
pub mod manual;
pub mod messaging;
pub mod payments;
pub mod payouts;
pub mod portone;
pub mod portone_cash_receipt;
pub mod portone_identity;
pub mod social;
pub mod solapi;
pub mod toss_payments;
pub mod toss_payouts;

use crate::integrations::{Field, Section};
use crate::providers::{ProviderError, Providers, TOSS, TOSS_BILLING};
use futures_util::future::BoxFuture;
use std::sync::Arc;

pub use http_guard::HttpGuard;

/// Adapter results keep today's error variants and their mapping to `ApiError` (`providers::ProviderError`).
pub type R<T> = Result<T, ProviderError>;
pub type Fut<'a, T> = BoxFuture<'a, R<T>>;

/// Capability flags: a u32 newtype, so no bitflags dependency for a few constants. There is deliberately no raw
/// card-number flag: card data never reaches this server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps(pub u32);

impl Caps {
    pub const NONE: Caps = Caps(0);
    pub const CARD: Caps = Caps(1);
    pub const LOCAL_METHODS: Caps = Caps(2);
    pub const PARTIAL_REFUND: Caps = Caps(4);
    pub const SAVED_METHOD: Caps = Caps(8);
    pub const SPLIT: Caps = Caps(16);
    pub const ASYNC_RAIL: Caps = Caps(32);
    pub const INSTALLMENTS: Caps = Caps(64);
    pub const WEBHOOK_SIGNED: Caps = Caps(128);
    pub const TEMPLATES_REQUIRED: Caps = Caps(256);
    pub const DELIVERY_RECEIPTS: Caps = Caps(512);
    pub const ONBOARD_KYC: Caps = Caps(1024);

    pub const fn has(self, o: Caps) -> bool {
        self.0 & o.0 == o.0
    }

    pub const fn with(self, o: Caps) -> Caps {
        Caps(self.0 | o.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Payments,
    Payouts,
    Messaging,
    Login,
    Address,
    Invoice,
    Identity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyMode {
    Test,
    Live,
    Unknown,
}

/// How far a webhook body is trusted. Either way it is only a hint: money moves after a `fetch` (D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebhookTrust {
    /// Every delivery carries a signature that must verify.
    Signed,
    /// The signature is optional or absent: the body names what to re-read, nothing more.
    RefetchOnly,
}

/// Where an adapter is in its life (D11, D12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterStatus {
    /// Compiled in and readable (rows, refunds, webhooks), never offered or written. Every new adapter starts here.
    Dark,
    /// Offered to new setups and written to rows.
    Available,
    /// Hidden from new setups; existing rows keep refunds, webhooks and saved-card charges.
    Deprecated,
}

/// The built adapter behind a code, by capability.
#[derive(Clone)]
pub enum AnyAdapter {
    Pay(Arc<dyn payments::PaymentGateway>),
    Payout(Arc<dyn payouts::PayoutRail>),
    Msg(Arc<dyn messaging::Messenger>),
    Login(Arc<dyn login::SocialVerifier>),
    Address(Arc<dyn address::AddressLookup>),
    Invoice(Arc<dyn invoice::InvoiceSink>),
    Identity(Arc<dyn identity::IdentityVerifier>),
}

/// Builds an adapter on the live keys for one of its stored codes.
pub type Build = fn(&Providers, &'static str) -> AnyAdapter;

pub struct AdapterSpec {
    /// The code new rows store, forever.
    pub id: &'static str,
    /// Further codes this adapter writes or once wrote, forever (Toss's billing MID: `tosspayments_billing`).
    pub extra_codes: &'static [&'static str],
    /// `integration_settings.section` its keys live in; "" for an adapter without keys.
    pub section: &'static str,
    pub capability: Capability,
    /// ISO 3166-1 alpha-2 codes, or `["*"]`.
    pub countries: &'static [&'static str],
    pub caps: Caps,
    /// The hosts it calls (the `HttpGuard` allowlist), sandbox and live.
    pub hosts: &'static [&'static str],
    pub webhook: Option<WebhookTrust>,
    pub status: AdapterStatus,
    /// None for a code with nothing of its own to call.
    pub build: Option<Build>,
}

impl AdapterSpec {
    /// Every code rows may hold for this adapter: `id` first.
    pub fn codes(&self) -> impl Iterator<Item = &'static str> + use<> {
        std::iter::once(self.id).chain(self.extra_codes.iter().copied())
    }

    /// New rows and new setups may use it (D11): only `Available` adapters are written.
    pub fn writable(&self) -> bool {
        self.status == AdapterStatus::Available
    }

    /// The setup-wizard fields of its section (`integrations::Section`); none for an adapter without keys.
    pub fn fields(&self) -> &'static [Field] {
        Section::parse(self.section).map_or(&[], Section::fields)
    }
}

const KR: &[&str] = &["KR"];
const ANYWHERE: &[&str] = &["*"];

const BASE: AdapterSpec = AdapterSpec {
    id: "",
    extra_codes: &[],
    section: "",
    capability: Capability::Payments,
    countries: KR,
    caps: Caps::NONE,
    hosts: &[],
    webhook: None,
    status: AdapterStatus::Available,
    build: None,
};

const KR_PG: Caps = Caps::CARD.with(Caps::LOCAL_METHODS).with(Caps::PARTIAL_REFUND).with(Caps::SAVED_METHOD);

/// Every adapter in this image. Order is display order within a capability. Never remove an entry (D12): retire it
/// as `Deprecated`. Add new ones as `Dark` and flip them to `Available` one release later (D11).
pub static ADAPTERS: &[AdapterSpec] = &[
    AdapterSpec {
        id: "portone",
        section: "portone",
        caps: KR_PG.with(Caps::WEBHOOK_SIGNED),
        hosts: &["api.portone.io"],
        webhook: Some(WebhookTrust::Signed),
        build: Some(portone::build),
        ..BASE
    },
    AdapterSpec {
        id: TOSS,
        extra_codes: &[TOSS_BILLING],
        section: TOSS,
        caps: KR_PG.with(Caps::INSTALLMENTS),
        hosts: &["api.tosspayments.com"],
        webhook: Some(WebhookTrust::RefetchOnly),
        build: Some(toss_payments::build),
        ..BASE
    },
    // The pre-release fake gateway's `payments.provider`, still in long-lived dev databases. Served like PortOne.
    AdapterSpec {
        id: "pg",
        countries: ANYWHERE,
        status: AdapterStatus::Deprecated,
        build: Some(portone::build),
        ..BASE
    },
    AdapterSpec {
        id: "toss",
        section: "payout",
        capability: Capability::Payouts,
        caps: Caps::ONBOARD_KYC.with(Caps::WEBHOOK_SIGNED),
        hosts: &["api.tosspayments.com"],
        webhook: Some(WebhookTrust::Signed),
        build: Some(toss_payouts::build),
        ..BASE
    },
    AdapterSpec {
        id: "manual",
        capability: Capability::Payouts,
        countries: ANYWHERE,
        build: Some(manual::build),
        ..BASE
    },
    AdapterSpec {
        id: "solapi",
        section: "solapi",
        capability: Capability::Messaging,
        caps: Caps::TEMPLATES_REQUIRED,
        hosts: &["api.solapi.com"],
        build: Some(solapi::build),
        ..BASE
    },
    AdapterSpec {
        id: "kakao",
        section: "kakao",
        capability: Capability::Login,
        hosts: &["kapi.kakao.com"],
        build: Some(social::build),
        ..BASE
    },
    AdapterSpec {
        id: "naver",
        section: "naver",
        capability: Capability::Login,
        hosts: &["nid.naver.com", "openapi.naver.com"],
        build: Some(social::build),
        ..BASE
    },
    AdapterSpec {
        id: "google",
        section: "google",
        capability: Capability::Login,
        countries: ANYWHERE,
        hosts: &["www.googleapis.com", "oauth2.googleapis.com"],
        build: Some(social::build),
        ..BASE
    },
    AdapterSpec {
        id: "apple",
        section: "apple",
        capability: Capability::Login,
        countries: ANYWHERE,
        hosts: &["appleid.apple.com"],
        build: Some(social::build),
        ..BASE
    },
    AdapterSpec {
        id: "juso",
        section: "juso",
        capability: Capability::Address,
        hosts: &["business.juso.go.kr"],
        build: Some(juso::build),
        ..BASE
    },
    // PortOne's cash receipts and PASS, on the PortOne keys. New codes, so Dark (D11): no row or pack names them
    // yet; today's routes call the same code directly.
    AdapterSpec {
        id: "portone_cash_receipt",
        section: "portone",
        capability: Capability::Invoice,
        hosts: &["api.portone.io"],
        status: AdapterStatus::Dark,
        build: Some(portone_cash_receipt::build),
        ..BASE
    },
    AdapterSpec {
        id: "portone_identity",
        section: "portone",
        capability: Capability::Identity,
        hosts: &["api.portone.io"],
        status: AdapterStatus::Dark,
        build: Some(portone_identity::build),
        ..BASE
    },
];

/// The spec a stored code belongs to, whatever its status: rows written by any release stay readable.
pub fn spec(code: &str) -> Option<&'static AdapterSpec> {
    ADAPTERS.iter().find(|s| s.codes().any(|c| c == code))
}

/// Every code of the adapters with one of these capabilities, in every status: what a live row may name.
pub fn stored_codes(capabilities: &[Capability]) -> Vec<&'static str> {
    ADAPTERS
        .iter()
        .filter(|s| capabilities.contains(&s.capability))
        .flat_map(AdapterSpec::codes)
        .collect()
}

/// Every code a country pack may recommend: those of the `Available` adapters.
pub fn offerable_codes() -> Vec<&'static str> {
    ADAPTERS.iter().filter(|s| s.writable()).flat_map(AdapterSpec::codes).collect()
}

/// The adapter for a stored code, on the live keys. An unknown code, or one whose capability has no trait shim
/// yet, is `NotConfigured`: fail closed.
pub fn adapter(p: &Providers, code: &str) -> R<AnyAdapter> {
    let spec = spec(code).ok_or(ProviderError::NotConfigured("adapter"))?;
    let build = spec.build.ok_or(ProviderError::NotConfigured("adapter"))?;
    let code = spec.codes().find(|c| *c == code).unwrap_or(spec.id);
    Ok(build(p, code))
}

/// The payment gateway behind a `payments.provider` or `checkouts.gateway` code.
pub fn payment_gateway(p: &Providers, code: &str) -> R<Arc<dyn payments::PaymentGateway>> {
    match adapter(p, code)? {
        AnyAdapter::Pay(g) => Ok(g),
        AnyAdapter::Payout(_) | AnyAdapter::Msg(_) | AnyAdapter::Login(_) | AnyAdapter::Address(_) | AnyAdapter::Invoice(_) | AnyAdapter::Identity(_) => {
            Err(ProviderError::NotConfigured("adapter"))
        }
    }
}

/// `$name(p, code)`: the adapter behind a code when it has capability `$variant`, else `NotConfigured`.
macro_rules! port {
    ($(#[$doc:meta])* $name:ident, $variant:ident, $tr:path) => {
        $(#[$doc])*
        pub fn $name(p: &Providers, code: &str) -> R<Arc<dyn $tr>> {
            match adapter(p, code)? {
                AnyAdapter::$variant(a) => Ok(a),
                _ => Err(ProviderError::NotConfigured("adapter")),
            }
        }
    };
}

port!(
    /// The payout rail behind a `payouts.provider` code.
    payout_rail, Payout, payouts::PayoutRail
);
port!(
    /// The messenger behind a code (`solapi`).
    messenger, Msg, messaging::Messenger
);
port!(
    /// The sign-in verifier behind a `user_identities.provider` code.
    social_verifier, Login, login::SocialVerifier
);
port!(
    /// The address search behind a code (`juso`).
    address_lookup, Address, address::AddressLookup
);
port!(
    /// The tax-document issuer behind a code (`portone_cash_receipt`).
    invoice_sink, Invoice, invoice::InvoiceSink
);
port!(
    /// The identity verifier behind a code (`portone_identity`).
    identity_verifier, Identity, identity::IdentityVerifier
);

/// Codes the previous release (1.6.6) already reads. D11: an adapter may only be `Available` (written) when its
/// codes are here. Raise this list once per release, to what that release shipped, never ahead of it.
pub const READ_BY_PREVIOUS_RELEASE: &[&str] = &[
    "portone",
    "tosspayments",
    "tosspayments_billing",
    "pg",
    "toss",
    "manual",
    "solapi",
    "kakao",
    "naver",
    "google",
    "apple",
    "juso",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_never_empty() {
        let mut all: Vec<&str> = ADAPTERS.iter().flat_map(AdapterSpec::codes).collect();
        assert!(all.iter().all(|c| !c.is_empty()));
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n, "a code belongs to one adapter");
    }

    #[test]
    fn written_codes_were_read_by_the_previous_release() {
        for s in ADAPTERS.iter().filter(|s| s.writable()) {
            for c in s.codes() {
                assert!(
                    READ_BY_PREVIOUS_RELEASE.contains(&c),
                    "{c} is written before a release reads it (D11): ship it Dark first"
                );
            }
        }
    }

    #[test]
    fn readiness_knows_the_same_codes_as_before_the_registry() {
        let mut known = stored_codes(&[Capability::Payments, Capability::Payouts]);
        known.sort_unstable();
        assert_eq!(known, ["manual", "pg", "portone", "toss", "tosspayments", "tosspayments_billing"]);
    }

    #[test]
    fn packs_may_name_the_same_adapters_as_before_the_registry() {
        let mut offered = offerable_codes();
        offered.sort_unstable();
        let mut before = [
            "portone",
            "tosspayments",
            "tosspayments_billing",
            "solapi",
            "kakao",
            "naver",
            "google",
            "apple",
            "juso",
            "manual",
            "toss",
        ];
        before.sort_unstable();
        assert_eq!(offered, before);
    }

    #[test]
    fn sections_name_real_wizard_cards() {
        for s in ADAPTERS.iter().filter(|s| !s.section.is_empty()) {
            assert!(Section::parse(s.section).is_some(), "{}: unknown section {}", s.id, s.section);
            assert!(!s.fields().is_empty(), "{}", s.id);
        }
        assert!(spec("manual").is_some_and(|s| s.fields().is_empty()));
    }

    #[test]
    fn spec_finds_extra_codes_and_refuses_unknown_ones() {
        assert_eq!(spec(TOSS_BILLING).map(|s| s.id), Some(TOSS));
        assert!(spec("stripe").is_none());
        assert!(spec("").is_none());
    }

    /// R2: every code with something of its own to call has a trait shim; the legacy `pg` borrows PortOne's.
    #[test]
    fn every_adapter_builds_behind_its_capability() {
        for s in ADAPTERS {
            assert!(s.build.is_some(), "{} has no shim", s.id);
        }
        let dark: Vec<&str> = ADAPTERS.iter().filter(|s| s.status == AdapterStatus::Dark).map(|s| s.id).collect();
        assert_eq!(dark, ["portone_cash_receipt", "portone_identity"]);
    }

    #[test]
    fn caps_combine() {
        assert!(KR_PG.has(Caps::CARD) && KR_PG.has(Caps::SAVED_METHOD));
        assert!(!KR_PG.has(Caps::SPLIT));
        assert!(Caps::NONE.has(Caps::NONE));
    }

    /// D5: adapters reach the database and the HTTP framework only through core.
    #[test]
    fn adapters_touch_neither_sqlx_nor_axum() {
        let sources = [
            include_str!("mod.rs"),
            include_str!("address.rs"),
            include_str!("fake.rs"),
            include_str!("fake_ports.rs"),
            include_str!("http_guard.rs"),
            include_str!("identity.rs"),
            include_str!("invoice.rs"),
            include_str!("juso.rs"),
            include_str!("login.rs"),
            include_str!("manual.rs"),
            include_str!("messaging.rs"),
            include_str!("payments.rs"),
            include_str!("payouts.rs"),
            include_str!("portone.rs"),
            include_str!("portone_cash_receipt.rs"),
            include_str!("portone_identity.rs"),
            include_str!("social.rs"),
            include_str!("solapi.rs"),
            include_str!("toss_payments.rs"),
            include_str!("toss_payouts.rs"),
        ];
        let banned = [concat!("sql", "x::"), concat!("ax", "um::")];
        for src in sources {
            assert!(banned.iter().all(|b| !src.contains(b)), "an adapter module uses sqlx or axum");
        }
    }
}
