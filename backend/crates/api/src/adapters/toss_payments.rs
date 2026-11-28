// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `PaymentGateway` over today's Toss Payments code, for both of its stored codes: `tosspayments` (window MID) and
//! `tosspayments_billing` (자동결제 MID, saved-card charges). Every call goes through the same `Providers` method the
//! routes use, so behaviour is unchanged.

use super::payments::{
    Charge, CheckoutReq, CheckoutStart, Confirm, CustomerRef, HintKind, KR_METHODS, MethodId, PaymentGateway, RefundInfo, RefundReq, SavedCards, WebhookError,
    WebhookHint, ready,
};
use super::{AnyAdapter, Fut};
use crate::providers::{Gateway, PaymentInfo, ProviderError, Providers, SavedCard, TOSS, toss_webhook};
use crate::routes::webhooks::{TossGate, event_id, toss_event, toss_gate};
use chrono::{DateTime, Utc};
use domain::money::Money;
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct TossGateway {
    p: Providers,
    /// `tosspayments` or `tosspayments_billing`: which MID lookups and refunds use.
    code: &'static str,
}

pub fn build(p: &Providers, code: &'static str) -> AnyAdapter {
    AnyAdapter::Pay(Arc::new(TossGateway { p: p.clone(), code }))
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> &'a str {
    h.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

/// The hint of one Toss payment event (`routes::webhooks::toss_event`). Signed events carry a ledger key.
fn hints(event: &Value, event_id: Option<String>) -> Vec<WebhookHint> {
    toss_event(event)
        .map(|(order, cancel)| WebhookHint {
            provider_tx: order,
            event_id,
            kind: match cancel {
                Some(kind) => HintKind::Cancelled {
                    partial: kind == "Transaction.PartialCancelled",
                },
                None => HintKind::Payment,
            },
        })
        .into_iter()
        .collect()
}

impl PaymentGateway for TossGateway {
    fn methods(&self) -> &[MethodId] {
        &KR_METHODS
    }

    /// Toss starts every method from its one client key; it issues cash receipts in its own window.
    fn start(&self, _req: CheckoutReq) -> Fut<'_, CheckoutStart> {
        let integ = self.p.integ();
        let key = integ.toss_payments.as_ref().map(|t| t.client_key.clone());
        ready(key.ok_or(ProviderError::NotConfigured(TOSS)).map(|key| CheckoutStart::Sdk {
            params: json!({ "store_id": null, "channel_key": key, "cash_receipts": false }),
        }))
    }

    /// `POST /v1/payments/confirm` with our stored amount: `provider_tx` is the paymentKey, the orderId our checkout id.
    fn confirm<'a>(&'a self, c: Confirm<'a>) -> Fut<'a, PaymentInfo> {
        Box::pin(self.p.confirm_toss(c.provider_tx, c.checkout_id, c.expected.0))
    }

    /// Looked up by orderId (our checkout or charge id) on this code's MID.
    fn fetch<'a>(&'a self, provider_tx: &'a str, expected: Money) -> Fut<'a, PaymentInfo> {
        Box::pin(self.p.payment(self.code, provider_tx, expected.0))
    }

    fn refund(&self, req: RefundReq) -> Fut<'_, RefundInfo> {
        Box::pin(async move {
            match req.expected_cancellable {
                Some(c) => self.p.refund_checked(self.code, &req.provider_tx, req.amount.0, c.0).await,
                None => self.p.refund(self.code, &req.provider_tx, req.amount.0).await,
            }
            .map(|()| RefundInfo { refund_id: None })
        })
    }

    /// Toss does not promise a signature on payment events: with the 보안 키 set and the header present it must
    /// verify; otherwise the body is a hint only (no ledger key). Without a Toss card only dev mode accepts events.
    fn verify_webhook(&self, h: &HeaderMap, raw: &[u8], _now: DateTime<Utc>) -> Result<Vec<WebhookHint>, WebhookError> {
        let integ = self.p.integ();
        let key = integ.toss_payments.as_ref().and_then(|t| t.security_key.as_deref());
        let signature = header(h, toss_webhook::SIGNATURE_HEADER);
        let gate = toss_gate(self.p.dev(), integ.toss_payments.is_some(), key.is_some(), !signature.is_empty());
        let signed = match gate {
            TossGate::Verify => {
                let time = header(h, toss_webhook::TRANSMISSION_TIME_HEADER);
                if !toss_webhook::verify(key.unwrap_or_default(), time, signature, raw) {
                    return Err(WebhookError::BadSignature);
                }
                true
            }
            TossGate::Hint => false,
            TossGate::Unavailable => return Err(WebhookError::NotConfigured),
        };
        let event: Value = serde_json::from_slice(raw).map_err(|_| WebhookError::Malformed)?;
        Ok(hints(&event, signed.then(|| event_id("", raw))))
    }

    fn saved_cards(&self) -> Option<&dyn SavedCards> {
        Some(self)
    }
}

impl SavedCards for TossGateway {
    /// The authKey becomes a billingKey bound to the customerKey (`customer.issue`).
    fn issue<'a>(&'a self, customer: &'a CustomerRef, auth: &'a str) -> Fut<'a, String> {
        Box::pin(self.p.issue_billing_key(Gateway::TossPayments, auth, &customer.id, &customer.issue))
    }

    fn charge<'a>(&'a self, c: Charge<'a>) -> Fut<'a, String> {
        Box::pin(async move {
            let card = SavedCard {
                gateway: Gateway::TossPayments,
                sealed_key: c.sealed_key.to_string(),
                customer_key: c.customer_key.map(str::to_string),
            };
            self.p
                .charge_billing(&card, c.amount.0, c.currency, c.payment_id, c.order_name, c.customer)
                .await
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> Fut<'a, ()> {
        Box::pin(self.p.delete_billing_key(Gateway::TossPayments, key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_become_hints_by_status() {
        let ev = |status: &str| json!({ "eventType": "PAYMENT_STATUS_CHANGED", "data": { "orderId": "ord-1", "status": status } });
        let one = |status| hints(&ev(status), None);
        assert_eq!(one("DONE")[0].kind, HintKind::Payment);
        assert_eq!(one("CANCELED")[0].kind, HintKind::Cancelled { partial: false });
        assert_eq!(one("PARTIAL_CANCELED")[0].kind, HintKind::Cancelled { partial: true });
        assert_eq!(one("DONE")[0].provider_tx, "ord-1");
        assert!(
            hints(&json!({ "eventType": "PAYOUT_STATUS_CHANGED" }), None).is_empty(),
            "other events are ignored"
        );
        let deposit = hints(&json!({ "orderId": "ord-2", "status": "DONE" }), Some("sha256:x".into()));
        assert_eq!(deposit[0].event_id.as_deref(), Some("sha256:x"), "DEPOSIT_CALLBACK has no eventType");
    }
}
