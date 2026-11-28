// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! In-memory payment gateways that behave like PortOne and like Toss Payments: confirm, lookups, refunds with the
//! cancellable-balance rule, saved cards and signed webhooks. They hold no keys and move no money, so they refuse
//! to exist outside dev mode (§8.4). The contract suite (`tests/contract_payments.rs`) runs against them and against
//! the real shims, so a fresh-country flow can be tested end to end before a real gateway ships.

use super::payments::{
    Charge, CheckoutReq, CheckoutStart, Confirm, CustomerRef, HintKind, KR_METHODS, MethodId, PaymentGateway, RefundInfo, RefundReq, SavedCards, WebhookError,
    WebhookHint, ready,
};
use super::{Fut, R};
use crate::providers::{PaymentInfo, ProviderError, portone::BALANCE_MOVED};
use chrono::{DateTime, Utc};
use domain::money::Money;
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};
use subtle::ConstantTimeEq;

/// Header the fake signs deliveries with: `sha256(secret + body)` in hex.
pub const SIGNATURE_HEADER: &str = "fake-signature";

/// Whose confirm the fake copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavour {
    /// The payment exists once the customer paid in the window; confirming is reading it.
    PortOne,
    /// The window only authorises; confirm captures with our amount.
    Toss,
}

struct Payment {
    total: i64,
    cancelled: i64,
    currency: String,
}

#[derive(Default)]
struct State {
    /// Payments by our id (checkout id or charge id).
    payments: HashMap<String, Payment>,
    /// Authorisations not yet confirmed (Toss): provider_tx -> amount.
    authorised: HashMap<String, i64>,
    /// Billing key -> the customer/ticket it is bound to.
    cards: HashMap<String, String>,
}

pub struct FakeGateway {
    flavour: Flavour,
    data_key: [u8; 32],
    secret: String,
    state: Mutex<State>,
}

impl FakeGateway {
    /// Refused unless `dev_mode` (`Config::dev_mode`): a fake must never take a live payment. `data_key` opens
    /// sealed billing keys, as `Config::data_key` does for the real gateways.
    pub fn new(flavour: Flavour, dev_mode: bool, data_key: [u8; 32], secret: &str) -> R<Self> {
        if !dev_mode {
            return Err(ProviderError::NotConfigured("fake_gateway"));
        }
        Ok(Self {
            flavour,
            data_key,
            secret: secret.to_string(),
            state: Mutex::default(),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // A panic while holding the lock leaves whole entries only.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The customer pays `amount` for `checkout_id` in the window. Returns what the return URL carries.
    pub fn customer_pays(&self, checkout_id: &str, amount: i64, currency: &str) -> String {
        let mut s = self.state();
        match self.flavour {
            Flavour::PortOne => {
                s.payments.insert(checkout_id.to_string(), paid(amount, currency));
                checkout_id.to_string()
            }
            Flavour::Toss => {
                let key = format!("pk_{checkout_id}");
                s.authorised.insert(key.clone(), amount);
                key
            }
        }
    }

    /// The card window's result for a card bound to `customer`.
    pub fn card_window(&self, customer: &CustomerRef) -> String {
        let auth = format!("auth_{}", customer.issue);
        self.state().cards.insert(format!("fk_{auth}"), bound_to(customer, self.flavour));
        auth
    }

    /// The headers and body of a delivery about `provider_tx`, signed with the gateway's secret (or `forge`d).
    pub fn delivery(&self, provider_tx: &str, kind: HintKind, forge: bool) -> (HeaderMap, Vec<u8>) {
        let body = json!({ "tx": provider_tx, "kind": kind_name(kind) }).to_string().into_bytes();
        let secret = if forge { "forged" } else { self.secret.as_str() };
        let mut h = HeaderMap::new();
        if let Ok(v) = sign(secret, &body).parse() {
            h.insert(SIGNATURE_HEADER, v);
        }
        (h, body)
    }

    fn info(p: &Payment) -> PaymentInfo {
        let status = match (p.cancelled, p.total - p.cancelled) {
            (0, _) => "PAID",
            (_, 0) => "CANCELLED",
            (_, _) => "PARTIAL_CANCELLED",
        };
        PaymentInfo {
            status: status.into(),
            total: p.total,
            cancelled: p.cancelled,
            currency: p.currency.clone(),
            method: json!({ "type": "PaymentMethodCard" }),
        }
    }

    fn lookup(&self, id: &str) -> R<PaymentInfo> {
        self.state().payments.get(id).map(Self::info).ok_or(ProviderError::NotFound)
    }

    fn capture(&self, c: &Confirm<'_>) -> R<PaymentInfo> {
        let mut s = self.state();
        if let Some(p) = s.payments.get(c.checkout_id) {
            return Ok(Self::info(p));
        }
        let authorised = s
            .authorised
            .remove(c.provider_tx)
            .ok_or(ProviderError::Declined("NOT_FOUND_PAYMENT_SESSION".into()))?;
        if authorised != c.expected.0 {
            s.authorised.insert(c.provider_tx.to_string(), authorised);
            return Err(ProviderError::Declined("amount_mismatch".into()));
        }
        let p = paid(authorised, c.currency);
        let info = Self::info(&p);
        s.payments.insert(c.checkout_id.to_string(), p);
        Ok(info)
    }

    /// The real gateways' rule: with an expected balance, a balance that already shows this refund is our earlier
    /// attempt (done); any other difference is refused.
    fn cancel(&self, r: &RefundReq) -> R<RefundInfo> {
        let mut s = self.state();
        let p = s.payments.get_mut(&r.provider_tx).ok_or(ProviderError::NotFound)?;
        let left = p.total - p.cancelled;
        if let Some(c) = r.expected_cancellable {
            if left == c.0 - r.amount.0 {
                return Ok(RefundInfo { refund_id: None });
            }
            if left != c.0 {
                return Err(ProviderError::Declined(BALANCE_MOVED.into()));
            }
        }
        if r.amount.0 <= 0 || r.amount.0 > left {
            return Err(ProviderError::Declined("cancel_amount_exceeds_cancellable".into()));
        }
        p.cancelled += r.amount.0;
        Ok(RefundInfo { refund_id: None })
    }

    fn charge_now(&self, c: &Charge<'_>) -> R<String> {
        let key = crate::crypto::open_billing_key(&self.data_key, c.sealed_key).ok_or(ProviderError::Declined("billing_key_unreadable".into()))?;
        let mut s = self.state();
        let owner = s.cards.get(&key).ok_or(ProviderError::Declined("billing_key_not_found".into()))?;
        if self.flavour == Flavour::Toss && Some(owner.as_str()) != c.customer_key {
            return Err(ProviderError::Declined("billing_key_not_bound".into()));
        }
        if s.payments.contains_key(c.payment_id) {
            return Err(ProviderError::Upstream("charge_already_paid".into()));
        }
        s.payments.insert(c.payment_id.to_string(), paid(c.amount.0, c.currency));
        Ok(c.payment_id.to_string())
    }
}

fn paid(total: i64, currency: &str) -> Payment {
    Payment {
        total,
        cancelled: 0,
        currency: currency.to_string(),
    }
}

/// PortOne binds a key to the customer id, Toss to the customerKey (the ticket).
fn bound_to(c: &CustomerRef, flavour: Flavour) -> String {
    match flavour {
        Flavour::PortOne => c.id.clone(),
        Flavour::Toss => c.issue.clone(),
    }
}

fn sign(secret: &str, body: &[u8]) -> String {
    let mut msg = secret.as_bytes().to_vec();
    msg.extend_from_slice(body);
    crate::auth::sha256_hex(&msg)
}

fn kind_name(kind: HintKind) -> &'static str {
    match kind {
        HintKind::Payment => "payment",
        HintKind::Cancelled { partial: false } => "cancelled",
        HintKind::Cancelled { partial: true } => "partially_cancelled",
    }
}

fn parse_kind(name: &str) -> Option<HintKind> {
    [HintKind::Payment, HintKind::Cancelled { partial: false }, HintKind::Cancelled { partial: true }]
        .into_iter()
        .find(|k| kind_name(*k) == name)
}

impl PaymentGateway for FakeGateway {
    fn methods(&self) -> &[MethodId] {
        &KR_METHODS
    }

    fn start(&self, req: CheckoutReq) -> Fut<'_, CheckoutStart> {
        ready(Ok(CheckoutStart::Sdk {
            params: json!({ "fake": true, "checkout_id": req.checkout_id, "amount": req.amount, "currency": req.currency }),
        }))
    }

    fn confirm<'a>(&'a self, c: Confirm<'a>) -> Fut<'a, PaymentInfo> {
        ready(match self.flavour {
            Flavour::PortOne => self.lookup(c.provider_tx),
            Flavour::Toss => self.capture(&c),
        })
    }

    fn fetch<'a>(&'a self, provider_tx: &'a str, _expected: Money) -> Fut<'a, PaymentInfo> {
        ready(self.lookup(provider_tx))
    }

    fn refund(&self, req: RefundReq) -> Fut<'_, RefundInfo> {
        ready(self.cancel(&req))
    }

