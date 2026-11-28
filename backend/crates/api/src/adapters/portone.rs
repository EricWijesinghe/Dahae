// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `PaymentGateway` over today's PortOne code. Every call goes through the same `Providers` method the routes use
//! (dev fakes, health records and all), so behaviour is unchanged. Also serves the legacy `pg` code, exactly as
//! `Providers` does for it.

use super::payments::{
    Charge, CheckoutReq, CheckoutStart, Confirm, CustomerRef, HintKind, KR_METHODS, MethodId, PaymentGateway, RefundInfo, RefundReq, SavedCards, WebhookError,
    WebhookHint, ready,
};
use super::{AnyAdapter, Fut};
use crate::providers::{Gateway, PaymentInfo, ProviderError, Providers, SavedCard, portone};
use crate::routes::{
    payments::channel_key,
    webhooks::{WebhookAuth, event_id, is_cancel_event, is_payment_event, webhook_auth},
};
use chrono::{DateTime, Utc};
use domain::money::Money;
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct PortOneGateway {
    p: Providers,
    /// The stored code calls are made for: `portone`, or the legacy `pg`.
    code: &'static str,
}

pub fn build(p: &Providers, code: &'static str) -> AnyAdapter {
    AnyAdapter::Pay(Arc::new(PortOneGateway { p: p.clone(), code }))
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> &'a str {
    h.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

/// The hints of one PortOne event: settlement events and cancellations naming a payment; anything else is ignored.
fn hints(event: &Value, event_id: String) -> Vec<WebhookHint> {
    let kind = event["type"].as_str().unwrap_or_default();
    let kind = if is_cancel_event(kind) {
        HintKind::Cancelled {
            partial: kind == "Transaction.PartialCancelled",
        }
    } else if is_payment_event(kind) {
        HintKind::Payment
    } else {
        return Vec::new();
    };
    event["data"]["paymentId"]
        .as_str()
        .map(|tx| WebhookHint {
            provider_tx: tx.to_string(),
            event_id: Some(event_id),
            kind,
        })
        .into_iter()
        .collect()
}

impl PaymentGateway for PortOneGateway {
    fn methods(&self) -> &[MethodId] {
        &KR_METHODS
    }

    /// The SDK parameters `POST /checkouts` returns: the store, the method's channel key and cash receipts. The dev
    /// fake takes no channel key.
    fn start(&self, req: CheckoutReq) -> Fut<'_, CheckoutStart> {
        let integ = self.p.integ();
        let key = channel_key(&integ.portone.channel_keys, &req.method);
        let key = if self.p.dev() {
            Ok(key.unwrap_or_default())
        } else {
            key.ok_or(ProviderError::NotConfigured("portone"))
        };
        ready(key.map(|key| CheckoutStart::Sdk {
            params: json!({
                "store_id": integ.portone.store_id,
                "channel_key": key,
                "cash_receipts": integ.portone.cash_receipts,
            }),
        }))
    }

    /// PortOne captures in its own window: confirming is reading the payment.
    fn confirm<'a>(&'a self, c: Confirm<'a>) -> Fut<'a, PaymentInfo> {
        self.fetch(c.provider_tx, c.expected)
    }

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

    /// Standard Webhooks with the configured secret. Without one only dev mode accepts unsigned events.
    fn verify_webhook(&self, h: &HeaderMap, raw: &[u8], now: DateTime<Utc>) -> Result<Vec<WebhookHint>, WebhookError> {
        let integ = self.p.integ();
        let secret = integ.portone.webhook_secret.as_deref();
        match (webhook_auth(self.p.dev(), secret), secret) {
            (WebhookAuth::Verify, Some(secret)) => portone::verify_webhook(
                secret,
                header(h, "webhook-id"),
                header(h, "webhook-timestamp"),
                header(h, "webhook-signature"),
                raw,
                now.timestamp(),
            )
            .map_err(|e| match e {
                portone::WebhookError::Malformed => WebhookError::Malformed,
                portone::WebhookError::Stale => WebhookError::Stale,
                portone::WebhookError::BadSignature => WebhookError::BadSignature,
            })?,
            (WebhookAuth::UnsignedDev, _) => {}
            (WebhookAuth::Verify | WebhookAuth::Unavailable, _) => return Err(WebhookError::NotConfigured),
        }
        let event: Value = serde_json::from_slice(raw).map_err(|_| WebhookError::Malformed)?;
        Ok(hints(&event, event_id(header(h, "webhook-id"), raw)))
    }

    fn saved_cards(&self) -> Option<&dyn SavedCards> {
        Some(self)
    }
}

impl SavedCards for PortOneGateway {
    /// The key the browser SDK issued, once it is `ISSUED` and bound to this customer and ticket.
    fn issue<'a>(&'a self, customer: &'a CustomerRef, auth: &'a str) -> Fut<'a, String> {
        Box::pin(self.p.issue_billing_key(Gateway::Portone, auth, &customer.id, &customer.issue))
    }

    fn charge<'a>(&'a self, c: Charge<'a>) -> Fut<'a, String> {
        Box::pin(async move {
            let card = SavedCard {
                gateway: Gateway::Portone,
                sealed_key: c.sealed_key.to_string(),
                customer_key: c.customer_key.map(str::to_string),
            };
            self.p
                .charge_billing(&card, c.amount.0, c.currency, c.payment_id, c.order_name, c.customer)
                .await
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> Fut<'a, ()> {
        Box::pin(self.p.delete_billing_key(Gateway::Portone, key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_become_hints_by_type() {
        let ev = |kind: &str| json!({ "type": kind, "data": { "paymentId": "pay-1" } });
        let one = |kind| hints(&ev(kind), "msg-1".into());
        assert_eq!(one("Transaction.Paid")[0].kind, HintKind::Payment);
        assert_eq!(one("Transaction.VirtualAccountIssued")[0].kind, HintKind::Payment);
        assert_eq!(one("Transaction.Cancelled")[0].kind, HintKind::Cancelled { partial: false });
        assert_eq!(one("Transaction.PartialCancelled")[0].kind, HintKind::Cancelled { partial: true });
        assert_eq!(one("Transaction.Paid")[0].event_id.as_deref(), Some("msg-1"));
        assert!(one("Transaction.Ready").is_empty(), "unknown events are ignored");
        assert!(hints(&json!({ "type": "Transaction.Paid", "data": {} }), String::new()).is_empty());
    }
}
