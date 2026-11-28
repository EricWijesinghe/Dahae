// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The payment port. A gateway starts, confirms, reads and refunds payments and verifies its webhooks; it never
//! decides "paid" (D8). Core compares what `fetch` returns with its own row (`routes::checkouts::amount_matches`:
//! status, amount and currency) before any money state moves.
//!
//! Answers use today's `providers::PaymentInfo` (PortOne-shaped statuses and method object), so core code reads
//! every gateway the same way; `PaymentInfo::pay_status` gives the typed view.

use super::{Fut, R};
use crate::providers::PaymentInfo;
use chrono::{DateTime, Utc};
use domain::money::Money;
use reqwest::header::HeaderMap;
use serde_json::Value;
use uuid::Uuid;

/// A method code: "card", "kakaopay", "virtual_account" (KR codes stay unprefixed); "br.pix", "jp.konbini" later.
pub type MethodId = &'static str;

/// The methods a KR gateway can serve (`routes::payments::METHODS`). Which one serves a method follows
/// `Integrations::gateway_for`.
pub const KR_METHODS: [MethodId; 10] = [
    "card",
    "kakaopay",
    "naverpay",
    "tosspay",
    "payco",
    "samsungpay",
    "applepay",
    "transfer",
    "virtual_account",
    "phone",
];

/// A new checkout, already stored with its gateway, amount and currency.
pub struct CheckoutReq {
    pub checkout_id: Uuid,
    pub amount: Money,
    /// ISO 4217, the install's currency.
    pub currency: String,
    pub method: String,
}

/// How the customer pays.
#[derive(Debug, PartialEq)]
pub enum CheckoutStart {
    Redirect {
        url: String,
    },
    /// The app opens the gateway's SDK with these parameters.
    Sdk {
        params: Value,
    },
    /// Pay later off-line (virtual account, Pix, konbini).
    AwaitingCustomer {
        reference: String,
        expires_at: DateTime<Utc>,
        instructions_key: &'static str,
    },
}

/// The typed view of a payment's provider status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayStatus {
    Pending,
    /// Waiting for an off-line deposit (virtual account).
    PendingOffline {
        expires_at: Option<DateTime<Utc>>,
    },
    Paid,
    PartiallyRefunded,
    Refunded,
    Failed,
    /// Ended without money taken.
    Cancelled,
    /// A status this build does not know. It never settles anything.
    Unknown(String),
}

impl PaymentInfo {
    /// The provider status as a `PayStatus`. Toss statuses arrive already mapped to PortOne's names
    /// (`providers::toss_payments::status`).
    pub fn pay_status(&self) -> PayStatus {
        match self.status.as_str() {
            "READY" | "PAY_PENDING" => PayStatus::Pending,
            "VIRTUAL_ACCOUNT_ISSUED" => PayStatus::PendingOffline {
                expires_at: self.method["expiredAt"]
                    .as_str()
                    .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                    .map(|t| t.with_timezone(&Utc)),
            },
            "PAID" => PayStatus::Paid,
            "PARTIAL_CANCELLED" => PayStatus::PartiallyRefunded,
            "CANCELLED" if self.cancelled > 0 => PayStatus::Refunded,
            "CANCELLED" => PayStatus::Cancelled,
            "FAILED" => PayStatus::Failed,
            other => PayStatus::Unknown(other.to_string()),
        }
    }

    /// What can still be refunded; None when the provider reported no amounts.
    pub fn cancellable(&self) -> Option<Money> {
        (self.total >= 0 && self.cancelled >= 0).then(|| Money(self.total - self.cancelled))
    }
}

/// Confirms what the customer authorised. `provider_tx` is what the return URL carried (Toss: the paymentKey;
/// PortOne: the paymentId, which is the checkout id). Lookups and refunds afterwards take the checkout id.
pub struct Confirm<'a> {
    pub checkout_id: &'a str,
    pub provider_tx: &'a str,
    /// Our stored amount. Toss captures exactly this; the dev fake prices its synthetic payments with it.
    pub expected: Money,
    pub currency: &'a str,
}

pub struct RefundReq {
    pub provider_tx: String,
    pub amount: Money,
    /// What our books say is still refundable. With it the gateway refuses when its balance differs (a duplicate or
    /// a stale retry), and a balance that already shows this refund counts as done. None refunds unchecked.
    pub expected_cancellable: Option<Money>,
}

pub struct RefundInfo {
    /// The provider's refund id, when it has one (Toss and PortOne: none kept).
    pub refund_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintKind {
    /// The payment may have settled or issued a virtual account: re-read and settle.
    Payment,
    /// The payment was (partly) cancelled at the provider: re-read and record.
    Cancelled { partial: bool },
}

/// What a webhook says to re-read. Never proof of payment (D8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebhookHint {
    pub provider_tx: String,
    /// The `webhook_events` ledger key. None for an unsigned hint, which never enters the ledger: anyone could post
    /// one and pre-record a genuine body.
    pub event_id: Option<String>,
    pub kind: HintKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebhookError {
    /// No webhook secret and not dev mode: refuse everything.
    NotConfigured,
    Malformed,
    Stale,
    BadSignature,
}

/// One saved-card charge. `payment_id` is unique per charge and doubles as the idempotency key.
pub struct Charge<'a> {
    /// The billing key as stored (sealed); the gateway opens it.
    pub sealed_key: &'a str,
    /// Toss's customerKey the key is bound to.
    pub customer_key: Option<&'a str>,
    pub amount: Money,
    pub currency: &'a str,
    pub payment_id: &'a str,
    pub order_name: &'a str,
    pub customer: Uuid,
}

