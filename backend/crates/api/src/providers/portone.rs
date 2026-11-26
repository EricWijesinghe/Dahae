// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! PortOne V2 adapter: payments, identity verification, cancellations and
//! Standard Webhooks signatures. Dev fakes only answer `dev:` ids.

use super::{IdentityInfo, PaymentInfo, ProviderError, Providers};
use crate::config::PortOne;

/// The refusal code for a cancel whose balance moved since our books were written. Retrying cannot succeed.
pub const BALANCE_MOVED: &str = "cancellable_amount_changed";
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub(crate) const API: &str = "https://api.portone.io";
const WEBHOOK_TOLERANCE_SECS: i64 = 300;

fn auth(p: &Providers) -> Result<String, ProviderError> {
    let integ = p.integ();
    let secret = integ.portone.api_secret.as_deref().ok_or(ProviderError::NotConfigured("portone"))?;
    Ok(format!("PortOne {secret}"))
}

/// Setup-wizard check: looks up a payment that cannot exist. 404 means the secret was accepted.
pub async fn check(http: &reqwest::Client, secret: &str) -> Result<StatusCode, ProviderError> {
    http.get(format!("{API}/payments/malkkeum-setup-check"))
        .header(AUTHORIZATION, format!("PortOne {secret}"))
        .send()
        .await
        .map(|r| r.status())
        .map_err(upstream)
}

/// Ids are put into URL paths: only plain tokens, so no traversal or query injection.
pub(crate) fn check_id(id: &str) -> Result<(), ProviderError> {
    let ok = !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok { Ok(()) } else { Err(ProviderError::NotFound) }
}

fn upstream(e: reqwest::Error) -> ProviderError {
    ProviderError::Upstream(e.to_string())
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

async fn read_json(resp: reqwest::Response) -> Result<Value, ProviderError> {
    let status = resp.status();
    if status == StatusCode::NOT_FOUND {
        return Err(ProviderError::NotFound);
    }
    // UnauthorizedError: the API secret is wrong or missing (PG secrets are rejected here too). A config error.
    if status == StatusCode::UNAUTHORIZED {
        return Err(ProviderError::NotConfigured("portone"));
    }
    if !status.is_success() {
        return Err(ProviderError::Upstream(format!("status {status}")));
    }
    resp.json().await.map_err(upstream)
}

async fn get_json(p: &Providers, path: &str) -> Result<Value, ProviderError> {
    let resp = p
        .http()
        .get(format!("{}{path}", p.portone_api()))
        .header(AUTHORIZATION, auth(p)?)
        .send()
        .await
        .map_err(upstream)?;
    read_json(resp).await
}

pub async fn payment(p: &Providers, id: &str) -> Result<PaymentInfo, ProviderError> {
    check_id(id)?;
    let v = get_json(p, &format!("/payments/{id}")).await?;
    if test_channel(&v["channel"]) {
        p.observe("payment_test_channel", Err("portone_test_channel")).await;
    }
    Ok(PaymentInfo {
        // Read as failed so nothing books: no money moved.
        status: if foreign_test_channel(p, &v["channel"]) {
            "FAILED".into()
        } else {
            text(&v["status"])
        },
        // Missing amount reads as -1, which never matches a checkout: the payment gets refunded.
        total: v["amount"]["total"].as_i64().unwrap_or(-1),
        cancelled: v["amount"]["cancelled"].as_i64().unwrap_or(-1),
        currency: text(&v["currency"]),
        method: v["method"].clone(),
    })
}

/// One payment from the daily list: what the reconciliation diff needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPayment {
    pub id: String,
    pub status: String,
    pub total: i64,
}

fn listed(item: &Value) -> Result<ProviderPayment, ProviderError> {
    let id = item["id"].as_str().ok_or_else(|| ProviderError::Upstream("payment without id".into()))?;
    let total = item["amount"]["total"]
        .as_i64()
        .ok_or_else(|| ProviderError::Upstream("payment without amount".into()))?;
    Ok(ProviderPayment {
        id: id.to_string(),
        status: text(&item["status"]),
        total,
    })
}

