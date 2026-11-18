// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Toss Payments (direct, API 개별 연동 keys): window confirm, lookup by orderId, cancels, billing keys on the
//! 자동결제 MID and the transaction list for the daily diff. Answers are mapped into the PortOne-shaped
//! `PaymentInfo` (statuses and the virtual account), so checkout policy and the apps stay gateway-neutral.
//! The orderId is always our own id (checkout id or charge pid); the paymentKey is looked up, never stored.
//! Source: https://docs.tosspayments.com/reference (Payment, 결제 승인/조회/취소, 자동결제, 거래 조회, 에러 코드).

use super::{
    PaymentInfo, ProviderError, Providers, TOSS,
    payout::TOSS_API,
    portone::{BALANCE_MOVED, ProviderPayment, check_id},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{RequestBuilder, StatusCode, Url, header::AUTHORIZATION};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

/// A billing charge may take up to 60 s at Toss. 45 s plus one 10 s lookup keeps `bookings::charge`, which runs inside
/// a transaction, under the 60 s idle-in-transaction limit (db.rs).
const CHARGE_TIMEOUT: Duration = Duration::from_secs(45);
/// Toss's maximum page size for `/v1/transactions`.
const TX_PAGE: usize = 5000;
/// 20 pages of 5000 rows: hitting it fails the diff pass instead of diffing half a day.
const TX_MAX_PAGES: usize = 20;
/// Opaque Toss keys (paymentKey, billingKey) are at most 200 characters; an authKey at most 300.
const KEY_MAX: usize = 200;
const AUTH_KEY_MAX: usize = 300;
pub const SETUP_CHECK_ORDER: &str = "dahae-setup-check";

/// Which Toss MID a call is made on: the one-time window MID, or the 자동결제 (billing) MID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mid {
    Window,
    Billing,
}

/// `Basic base64("{secret}:")`, as every Toss server API takes it.
pub(crate) fn auth(secret: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{secret}:")))
}

fn secret(p: &Providers, mid: Mid) -> Result<String, ProviderError> {
    let integ = p.integ();
    let t = integ.toss_payments.as_ref().ok_or(ProviderError::NotConfigured(TOSS))?;
    match mid {
        Mid::Window => Ok(t.secret_key.clone()),
        Mid::Billing => t.billing.as_ref().map(|(_, sk)| sk.clone()).ok_or(ProviderError::NotConfigured(TOSS)),
    }
}

/// An opaque Toss key as it may be sent: visible ASCII, no whitespace, 1..=max characters.
pub fn valid_key(key: &str, max: usize) -> bool {
    !key.is_empty() && key.len() <= max && key.bytes().all(|b| b.is_ascii_graphic())
}

/// `{TOSS_API}/seg/...` with every segment percent-encoded, so an opaque key cannot change the path.
fn url(segments: &[&str]) -> Result<Url, ProviderError> {
    let mut u = Url::parse(TOSS_API).map_err(|e| ProviderError::Upstream(e.to_string()))?;
    u.path_segments_mut()
        .map_err(|()| ProviderError::Upstream("bad base url".into()))?
        .pop_if_empty()
        .extend(segments);
    Ok(u)
}

fn upstream(e: reqwest::Error) -> ProviderError {
    ProviderError::Upstream(e.to_string())
}