/// Who a card is saved for: our customer id and the one-time issue ticket (Toss's customerKey).
pub struct CustomerRef {
    pub id: String,
    pub issue: String,
}

pub trait PaymentGateway: Send + Sync {
    fn methods(&self) -> &[MethodId];
    fn start(&self, req: CheckoutReq) -> Fut<'_, CheckoutStart>;
    fn confirm<'a>(&'a self, c: Confirm<'a>) -> Fut<'a, PaymentInfo>;
    /// The source of truth after any webhook. `expected` is our row's amount: real gateways ignore it, the dev fake
    /// prices its synthetic payments with it.
    fn fetch<'a>(&'a self, provider_tx: &'a str, expected: Money) -> Fut<'a, PaymentInfo>;
    fn refund(&self, req: RefundReq) -> Fut<'_, RefundInfo>;
    /// No default: every gateway states how its deliveries are trusted.
    fn verify_webhook(&self, h: &HeaderMap, raw: &[u8], now: DateTime<Utc>) -> Result<Vec<WebhookHint>, WebhookError>;
    fn saved_cards(&self) -> Option<&dyn SavedCards> {
        None
    }
}

pub trait SavedCards: Send + Sync {
    /// Exchanges the card window's result for a billing key bound to `customer`. Returns the key in the clear: the
    /// caller seals it (`crypto::seal_billing_key`) before it is stored.
    fn issue<'a>(&'a self, customer: &'a CustomerRef, auth: &'a str) -> Fut<'a, String>;
    /// Charges once. Returns the id lookups and refunds take. Anything that may have charged is `Upstream`.
    fn charge<'a>(&'a self, c: Charge<'a>) -> Fut<'a, String>;
    /// Stops the card at the gateway. Takes the opened key.
    fn delete<'a>(&'a self, key: &'a str) -> Fut<'a, ()>;
}

/// A finished future, for gateway calls answered without I/O.
pub(crate) fn ready<'a, T: Send + 'a>(r: R<T>) -> Fut<'a, T> {
    Box::pin(std::future::ready(r))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info(status: &str, total: i64, cancelled: i64, method: Value) -> PaymentInfo {
        PaymentInfo {
            status: status.into(),
            total,
            cancelled,
            currency: "KRW".into(),
            method,
        }
    }

    #[test]
    fn every_provider_status_maps() {
        let none = || json!({});
        assert_eq!(info("READY", 1, 0, none()).pay_status(), PayStatus::Pending);
        assert_eq!(info("PAY_PENDING", 1, 0, none()).pay_status(), PayStatus::Pending);
        assert_eq!(info("PAID", 1, 0, none()).pay_status(), PayStatus::Paid);
        assert_eq!(info("PARTIAL_CANCELLED", 2, 1, none()).pay_status(), PayStatus::PartiallyRefunded);
        assert_eq!(info("CANCELLED", 2, 2, none()).pay_status(), PayStatus::Refunded);
        assert_eq!(info("CANCELLED", 2, 0, none()).pay_status(), PayStatus::Cancelled);
        assert_eq!(info("FAILED", 1, 0, none()).pay_status(), PayStatus::Failed);
        assert_eq!(info("MYSTERY", 1, 0, none()).pay_status(), PayStatus::Unknown("MYSTERY".into()));
        let va = info("VIRTUAL_ACCOUNT_ISSUED", 1, 0, json!({ "expiredAt": "2026-11-30T23:59:59+09:00" })); // tz-ok: PortOne wire fixture
        let due = DateTime::parse_from_rfc3339("2026-11-30T14:59:59Z").map(|t| t.with_timezone(&Utc)).ok();
        assert_eq!(va.pay_status(), PayStatus::PendingOffline { expires_at: due });
    }

    #[test]
    fn status_mapping_has_no_catch_all() {
        let src = include_str!("payments.rs");
        let catch_all = concat!("_ ", "=>");
        assert!(!src.contains(catch_all), "map every provider status by name");
    }

    #[test]
    fn cancellable_needs_both_amounts() {
        assert_eq!(info("PAID", 5000, 1000, json!({})).cancellable(), Some(Money(4000)));
        assert_eq!(info("PAID", 5000, -1, json!({})).cancellable(), None);
        assert_eq!(info("PAID", -1, 0, json!({})).cancellable(), None);
    }

    #[test]
    fn kr_methods_are_the_methods_customers_can_pick() {
        let codes: Vec<&str> = crate::routes::payments::METHODS.iter().map(|m| m.0).collect();
        assert_eq!(codes, KR_METHODS);
    }
}