/// One page of the store's payments created in `[from, until)` (RFC 3339), with the total count.
/// A missing or malformed field fails the page rather than dropping that payment from the diff.
pub async fn list_payments(p: &Providers, from: &str, until: &str, page: u32, size: u32) -> Result<(Vec<ProviderPayment>, i64), ProviderError> {
    let integ = p.integ();
    let store = integ.portone.store_id.as_deref().ok_or(ProviderError::NotConfigured("portone"))?;
    let (page, size) = (page.to_string(), size.to_string());
    let resp = p
        .http()
        .get(format!("{}/payments", p.portone_api()))
        .query(&[
            ("storeId", store),
            ("from", from),
            ("until", until),
            ("pageNumber", page.as_str()),
            ("pageSize", size.as_str()),
        ])
        .header(AUTHORIZATION, auth(p)?)
        .send()
        .await
        .map_err(upstream)?;
    let body = read_json(resp).await?;
    let items = body["items"]
        .as_array()
        .ok_or_else(|| ProviderError::Upstream("payment list without items".into()))?;
    let total = body["page"]["totalCount"]
        .as_i64()
        .ok_or_else(|| ProviderError::Upstream("payment list without totalCount".into()))?;
    Ok((items.iter().map(listed).collect::<Result<_, _>>()?, total))
}

/// Full or partial cancellation of a PAID payment (used for refunds). With `cancellable`, PortOne only
/// cancels while the payment's cancellable balance equals it (`currentCancellableAmount`).
/// PortOne V2 has no idempotency key on cancel, so this balance check is what makes a cancel happen once.
/// Source: https://developers.portone.io/api/rest-v2/payment (CancelPaymentBody, 409 CancellableAmountConsistencyBrokenError).
pub async fn cancel(p: &Providers, id: &str, amount: i64, cancellable: Option<i64>) -> Result<(), ProviderError> {
    check_id(id)?;
    let mut body = json!({ "amount": amount, "reason": "booking refund" });
    if let Some(c) = cancellable {
        body["currentCancellableAmount"] = json!(c);
    }
    let resp = p
        .http()
        .post(format!("{}/payments/{id}/cancel", p.portone_api()))
        .header(AUTHORIZATION, auth(p)?)
        .json(&body)
        .send()
        .await
        .map_err(upstream)?;
    if resp.status() != StatusCode::CONFLICT {
        return read_json(resp).await.map(|_| ());
    }
    let refusal: Value = resp.json().await.unwrap_or(Value::Null);
    match (refusal["type"].as_str(), cancellable) {
        (Some("CancellableAmountConsistencyBrokenError"), Some(before)) => landed_before(p, id, before - amount).await,
        (code, _) => Err(ProviderError::Declined(code.unwrap_or("cancel_refused").to_string())),
    }
}

/// The balance moved since our books were written. It is our own earlier attempt (its commit was lost)
/// exactly when the balance left is what this cancel would leave; anything else is refused.
async fn landed_before(p: &Providers, id: &str, expected_left: i64) -> Result<(), ProviderError> {
    let v = get_json(p, &format!("/payments/{id}")).await?;
    let left = v["amount"]["total"].as_i64().zip(v["amount"]["cancelled"].as_i64()).map(|(t, c)| t - c);
    if left == Some(expected_left) {
        tracing::warn!(payment = id, "cancel already applied at PortOne; recording it");
        Ok(())
    } else {
        Err(ProviderError::Declined(BALANCE_MOVED.into()))
    }
}

pub async fn identity(p: &Providers, id: &str) -> Result<IdentityInfo, ProviderError> {
    check_id(id)?;
    let v = get_json(p, &format!("/identity-verifications/{id}")).await?;
    let customer = &v["verifiedCustomer"];
    Ok(IdentityInfo {
        id: text(&v["id"]),
        status: text(&v["status"]),
        ci: customer["ci"].as_str().map(str::to_string),
        name: customer["name"].as_str().map(str::to_string),
    })
}