/// Sends a request (re-pointed by `with_toss_api` in tests). The body is JSON or empty; anything else reads as null.
async fn call(p: &Providers, rb: RequestBuilder) -> Result<(StatusCode, Value), ProviderError> {
    let req = p.toss_target(rb.build().map_err(upstream)?)?;
    let resp = p.http().execute(req).await.map_err(upstream)?;
    let status = resp.status();
    let text = resp.text().await.map_err(upstream)?;
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

/// The error code of a v1 body `{code, message}`, or of a v2 body `{error: {code}}`.
fn error_code(body: &Value) -> &str {
    body["code"].as_str().or_else(|| body["error"]["code"].as_str()).unwrap_or_default()
}

/// Maps a Toss error answer. Only a definite business refusal is a decline; anything that may still have
/// happened (5xx, 409, 429, a request still processing, Toss's internal failures) is Upstream, so callers look up.
pub(crate) fn classify(status: StatusCode, body: &Value) -> ProviderError {
    let code = error_code(body);
    if status == StatusCode::UNAUTHORIZED || matches!(code, "UNAUTHORIZED_KEY" | "INVALID_API_KEY") {
        return ProviderError::NotConfigured(TOSS);
    }
    if status == StatusCode::NOT_FOUND && matches!(code, "NOT_FOUND_PAYMENT" | "") {
        return ProviderError::NotFound;
    }
    let temporary = matches!(
        code,
        "ALREADY_PROCESSING_REQUEST" | "IDEMPOTENT_REQUEST_PROCESSING" | "COMMON_ERROR" | "PROVIDER_ERROR"
    ) || (code.starts_with("FAILED_") && code.contains("PROCESSING"));
    let unclear = matches!(status, StatusCode::CONFLICT | StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT);
    if temporary || unclear || !status.is_client_error() {
        return ProviderError::Upstream(format!("status {status} {code}"));
    }
    ProviderError::Declined(if code.is_empty() { "payment_declined".into() } else { code.to_string() })
}

/// Toss status -> the PortOne status names checkout policy reads. Unknown statuses pass through and never settle.
fn status(s: &str) -> &str {
    match s {
        "DONE" => "PAID",
        "WAITING_FOR_DEPOSIT" => "VIRTUAL_ACCOUNT_ISSUED",
        "CANCELED" => "CANCELLED",
        "PARTIAL_CANCELED" => "PARTIAL_CANCELLED",
        "ABORTED" | "EXPIRED" => "FAILED",
        "READY" | "IN_PROGRESS" => "READY",
        other => other,
    }
}

/// Toss 은행 코드 (two digits; the three-digit form is accepted too) -> the PortOne `Bank` names the apps label.
/// Source: https://docs.tosspayments.com/codes/org-codes (은행 코드). An unknown code passes through raw.
fn bank(code: &str) -> String {
    const BANKS: [(u16, &str); 25] = [
        (2, "KDB"),
        (3, "IBK"),
        (6, "KOOKMIN"),
        (7, "SUHYUP"),
        (11, "NONGHYUP"),
        (12, "LOCAL_NONGHYUP"),
        (20, "WOORI"),
        (23, "STANDARD_CHARTERED"),
        (27, "CITI"),
        (31, "DAEGU"),
        (32, "BUSAN"),
        (34, "KWANGJU"),
        (35, "JEJU"),
        (37, "JEONBUK"),
        (39, "KYONGNAM"),
        (45, "KFCC"),
        (48, "SHINHYUP"),
        (50, "SAVINGS_BANK"),
        (64, "NFCF"),
        (71, "POST"),
        (81, "HANA"),
        (88, "SHINHAN"),
        (89, "K_BANK"),
        (90, "KAKAO"),
        (92, "TOSS"),
    ];
    code.parse::<u16>()
        .ok()
        .and_then(|n| BANKS.iter().find(|(c, _)| *c == n))
        .map_or_else(|| code.to_string(), |(_, name)| (*name).to_string())
}

/// The method object under PortOne's field names. Toss returns no account holder, so `remitteeName` is left out.
fn method(v: &Value) -> Value {
    let va = &v["virtualAccount"];
    if va.is_object() {
        return json!({
            "type": "PaymentMethodVirtualAccount",
            "bank": bank(va["bankCode"].as_str().unwrap_or_default()),
            "accountNumber": va["accountNumber"],
            "expiredAt": va["dueDate"],
        });
    }
    if v["easyPay"].is_object() {
        return json!({ "type": "PaymentMethodEasyPay", "easyPayProvider": v["easyPay"]["provider"] });
    }
    json!({ "type": "PaymentMethodCard" })
}

/// A Toss Payment object as `PaymentInfo`. A missing amount reads as -1, which never matches: the payment is refunded.
pub fn info(v: &Value) -> PaymentInfo {
    let total = v["totalAmount"].as_i64();
    PaymentInfo {
        status: status(v["status"].as_str().unwrap_or_default()).to_string(),
        total: total.unwrap_or(-1),
        cancelled: total.zip(v["balanceAmount"].as_i64()).map_or(-1, |(t, b)| t - b),
        currency: v["currency"].as_str().unwrap_or_default().to_string(),
        method: method(v),
    }
}

/// Setup-wizard check: looks up an order that cannot exist. 404 means the secret key was accepted, 401 that it was not.
pub(crate) async fn check(p: &Providers, secret: &str) -> Result<StatusCode, ProviderError> {
    let rb = p
        .http()
        .get(url(&["v1", "payments", "orders", SETUP_CHECK_ORDER])?)
        .header(AUTHORIZATION, auth(secret));
    call(p, rb).await.map(|(s, _)| s)
}

/// The raw Payment object of an order, on the given MID.
async fn order(p: &Providers, mid: Mid, order_id: &str) -> Result<Value, ProviderError> {
    check_id(order_id)?;
    let rb = p
        .http()
        .get(url(&["v1", "payments", "orders", order_id])?)
        .header(AUTHORIZATION, auth(&secret(p, mid)?));
    match call(p, rb).await? {
        (s, v) if s.is_success() => Ok(v),
        (StatusCode::NOT_FOUND, _) => Err(ProviderError::NotFound),
        (s, v) => Err(classify(s, &v)),
    }
}

/// `GET /v1/payments/orders/{orderId}`.
pub async fn payment(p: &Providers, mid: Mid, order_id: &str) -> Result<PaymentInfo, ProviderError> {
    order(p, mid, order_id).await.map(|v| info(&v))
}

/// `POST /v1/payments/confirm` on the window MID, with the amount we stored. An unclear answer is looked up before it
/// counts as a failure (doc 15 §8); a lookup that shows nothing settled keeps the original error.
pub async fn confirm(p: &Providers, payment_key: &str, order_id: &str, amount: i64) -> Result<PaymentInfo, ProviderError> {
    check_id(order_id)?;
    if !valid_key(payment_key, KEY_MAX) {
        return Err(ProviderError::Declined("invalid_payment_key".into()));
    }
    let rb = p
        .http()
        .post(url(&["v1", "payments", "confirm"])?)
        .header(AUTHORIZATION, auth(&secret(p, Mid::Window)?))
        .json(&json!({ "paymentKey": payment_key, "orderId": order_id, "amount": amount }));
    let err = match call(p, rb).await {
        Ok((s, v)) if s.is_success() => return Ok(info(&v)),
        Ok((_, v)) if error_code(&v) == "ALREADY_PROCESSED_PAYMENT" => return payment(p, Mid::Window, order_id).await,
        Ok((s, v)) => match classify(s, &v) {
            // NOT_FOUND_PAYMENT_SESSION and other 404s: the authentication is gone, nothing was captured.
            ProviderError::NotFound => ProviderError::Declined(nonempty(error_code(&v), "NOT_FOUND_PAYMENT_SESSION")),
            e => e,
        },
        Err(e) => e,
    };
    if !matches!(err, ProviderError::Upstream(_)) {
        return Err(err);
    }
    match payment(p, Mid::Window, order_id).await {
        Ok(found) if found.status != "READY" => Ok(found),
        _ => Err(err),
    }
}

fn nonempty(code: &str, fallback: &str) -> String {
    if code.is_empty() { fallback.into() } else { code.into() }
}

/// Full or partial cancel of an order (refunds). With `cancellable`, Toss only cancels while the balance equals it
/// (`refundableAmount`), and a balance that already shows this cancel means our earlier attempt landed.
pub async fn cancel(p: &Providers, mid: Mid, order_id: &str, amount: i64, cancellable: Option<i64>) -> Result<(), ProviderError> {
    let v = order(p, mid, order_id).await?;
    if let Some(c) = cancellable {
        match v["balanceAmount"].as_i64() {
            Some(left) if left == c - amount => return landed(order_id),
            Some(left) if left == c => {}
            _ => return Err(ProviderError::Declined(BALANCE_MOVED.into())),
        }
    }
    let key = v["paymentKey"].as_str().filter(|k| valid_key(k, KEY_MAX));
    let key = key.ok_or_else(|| ProviderError::Upstream("payment without paymentKey".into()))?;
    let mut body = json!({ "cancelReason": "예약 환불", "cancelAmount": amount });
    if let Some(c) = cancellable {
        body["refundableAmount"] = json!(c);
    }
    let rb = p
        .http()
        .post(url(&["v1", "payments", key, "cancel"])?)
        .header(AUTHORIZATION, auth(&secret(p, mid)?))
        .json(&body);
    let (s, v) = call(p, rb).await?;
    if s.is_success() {
        return Ok(());
    }
    match (error_code(&v), cancellable) {
        ("NOT_MATCHES_REFUNDABLE_AMOUNT" | "ALREADY_CANCELED_PAYMENT", Some(c)) => {
            let now = order(p, mid, order_id).await?;
            if now["balanceAmount"].as_i64() == Some(c - amount) {
                landed(order_id)
            } else {
                Err(ProviderError::Declined(BALANCE_MOVED.into()))
            }
        }
        _ => Err(classify(s, &v)),
    }
}

fn landed(order_id: &str) -> Result<(), ProviderError> {
    tracing::warn!(order = order_id, "cancel already applied at Toss Payments; recording it");
    Ok(())
}

/// `POST /v1/billing/authorizations/issue` on the billing MID: the authKey from the card window becomes a billingKey.
/// The key must be bound to our customerKey. It cannot be read again, so the caller stores it sealed.
pub async fn issue_billing_key(p: &Providers, auth_key: &str, customer_key: &str) -> Result<String, ProviderError> {
    if !valid_key(auth_key, AUTH_KEY_MAX) {
        return Err(ProviderError::Declined("invalid_billing_key".into()));
    }
    let rb = p
        .http()
        .post(url(&["v1", "billing", "authorizations", "issue"])?)
        .header(AUTHORIZATION, auth(&secret(p, Mid::Billing)?))
        .json(&json!({ "authKey": auth_key, "customerKey": customer_key }));
    let (s, v) = call(p, rb).await?;
    if !s.is_success() {
        return Err(classify(s, &v));
    }
    if v["customerKey"].as_str() != Some(customer_key) {
        return Err(ProviderError::Declined("billing_key_not_bound".into()));
    }
    v["billingKey"]
        .as_str()
        .filter(|k| valid_key(k, KEY_MAX))
        .map(str::to_string)
        .ok_or_else(|| ProviderError::Upstream("issue without billingKey".into()))
}

/// `POST /v1/billing/{billingKey}`: charges a saved card once under our `order_id` (unique per charge). Returns the
/// order id, which lookups and cancels take. Anything that may have charged is Upstream: the caller looks it up.
pub async fn charge_billing(
    p: &Providers,
    billing_key: &str,
    customer_key: &str,
    order_id: &str,
    amount: i64,
    order_name: &str,
) -> Result<String, ProviderError> {
    check_id(order_id)?;
    if !valid_key(billing_key, KEY_MAX) {
        return Err(ProviderError::Declined("invalid_billing_key".into()));
    }
    let rb = p
        .http()
        .post(url(&["v1", "billing", billing_key])?)
        .header(AUTHORIZATION, auth(&secret(p, Mid::Billing)?))
        .timeout(CHARGE_TIMEOUT)
        .json(&json!({ "customerKey": customer_key, "amount": amount, "orderId": order_id, "orderName": order_name }));
    let (s, v) = call(p, rb).await?;
    if s.is_success() {
        let done = v["status"].as_str() == Some("DONE") && v["totalAmount"].as_i64() == Some(amount);
        return if done {
            Ok(order_id.to_string())
        } else {
            Err(ProviderError::Upstream("charge_unconfirmed".into()))
        };
    }
    if matches!(error_code(&v), "DUPLICATED_ORDER_ID" | "ALREADY_PROCESSED_PAYMENT") {
        // An earlier attempt of this order went through: look it up, never charge again.
        return Err(ProviderError::Upstream("charge_already_paid".into()));
    }
    Err(classify(s, &v))
}

/// `DELETE /v1/billing/{billingKey}` on the billing MID. 404 is done (warned); anything unclear fails closed.
pub async fn delete_billing_key(p: &Providers, billing_key: &str) -> Result<(), ProviderError> {
    if !valid_key(billing_key, KEY_MAX) {
        return Err(ProviderError::Upstream("unreadable billing key".into()));
    }
    let rb = p
        .http()
        .delete(url(&["v1", "billing", billing_key])?)
        .header(AUTHORIZATION, auth(&secret(p, Mid::Billing)?));
    match call(p, rb).await? {
        (s, _) if s.is_success() => Ok(()),
        (StatusCode::NOT_FOUND, _) => {
            tracing::warn!("billing key not found at Toss Payments on delete; treating as deleted");
            Ok(())
        }
        (StatusCode::UNAUTHORIZED, _) => Err(ProviderError::NotConfigured(TOSS)),
        (s, v) => Err(ProviderError::Upstream(format!("billing key not deleted: {s} {}", error_code(&v)))),
    }
}

/// Every transaction in `[start, end]` (KST, `yyyy-MM-ddTHH:mm:ss`) on one MID, folded to one row per orderId.
pub async fn transactions(p: &Providers, mid: Mid, start: &str, end: &str) -> Result<Vec<ProviderPayment>, ProviderError> {
    let auth_header = auth(&secret(p, mid)?);
    let limit = TX_PAGE.to_string();
    let mut rows: Vec<Value> = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..TX_MAX_PAGES {
        let mut query = vec![("startDate", start), ("endDate", end), ("limit", limit.as_str())];
        if let Some(a) = &after {
            query.push(("startingAfter", a.as_str()));
        }
        let rb = p.http().get(url(&["v1", "transactions"])?).query(&query).header(AUTHORIZATION, &auth_header);
        let (s, v) = call(p, rb).await?;
        if !s.is_success() {
            return Err(classify(s, &v));
        }
        let page = v.as_array().ok_or_else(|| ProviderError::Upstream("transaction list is not a list".into()))?;
        let full = page.len() >= TX_PAGE;
        after = page.last().and_then(|t| t["transactionKey"].as_str()).map(str::to_string);
        rows.extend(page.iter().cloned());
        if !full {
            return fold(&rows);
        }
        if after.is_none() {
            return Err(ProviderError::Upstream("transaction without transactionKey".into()));
        }
    }
    Err(ProviderError::Upstream("transaction list exceeds the page cap".into()))
}

/// Transactions -> one payment per orderId: the status of the latest transaction, and the amount approved (the
/// DONE transaction), or the first transaction's amount when nothing was approved. A row without orderId or amount
/// fails the pass rather than dropping a payment from the diff.
pub fn fold(rows: &[Value]) -> Result<Vec<ProviderPayment>, ProviderError> {
    let mut order: Vec<&str> = Vec::new();
    let mut by_id: HashMap<&str, Vec<&Value>> = HashMap::new();
    for r in rows {
        let id = r["orderId"]
            .as_str()
            .ok_or_else(|| ProviderError::Upstream("transaction without orderId".into()))?;
        if r["amount"].as_i64().is_none() {
            return Err(ProviderError::Upstream("transaction without amount".into()));
        }
        let list = by_id.entry(id).or_default();
        if list.is_empty() {
            order.push(id);
        }
        list.push(r);
    }
    Ok(order
        .into_iter()
        .map(|id| {
            let txs = &by_id[id];
            let at = |t: &&&Value| chrono::DateTime::parse_from_rfc3339(t["transactionAt"].as_str().unwrap_or_default()).ok();
            let latest = txs.iter().max_by_key(at).unwrap_or(&txs[0]);
            let done = txs.iter().find(|t| t["status"].as_str() == Some("DONE")).unwrap_or(&txs[0]);
            ProviderPayment {
                id: id.to_string(),
                status: status(latest["status"].as_str().unwrap_or_default()).to_string(),
                total: done["amount"].as_i64().unwrap_or(-1),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_is_basic_of_the_secret_and_a_colon() {
        assert_eq!(auth("test_sk_abc"), format!("Basic {}", STANDARD.encode("test_sk_abc:")));
    }

    #[test]
    fn every_toss_status_maps_to_the_policy_names() {
        for (toss, ours) in [
            ("DONE", "PAID"),
            ("WAITING_FOR_DEPOSIT", "VIRTUAL_ACCOUNT_ISSUED"),
            ("CANCELED", "CANCELLED"),
            ("PARTIAL_CANCELED", "PARTIAL_CANCELLED"),
            ("ABORTED", "FAILED"),
            ("EXPIRED", "FAILED"),
            ("READY", "READY"),
            ("IN_PROGRESS", "READY"),
        ] {
            assert_eq!(info(&json!({ "status": toss })).status, ours, "{toss}");
        }
    }

    #[test]
    fn amounts_map_and_missing_ones_read_minus_one() {
        let i = info(&json!({ "status": "PARTIAL_CANCELED", "totalAmount": 52000, "balanceAmount": 2000, "currency": "KRW" }));
        assert_eq!((i.total, i.cancelled, i.currency.as_str()), (52000, 50000, "KRW"));
        let i = info(&json!({ "status": "DONE", "balanceAmount": 2000 }));
        assert_eq!((i.total, i.cancelled), (-1, -1));
        let i = info(&json!({ "status": "DONE", "totalAmount": 2000 }));
        assert_eq!((i.total, i.cancelled), (2000, -1));
    }

    #[test]
    fn virtual_account_takes_portone_field_names() {
        let v = json!({ "status": "WAITING_FOR_DEPOSIT", "virtualAccount": {
            "bankCode": "88", "accountNumber": "X1234", "dueDate": "2026-11-19T10:00:00+09:00", "customerName": "payer" } });
        assert_eq!(
            info(&v).method,
            json!({ "type": "PaymentMethodVirtualAccount", "bank": "SHINHAN", "accountNumber": "X1234", "expiredAt": "2026-11-19T10:00:00+09:00" })
        );
        assert_eq!(bank("088"), "SHINHAN", "three-digit form");
        assert_eq!(bank("06"), "KOOKMIN");
        assert_eq!(bank("54"), "54", "unknown code passes through raw");
        assert_eq!(bank(""), "");
    }

    #[test]
    fn easy_pay_and_card_methods() {
        let e = info(&json!({ "status": "DONE", "easyPay": { "provider": "카카오페이" } }));
        assert_eq!(e.method, json!({ "type": "PaymentMethodEasyPay", "easyPayProvider": "카카오페이" }));
        assert_eq!(info(&json!({ "status": "DONE" })).method, json!({ "type": "PaymentMethodCard" }));
    }

    fn declined(e: ProviderError) -> Option<String> {
        if let ProviderError::Declined(c) = e { Some(c) } else { None }
    }

    #[test]
    fn errors_classify_per_the_table() {
        let b = |code: &str| json!({ "code": code, "message": "m" });
        assert!(matches!(
            classify(StatusCode::UNAUTHORIZED, &b("UNAUTHORIZED_KEY")),
            ProviderError::NotConfigured(TOSS)
        ));
        assert!(matches!(
            classify(StatusCode::BAD_REQUEST, &b("INVALID_API_KEY")),
            ProviderError::NotConfigured(_)
        ));
        assert!(matches!(classify(StatusCode::NOT_FOUND, &b("NOT_FOUND_PAYMENT")), ProviderError::NotFound));
        assert!(matches!(classify(StatusCode::NOT_FOUND, &Value::Null), ProviderError::NotFound));
        assert_eq!(
            declined(classify(StatusCode::NOT_FOUND, &b("NOT_FOUND_PAYMENT_SESSION"))).as_deref(),
            Some("NOT_FOUND_PAYMENT_SESSION")
        );
        assert_eq!(
            declined(classify(StatusCode::FORBIDDEN, &b("REJECT_CARD_COMPANY"))).as_deref(),
            Some("REJECT_CARD_COMPANY")
        );
        assert_eq!(
            declined(classify(StatusCode::FORBIDDEN, &b("NOT_CANCELABLE_PAYMENT"))).as_deref(),
            Some("NOT_CANCELABLE_PAYMENT")
        );
        assert_eq!(
            declined(classify(StatusCode::BAD_REQUEST, &json!({ "error": { "code": "EXCEED_MAX_AMOUNT" } }))).as_deref(),
            Some("EXCEED_MAX_AMOUNT"),
            "v2 error body"
        );
        assert_eq!(declined(classify(StatusCode::BAD_REQUEST, &Value::Null)).as_deref(), Some("payment_declined"));
        for (st, body) in [
            (StatusCode::CONFLICT, b("IDEMPOTENT_REQUEST_PROCESSING")),
            (StatusCode::BAD_REQUEST, b("ALREADY_PROCESSING_REQUEST")),
            (StatusCode::TOO_MANY_REQUESTS, Value::Null),
            (StatusCode::INTERNAL_SERVER_ERROR, b("FAILED_INTERNAL_SYSTEM_PROCESSING")),
            (StatusCode::BAD_REQUEST, b("FAILED_PAYMENT_INTERNAL_SYSTEM_PROCESSING")),
            (StatusCode::BAD_REQUEST, b("COMMON_ERROR")),
            (StatusCode::BAD_GATEWAY, Value::Null),
        ] {
            assert!(matches!(classify(st, &body), ProviderError::Upstream(_)), "{st} {body}");
        }
    }

    #[test]
    fn opaque_keys_are_visible_ascii_without_whitespace() {
        assert!(valid_key("tgen_2024=abc/def", KEY_MAX), "= and / are allowed in the value");
        assert!(!valid_key("a b", KEY_MAX));
        assert!(!valid_key("a\nb", KEY_MAX));
        assert!(!valid_key("", KEY_MAX));
        assert!(!valid_key("키", KEY_MAX));
        assert!(valid_key(&"k".repeat(200), KEY_MAX));
        assert!(!valid_key(&"k".repeat(201), KEY_MAX));
    }

    #[test]
    fn keys_in_paths_are_percent_encoded() {
        let u = url(&["v1", "payments", "a/b=c?d", "cancel"]).expect("url");
        assert_eq!(u.as_str(), "https://api.tosspayments.com/v1/payments/a%2Fb=c%3Fd/cancel");
    }

    fn tx(order: &str, status: &str, amount: i64, at: &str) -> Value {
        json!({ "orderId": order, "status": status, "amount": amount, "transactionAt": at, "transactionKey": format!("{order}-{status}") })
    }

    #[test]
    fn transactions_fold_to_one_payment_per_order() {
        let rows = [
            tx("ck-1", "DONE", 52000, "2026-11-18T10:00:00+09:00"),
            tx("va-1", "WAITING_FOR_DEPOSIT", 30000, "2026-11-18T09:00:00+09:00"),
            tx("ck-1", "CANCELED", 52000, "2026-11-18T12:00:00+09:00"),
            tx("va-1", "DONE", 30000, "2026-11-18T11:00:00+09:00"),
            tx("cx-1", "PARTIAL_CANCELED", 1000, "2026-11-18T13:00:00+09:00"),
        ];
        let folded = fold(&rows).expect("fold");
        let pp = |id: &str, status: &str, total: i64| ProviderPayment {
            id: id.into(),
            status: status.into(),
            total,
        };
        assert_eq!(
            folded,
            vec![pp("ck-1", "CANCELLED", 52000), pp("va-1", "PAID", 30000), pp("cx-1", "PARTIAL_CANCELLED", 1000),]
        );
        assert!(fold(&[json!({ "status": "DONE", "amount": 1 })]).is_err(), "no orderId");
        assert!(fold(&[json!({ "orderId": "o-1", "status": "DONE" })]).is_err(), "no amount");
    }
}