    fn verify_webhook(&self, h: &HeaderMap, raw: &[u8], _now: DateTime<Utc>) -> Result<Vec<WebhookHint>, WebhookError> {
        let got = h.get(SIGNATURE_HEADER).and_then(|v| v.to_str().ok()).ok_or(WebhookError::Malformed)?;
        if !bool::from(got.as_bytes().ct_eq(sign(&self.secret, raw).as_bytes())) {
            return Err(WebhookError::BadSignature);
        }
        let v: Value = serde_json::from_slice(raw).map_err(|_| WebhookError::Malformed)?;
        let (Some(tx), Some(kind)) = (v["tx"].as_str(), v["kind"].as_str().and_then(parse_kind)) else {
            return Ok(Vec::new());
        };
        Ok(vec![WebhookHint {
            provider_tx: tx.to_string(),
            event_id: Some(format!("sha256:{}", crate::auth::sha256_hex(raw))),
            kind,
        }])
    }

    fn saved_cards(&self) -> Option<&dyn SavedCards> {
        Some(self)
    }
}

impl SavedCards for FakeGateway {
    fn issue<'a>(&'a self, customer: &'a CustomerRef, auth: &'a str) -> Fut<'a, String> {
        let key = format!("fk_{auth}");
        let bound = self.state().cards.get(&key).is_some_and(|c| *c == bound_to(customer, self.flavour));
        ready(if bound {
            Ok(key)
        } else {
            Err(ProviderError::Declined("billing_key_not_bound".into()))
        })
    }

    fn charge<'a>(&'a self, c: Charge<'a>) -> Fut<'a, String> {
        ready(self.charge_now(&c))
    }

    fn delete<'a>(&'a self, key: &'a str) -> Fut<'a, ()> {
        self.state().cards.remove(key);
        ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fake_refuses_to_exist_outside_dev_mode() {
        assert!(FakeGateway::new(Flavour::Toss, false, [7; 32], "s").is_err());
        assert!(FakeGateway::new(Flavour::PortOne, true, [7; 32], "s").is_ok());
    }
}