/// The checkout id a payment was created with, from its `customData` (object, or JSON text).
/// Dev mode has no provider to ask, so it answers `None`.
pub async fn custom_checkout_id(p: &Providers, id: &str) -> Result<Option<Uuid>, ProviderError> {
    if p.cfg().dev_mode {
        return Ok(None);
    }
    check_id(id)?;
    match get_json(p, &format!("/payments/{id}")).await {
        Ok(v) => Ok(custom_data_checkout_id(&v["customData"])),
        Err(ProviderError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

fn custom_data_checkout_id(custom: &Value) -> Option<Uuid> {
    let parsed_text: Value;
    let obj: &Value = match custom.as_str() {
        Some(raw) => {
            parsed_text = serde_json::from_str(raw).ok()?;
            &parsed_text
        }
        None => custom,
    };
    Uuid::parse_str(obj["checkoutId"].as_str()?).ok()
}

/// The channel saved cards bill through. Installs saved before the `card_billing` slot existed billed through
/// `card`, so that key stands in while the slot is empty (the alias is read here, nothing is rewritten).
pub fn billing_channel(p: &PortOne) -> Option<&String> {
    p.channel_keys.get("card_billing").or_else(|| p.channel_keys.get("card"))
}

/// Billing (saved card) needs the API secret, the store and a billing channel key.
pub fn billing_ready(p: &PortOne) -> bool {
    p.api_secret.is_some() && p.store_id.is_some() && billing_channel(p).is_some()
}

/// Verifies a billing key the browser SDK issued before we store it. The key must be `ISSUED` and bound to
/// this customer and this issue ticket (see [`bound`]). The card details are not kept: we store the key only.
pub async fn verify_billing_key(p: &Providers, key: &str, customer: &str, issue: &str) -> Result<(), ProviderError> {
    let integ = p.integ();
    let secret = integ.portone.api_secret.as_deref().ok_or(ProviderError::NotConfigured("portone"))?;
    if check_id(key).is_err() {
        return Err(ProviderError::Declined("invalid_billing_key".into()));
    }
    let info = billing_key_at(p.http(), p.portone_api(), secret, key).await?;
    let channels = info["channels"].as_array().map(Vec::as_slice).unwrap_or_default();
    if channels.iter().any(test_channel) {
        p.observe("payment_test_channel", Err("portone_test_channel")).await;
    }
    if channels.iter().any(|c| foreign_test_channel(p, c)) {
        return Err(ProviderError::Declined("portone_test_channel".into()));
    }
    bound(&info, customer, issue)
}

/// A PortOne test channel (`channel.type` TEST in the V2 API): the payment or card moves no real money, so a live
/// server alerts the owner.
pub fn test_channel(channel: &Value) -> bool {
    channel["type"].as_str() == Some("TEST")
}

/// A test channel whose key this server does not hold. Channel keys are public, so any client can pay through
/// a test channel of the same store: such a payment or card must never count.
fn foreign_test_channel(p: &Providers, channel: &Value) -> bool {
    test_channel(channel) && !channel["key"].as_str().is_some_and(|k| p.integ().portone.channel_keys.values().any(|c| c == k))
}

/// Binds a key to the customer and ticket it was issued for. Missing fields fail closed.
/// Source: PortOne openapi IssuedBillingKeyInfo (`customer.id`, `issueId`).
/// shortcut: the browser SDK's `customer.customerId` must equal `customer.id`. Confirm once in a test store.
pub fn bound(info: &Value, customer: &str, issue: &str) -> Result<(), ProviderError> {
    issued(info)?;
    if info["customer"]["id"].as_str() == Some(customer) && info["issueId"].as_str() == Some(issue) {
        Ok(())
    } else {
        Err(ProviderError::Declined("billing_key_not_bound".into()))
    }
}

/// `GET {base}/billing-keys/{key}` with the API secret. `base` is `API` in production, a fake in tests.
/// Source: https://developers.portone.io/api/rest-v2/payment.billingKey (BillingKeyStatus: ISSUED | DELETED).
pub async fn billing_key_at(http: &reqwest::Client, base: &str, secret: &str, key: &str) -> Result<Value, ProviderError> {
    let resp = http
        .get(format!("{base}/billing-keys/{key}"))
        .header(AUTHORIZATION, format!("PortOne {secret}"))
        .send()
        .await
        .map_err(upstream)?;
    read_json(resp).await
}

/// `DELETE /billing-keys/{key}`: stops the saved card at the gateway.
pub async fn delete_billing_key(p: &Providers, key: &str) -> Result<(), ProviderError> {
    check_id(key)?;
    delete_at(p.http(), p.portone_api(), &auth(p)?, key).await
}

/// `delete_billing_key` against an explicit base and auth header. 409 BillingKeyAlreadyDeletedError is done (Ok);
/// any other 409 (e.g. PaymentScheduleAlreadyExistsError, key still live) is an error so account deletion fails closed.
/// 404 (BillingKeyNotFoundError / NotIssuedError) is Ok too, but logged: the key was unknown to PortOne.
/// Source: PortOne openapi, DELETE /billing-keys/{billingKey} responses (DeleteBillingKeyError).
pub async fn delete_at(http: &reqwest::Client, base: &str, auth_header: &str, key: &str) -> Result<(), ProviderError> {
    let resp = http
        .delete(format!("{base}/billing-keys/{key}"))
        .header(AUTHORIZATION, auth_header)
        .send()
        .await
        .map_err(upstream)?;
    match resp.status() {
        StatusCode::CONFLICT => {
            let body: Value = resp.json().await.map_err(upstream)?;
            match body["type"].as_str() {
                Some("BillingKeyAlreadyDeletedError") => Ok(()),
                other => Err(ProviderError::Upstream(format!("billing key not deleted: {}", other.unwrap_or("unknown")))),
            }
        }
        StatusCode::NOT_FOUND => {
            tracing::warn!("billing key not found at PortOne on delete; treating as deleted");
            Ok(())
        }
        StatusCode::UNAUTHORIZED => Err(ProviderError::NotConfigured("portone")),
        s if s.is_success() => Ok(()),
        s => Err(ProviderError::Upstream(format!("status {s}"))),
    }
}

/// `POST /payments/{paymentId}/billing-key`: charges a saved billing key once. `payment_id` must be unique per charge.
/// Returns `payment_id`, the id PortOne's cancel and lookup paths take. A 4xx is `Declined(type)`; a 5xx, transport
/// error, 409 AlreadyPaidError or a 200 without `payment.paidAt` is `Upstream` (outcome unknown: the caller looks the payment up).
/// Source: PortOne openapi, POST /payments/{paymentId}/billing-key (PayWithBillingKeyError), error body `{type, message}`.
/// shortcut: only the spec's error `type` names are known. Real card-decline codes need one test-store charge before live billing.
pub async fn charge_billing(
    p: &Providers,
    key: &str,
    payment_id: &str,
    amount: i64,
    currency: &str,
    order: &str,
    customer: Uuid,
) -> Result<String, ProviderError> {
    check_id(key)?;
    check_id(payment_id)?;
    let body = json!({
        "billingKey": key,
        "orderName": order,
        "customer": { "id": customer.to_string() },
        "amount": { "total": amount },
        "currency": currency,
    });
    charge_at(p.http(), p.portone_api(), &auth(p)?, payment_id, body).await
}

/// `charge_billing` against an explicit base and auth header. `payment_id` and `body` are already validated.
pub async fn charge_at(http: &reqwest::Client, base: &str, auth_header: &str, payment_id: &str, body: Value) -> Result<String, ProviderError> {
    let resp = http
        .post(format!("{base}/payments/{payment_id}/billing-key"))
        .header(AUTHORIZATION, auth_header)
        .json(&body)
        .send()
        .await
        .map_err(upstream)?;
    let status = resp.status();
    if status.is_server_error() {
        return Err(ProviderError::Upstream(format!("status {status}")));
    }
    if status == StatusCode::UNAUTHORIZED {
        return Err(ProviderError::NotConfigured("portone"));
    }
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    if status == StatusCode::CONFLICT && v["type"].as_str() == Some("AlreadyPaidError") {
        // The charge already succeeded (an earlier attempt whose reply was lost): look it up, never charge again.
        return Err(ProviderError::Upstream("charge_already_paid".into()));
    }
    if !status.is_success() {
        let code = v["type"].as_str().unwrap_or("charge_declined");
        return Err(ProviderError::Declined(code.to_string()));
    }
    if v["payment"]["paidAt"].is_string() {
        Ok(payment_id.to_string())
    } else {
        // Not confirmed as paid: the caller looks the payment up instead of charging again.
        Err(ProviderError::Upstream("charge_unconfirmed".into()))
    }
}

fn issued(info: &Value) -> Result<(), ProviderError> {
    match info["status"].as_str() {
        Some("ISSUED") => Ok(()),
        _ => Err(ProviderError::Declined("billing_key_not_issued".into())),
    }
}

/// Methods the dev payment window offers. Must match `devPaymentMethods` in dev_payment_sheet.dart.
pub const DEV_METHODS: [&str; 6] = ["card", "kakaopay", "naverpay", "tosspay", "transfer", "virtual_account"];

/// The method code a `dev:<method>` payment id carries, when it is one the dev window offers.
pub fn dev_method(id: &str) -> Option<&str> {
    id.strip_prefix("dev:").filter(|c| DEV_METHODS.contains(c))
}

/// Dev fakes: `dev:<method>` settles as that method (`dev:va` / `dev:virtual_account` issue a virtual
/// account), `dev:paid` settles at the expected total, and a checkout id pays as a card payment
/// (the dev gateway has no real payer).
pub fn dev_payment(id: &str, total: i64) -> Option<PaymentInfo> {
    let (status, method) = match id {
        "dev:paid" => ("PAID", json!({ "type": "PaymentMethodEasyPay" })),
        "dev:card" => ("PAID", json!({ "type": "PaymentMethodCard" })),
        _ if dev_method(id).is_some() && id != "dev:virtual_account" => ("PAID", json!({ "type": "PaymentMethodEasyPay", "easyPayProvider": &id[4..] })),
        "dev:va" | "dev:virtual_account" => (
            "VIRTUAL_ACCOUNT_ISSUED",
            json!({ "type": "PaymentMethodVirtualAccount", "bank": "KOOKMIN", "accountNumber": "123-456-789012" }),
        ),
        "dev:failed" => ("FAILED", json!({})),
        "dev:cancelled" => ("CANCELLED", json!({})),
        _ if Uuid::parse_str(id).is_ok() => ("PAID", json!({ "type": "PaymentMethodCard" })),
        _ => return None,
    };
    Some(PaymentInfo {
        status: status.into(),
        total,
        cancelled: 0,
        currency: "KRW".into(),
        method,
    })
}

pub fn dev_identity(id: &str) -> Option<IdentityInfo> {
    let rest = id.strip_prefix("dev:").filter(|r| !r.is_empty())?;
    Some(IdentityInfo {
        id: id.to_string(),
        status: "VERIFIED".into(),
        ci: Some(format!("dev-ci:{rest}")),
        name: None,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum WebhookError {
    Malformed,
    Stale,
    BadSignature,
}

/// Standard Webhooks check: HMAC-SHA256 over `<id>.<timestamp>.<body>` keyed with the
/// base64 part of `whsec_...`. The header may carry several `v1,` signatures during rotation.
pub fn verify_webhook(secret: &str, msg_id: &str, timestamp: &str, signatures: &str, body: &[u8], now: i64) -> Result<(), WebhookError> {
    let ts: i64 = timestamp.parse().map_err(|_| WebhookError::Malformed)?;
    if (now - ts).abs() > WEBHOOK_TOLERANCE_SECS {
        return Err(WebhookError::Stale);
    }
    let raw = secret.strip_prefix("whsec_").ok_or(WebhookError::Malformed)?;
    let key = STANDARD.decode(raw).map_err(|_| WebhookError::Malformed)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).map_err(|_| WebhookError::Malformed)?;
    mac.update(format!("{msg_id}.{timestamp}.").as_bytes());
    mac.update(body);
    let expected = STANDARD.encode(mac.finalize().into_bytes());
    let valid = signatures
        .split(' ')
        .filter_map(|s| s.strip_prefix("v1,"))
        .any(|s| bool::from(s.as_bytes().ct_eq(expected.as_bytes())));
    if valid { Ok(()) } else { Err(WebhookError::BadSignature) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::Path,
        http::{HeaderMap, StatusCode},
        routing::{delete, get, post},
    };

    const NOW: i64 = 1_800_000_000;

    fn secret() -> String {
        format!("whsec_{}", STANDARD.encode(b"0123456789abcdef0123456789abcdef"))
    }

    fn sign(secret: &str, id: &str, ts: i64, body: &[u8]) -> String {
        let key = STANDARD.decode(secret.trim_start_matches("whsec_")).expect("test key");
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("test key length");
        mac.update(format!("{id}.{ts}.").as_bytes());
        mac.update(body);
        format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn valid_signature_passes() {
        let body = br#"{"type":"Transaction.Paid"}"#;
        let sig = sign(&secret(), "msg_1", NOW, body);
        assert_eq!(verify_webhook(&secret(), "msg_1", &NOW.to_string(), &sig, body, NOW), Ok(()));
    }

    #[test]
    fn tampered_body_is_rejected() {
        let sig = sign(&secret(), "msg_1", NOW, b"{\"amount\":1}");
        let r = verify_webhook(&secret(), "msg_1", &NOW.to_string(), &sig, b"{\"amount\":9}", NOW);
        assert_eq!(r, Err(WebhookError::BadSignature));
    }

    #[test]
    fn stale_timestamp_is_rejected_both_ways() {
        let body = b"{}";
        let old = sign(&secret(), "m", NOW - 301, body);
        assert_eq!(
            verify_webhook(&secret(), "m", &(NOW - 301).to_string(), &old, body, NOW),
            Err(WebhookError::Stale)
        );
        let future = sign(&secret(), "m", NOW + 301, body);
        assert_eq!(
            verify_webhook(&secret(), "m", &(NOW + 301).to_string(), &future, body, NOW),
            Err(WebhookError::Stale)
        );
        let edge = sign(&secret(), "m", NOW - 300, body);
        assert_eq!(verify_webhook(&secret(), "m", &(NOW - 300).to_string(), &edge, body, NOW), Ok(()));
    }

    #[test]
    fn wrong_key_and_malformed_inputs_are_rejected() {
        let body = b"{}";
        let other = format!("whsec_{}", STANDARD.encode(b"another-key-another-key-another!"));
        let sig = sign(&other, "m", NOW, body);
        assert_eq!(
            verify_webhook(&secret(), "m", &NOW.to_string(), &sig, body, NOW),
            Err(WebhookError::BadSignature)
        );
        assert_eq!(verify_webhook(&secret(), "m", "soon", &sig, body, NOW), Err(WebhookError::Malformed));
        assert_eq!(verify_webhook("nope", "m", &NOW.to_string(), &sig, body, NOW), Err(WebhookError::Malformed));
    }

    #[test]
    fn rotated_signature_list_accepts_any_valid_entry() {
        let body = b"{}";
        let good = sign(&secret(), "m", NOW, body);
        let header = format!("v1,bogusbogus {good}");
        assert_eq!(verify_webhook(&secret(), "m", &NOW.to_string(), &header, body, NOW), Ok(()));
    }

    #[test]
    fn provider_ids_cannot_escape_the_url_path() {
        assert!(check_id("payment-abc_123").is_ok());
        assert!(check_id("../identity").is_err());
        assert!(check_id("x?y=1").is_err());
        assert!(check_id("").is_err());
    }

    #[test]
    fn custom_data_checkout_id_reads_object_and_json_text() {
        let id = Uuid::now_v7();
        let object = json!({ "checkoutId": id.to_string() });
        assert_eq!(custom_data_checkout_id(&object), Some(id));
        let text = Value::String(object.to_string());
        assert_eq!(custom_data_checkout_id(&text), Some(id));
        assert_eq!(custom_data_checkout_id(&json!({ "checkoutId": "not-a-uuid" })), None);
        assert_eq!(custom_data_checkout_id(&Value::String("{broken".into())), None);
        assert_eq!(custom_data_checkout_id(&Value::Null), None);
    }

    #[test]
    fn billing_needs_secret_store_and_a_billing_channel() {
        let mut p = PortOne {
            api_secret: Some("s".into()),
            store_id: Some("st".into()),
            ..Default::default()
        };
        assert!(!billing_ready(&p));
        p.channel_keys.insert("kakaopay".into(), "ch".into());
        assert!(!billing_ready(&p), "another method's channel does not bill");
        p.channel_keys.insert("card_billing".into(), "ch".into());
        assert!(billing_ready(&p));
        p.channel_keys.clear();
        p.channel_keys.insert("card".into(), "legacy".into());
        assert!(billing_ready(&p), "an install with only the old card channel keeps billing");
        assert_eq!(billing_channel(&p).map(String::as_str), Some("legacy"));
        p.api_secret = None;
        assert!(!billing_ready(&p));
    }

    /// Fake PortOne: checks the secret header and answers per billing key.
    async fn fake_billing(Path(key): Path<String>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
        if headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) != Some("PortOne test-secret") {
            return (StatusCode::UNAUTHORIZED, Json(json!({"type": "UnauthorizedError"})));
        }
        match key.as_str() {
            "bk_ok" => (StatusCode::OK, Json(json!({"status": "ISSUED", "billingKey": "bk_ok"}))),
            "bk_gone" => (StatusCode::OK, Json(json!({"status": "DELETED", "billingKey": "bk_gone"}))),
            _ => (StatusCode::NOT_FOUND, Json(json!({"type": "BillingKeyNotFoundError"}))),
        }
    }

    async fn billing_fake() -> String {
        let app = Router::new().route("/billing-keys/{key}", get(fake_billing));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind fake");
        let addr = listener.local_addr().expect("fake addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("fake serves") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn only_issued_billing_keys_pass_verification() {
        let base = billing_fake().await;
        let http = reqwest::Client::new();
        let ok = billing_key_at(&http, &base, "test-secret", "bk_ok").await.expect("issued");
        assert!(issued(&ok).is_ok());
        let gone = billing_key_at(&http, &base, "test-secret", "bk_gone").await.expect("deleted body");
        assert!(matches!(issued(&gone), Err(ProviderError::Declined(ref m)) if m == "billing_key_not_issued"));
        assert!(matches!(
            billing_key_at(&http, &base, "test-secret", "bk_none").await,
            Err(ProviderError::NotFound)
        ));
        assert!(matches!(
            billing_key_at(&http, &base, "wrong", "bk_ok").await,
            Err(ProviderError::NotConfigured(_))
        ));
    }

    /// Fake PortOne for delete and charge: each id picks the documented reply.
    async fn mock_delete(Path(key): Path<String>) -> (StatusCode, Json<Value>) {
        match key.as_str() {
            "gone" => (StatusCode::CONFLICT, Json(json!({"type": "BillingKeyAlreadyDeletedError"}))),
            "missing" => (StatusCode::NOT_FOUND, Json(json!({"type": "BillingKeyNotFoundError"}))),
            "scheduled" => (StatusCode::CONFLICT, Json(json!({"type": "PaymentScheduleAlreadyExistsError"}))),
            "unauth" => (StatusCode::UNAUTHORIZED, Json(json!({"type": "UnauthorizedError"}))),
            _ => (StatusCode::OK, Json(json!({"deletedAt": "2026-11-15T00:00:00Z"}))),
        }
    }

    async fn mock_charge(Path(pid): Path<String>) -> (StatusCode, Json<Value>) {
        match pid.as_str() {
            "paid" => (StatusCode::CONFLICT, Json(json!({"type": "AlreadyPaidError"}))),
            "over" => (StatusCode::CONFLICT, Json(json!({"type": "SumOfPartsExceedsTotalAmountError"}))),
            _ => (StatusCode::OK, Json(json!({"payment": {"pgTxId": "tx", "paidAt": "2026-11-15T00:00:00Z"}}))),
        }
    }

    async fn billing_mock() -> String {
        let app = Router::new()
            .route("/billing-keys/{key}", delete(mock_delete))
            .route("/payments/{pid}/billing-key", post(mock_charge));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind fake");
        let addr = listener.local_addr().expect("fake addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("fake serves") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn delete_treats_already_deleted_and_missing_keys_as_done() {
        let base = billing_mock().await;
        let http = reqwest::Client::new();
        let auth = "PortOne test-secret";
        assert!(delete_at(&http, &base, auth, "gone").await.is_ok());
        assert!(delete_at(&http, &base, auth, "missing").await.is_ok());
        assert!(delete_at(&http, &base, auth, "live").await.is_ok());
        assert!(matches!(delete_at(&http, &base, auth, "unauth").await, Err(ProviderError::NotConfigured(_))));
        assert!(matches!(delete_at(&http, &base, auth, "scheduled").await, Err(ProviderError::Upstream(_))));
    }

    #[tokio::test]
    async fn charge_maps_already_paid_to_lookup_and_declines_to_codes() {
        let base = billing_mock().await;
        let http = reqwest::Client::new();
        let auth = "PortOne test-secret";
        let body = json!({"billingKey": "bk", "orderName": "x", "customer": {"id": "u"}, "amount": {"total": 1}, "currency": "KRW"});
        assert_eq!(charge_at(&http, &base, auth, "fresh", body.clone()).await.expect("paid"), "fresh");
        assert!(matches!(charge_at(&http, &base, auth, "paid", body.clone()).await, Err(ProviderError::Upstream(m)) if m == "charge_already_paid"));
        assert!(matches!(charge_at(&http, &base, auth, "over", body).await, Err(ProviderError::Declined(m)) if m == "SumOfPartsExceedsTotalAmountError"));
    }

    #[test]
    fn test_channel_reads_the_channel_type() {
        assert!(test_channel(&json!({ "type": "TEST", "key": "channel-key-1" })));
        assert!(!test_channel(&json!({ "type": "LIVE" })));
        assert!(!test_channel(&Value::Null), "no channel: not flagged");
    }

    #[test]
    fn billing_key_must_be_bound_to_customer_and_ticket() {
        let info = json!({"status": "ISSUED", "billingKey": "bk", "issueId": "t1", "customer": {"id": "u1"}});
        assert!(bound(&info, "u1", "t1").is_ok());
        assert!(matches!(bound(&info, "u2", "t1"), Err(ProviderError::Declined(m)) if m == "billing_key_not_bound"));
        assert!(matches!(bound(&info, "u1", "t2"), Err(ProviderError::Declined(m)) if m == "billing_key_not_bound"));
        let no_fields = json!({"status": "ISSUED", "billingKey": "bk"});
        assert!(bound(&no_fields, "u1", "t1").is_err());
        let deleted = json!({"status": "DELETED", "issueId": "t1", "customer": {"id": "u1"}});
        assert!(matches!(bound(&deleted, "u1", "t1"), Err(ProviderError::Declined(m)) if m == "billing_key_not_issued"));
    }

    #[test]
    fn dev_gateway_pays_checkout_ids_and_keeps_fakes() {
        let id = Uuid::now_v7().to_string();
        assert_eq!(dev_payment(&id, 52_000).map(|p| (p.status, p.total)), Some(("PAID".into(), 52_000)));
        assert_eq!(dev_payment("dev:paid", 1).map(|p| p.status), Some("PAID".into()));
        assert_eq!(dev_payment("dev:va", 1).map(|p| p.status), Some("VIRTUAL_ACCOUNT_ISSUED".into()));
        assert_eq!(dev_payment("dev:kakaopay", 1).map(|p| p.status), Some("PAID".into()));
        assert_eq!(dev_method("dev:kakaopay"), Some("kakaopay"));
        assert_eq!(dev_method("dev:nope"), None);
        assert!(dev_payment("random-string", 1).is_none());
    }
}
