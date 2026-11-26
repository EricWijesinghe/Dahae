// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Provider callbacks. The Standard Webhooks signature is the gate. Without a configured
//! secret only dev mode accepts unsigned events (smoke tests); production refuses them.
//! Events never move money on their own: the payment is re-read from the provider.
//! Every accepted delivery is claimed in `webhook_events` before it is handled and marked done after, so a
//! redelivery of a finished event does nothing. Unsigned Toss Payments hints skip the ledger (see `toss_payments`).

use super::bookings::Actor;
use super::cases::{self, NewCase};
use super::checkouts::{CheckoutRow, find_by_id, find_by_payment, settle};
use super::tax_invoices;
use crate::{
    error::{ApiError, ApiResult},
    providers::{
        ProviderError, TOSS, TOSS_BILLING,
        portone::{self, WebhookError, verify_webhook},
        toss_webhook,
    },
    state::AppState,
};
use axum::{Json, Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use std::future::Future;
use uuid::Uuid;

const PAYMENT_EVENTS: [&str; 2] = ["Transaction.Paid", "Transaction.VirtualAccountIssued"];
/// Cancellations PortOne reports: a refund made in its console, or a partial one. They never settle a checkout.
const CANCEL_EVENTS: [&str; 2] = ["Transaction.Cancelled", "Transaction.PartialCancelled"];

/// Gateway names in staff case summaries.
const PORTONE_NAME: &str = "PortOne";
const TOSS_NAME: &str = "Toss Payments";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks/portone", post(portone))
        .route("/webhooks/toss", post(toss))
        .route("/webhooks/tosspayments", post(toss_payments))
}

/// How a Toss Payments webhook is let in. Toss does not promise the signature header on payment events, and the
/// body is only a hint (the handlers re-read the payment with our secret), so an unsigned event is accepted.
#[derive(Debug, PartialEq, Eq)]
pub enum TossGate {
    /// 보안 키 set and the header present: the signature must verify.
    Verify,
    /// Accept as a hint.
    Hint,
    /// Production with no Toss card: refuse.
    Unavailable,
}

pub fn toss_gate(dev_mode: bool, configured: bool, security_key: bool, signed: bool) -> TossGate {
    match (configured, security_key && signed) {
        (true, true) => TossGate::Verify,
        (true, false) => TossGate::Hint,
        (false, _) if dev_mode => TossGate::Hint,
        (false, _) => TossGate::Unavailable,
    }
}

/// What a Toss payment event is about: the orderId (`data.orderId`, or top level for DEPOSIT_CALLBACK, which has no
/// eventType) and, for a cancellation hint, the dispute kind it is recorded as. None for events about no payment.
pub fn toss_event(v: &Value) -> Option<(String, Option<&'static str>)> {
    let kind = v["eventType"].as_str();
    if !matches!(kind, None | Some("PAYMENT_STATUS_CHANGED" | "DEPOSIT_CALLBACK")) {
        return None;
    }
    let order = v["data"]["orderId"].as_str().or_else(|| v["orderId"].as_str())?;
    let status = v["data"]["status"].as_str().or_else(|| v["status"].as_str()).unwrap_or_default();
    let cancel = match status {
        "CANCELED" => Some(CANCEL_EVENTS[0]),
        "PARTIAL_CANCELED" => Some(CANCEL_EVENTS[1]),
        _ => None,
    };
    Some((order.to_string(), cancel))
}

/// Toss Payments `PAYMENT_STATUS_CHANGED` and `DEPOSIT_CALLBACK`. The payment is always re-read with our secret key,
/// so a forged body can at most cause one lookup. A 5xx makes Toss retry.
async fn toss_payments(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    let integ = s.providers.integ();
    let key = integ.toss_payments.as_ref().and_then(|t| t.security_key.clone());
    let signature = header(&headers, toss_webhook::SIGNATURE_HEADER);
    let gate = toss_gate(s.config.dev_mode, integ.toss_payments.is_some(), key.is_some(), !signature.is_empty());
    match gate {
        TossGate::Verify => {
            let time = header(&headers, toss_webhook::TRANSMISSION_TIME_HEADER);
            if !toss_webhook::verify(key.as_deref().unwrap_or_default(), time, signature, &body) {
                tracing::warn!("tosspayments webhook rejected: bad signature");
                s.providers.observe("tosspayments_webhook", Err("bad_signature")).await;
                return Err(ApiError::Unauthorized);
            }
            s.providers.observe("tosspayments_webhook", Ok(())).await;
        }
        TossGate::Hint => {}
        TossGate::Unavailable => return Err(ApiError::Unavailable("provider_not_configured")),
    }
    let signed = gate == TossGate::Verify;
    let event: Value = serde_json::from_slice(&body).map_err(|_| ApiError::BadRequest("invalid_event"))?;
    let work = async {
        match toss_event(&event) {
            Some((order, Some(kind))) => sync_cancellation(&s, &[TOSS, TOSS_BILLING], &order, kind).await,
            Some((order, None)) => handle_payment(&s, &order, false).await,
            None => Ok(()),
        }
    };
    // An unsigned hint never enters the ledger: anyone could post one, so recording it would let a stranger fill the
    // table or pre-record a genuine body and suppress the real event. The handlers re-read Toss and are idempotent.
    if signed {
        once(&s, TOSS, &event_id("", &body), work).await?;
    } else {
        work.await?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// Toss `payout.changed` / `seller.changed`. Signed with the 보안 키 (see `providers::toss_webhook`). A verified
/// event only kicks the sync, which re-reads every open payout from Toss. Repeats coalesce.
async fn toss(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<axum::http::StatusCode> {
    let key = s.providers.integ().payout.as_ref().and_then(|p| p.security_key.clone());
    match (key, s.config.dev_mode) {
        (Some(key), _) => {
            let verified = toss_webhook::verify(
                &key,
                header(&headers, toss_webhook::TRANSMISSION_TIME_HEADER),
                header(&headers, toss_webhook::SIGNATURE_HEADER),
                &body,
            );
            if !verified {
                tracing::warn!("toss webhook rejected: bad signature");
                s.providers.observe("toss_webhook", Err("bad_signature")).await;
                return Err(ApiError::Unauthorized);
            }
            s.providers.observe("toss_webhook", Ok(())).await;
        }
        (None, true) => tracing::warn!("unsigned toss webhook accepted: DEV_MODE without a payout security key"),
        (None, false) => return Err(ApiError::Unavailable("provider_not_configured")),
    }
    once(&s, "toss", &event_id("", &body), async {
        crate::payout_sync::kick(s.clone());
        Ok(())
    })
    .await?;
    Ok(axum::http::StatusCode::OK)
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> &'a str {
    h.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

#[derive(Debug, PartialEq, Eq)]
pub enum WebhookAuth {
    /// A secret is configured: the signature must verify.
    Verify,
    /// No secret, dev mode: accept unsigned events.
    UnsignedDev,
    /// No secret, not dev mode: refuse everything.
    Unavailable,
}

/// The signature rule, as a pure decision. A configured secret always wins, so dev mode
/// never bypasses a real secret.
pub fn webhook_auth(dev_mode: bool, secret: Option<&str>) -> WebhookAuth {
    match (secret, dev_mode) {
        (Some(_), _) => WebhookAuth::Verify,
        (None, true) => WebhookAuth::UnsignedDev,
        (None, false) => WebhookAuth::Unavailable,
    }
}

/// Events that can change a checkout's payment state.
pub fn is_payment_event(kind: &str) -> bool {
    PAYMENT_EVENTS.contains(&kind)
}

/// Events that report a cancellation of a payment we hold.
pub fn is_cancel_event(kind: &str) -> bool {
    CANCEL_EVENTS.contains(&kind)
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: EventData,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EventData {
    payment_id: Option<String>,
}

async fn portone(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    let integ = s.providers.integ();
    let secret = integ.portone.webhook_secret.as_deref();
    match (webhook_auth(s.config.dev_mode, secret), secret) {
        (WebhookAuth::Verify, Some(secret)) => {
            let verified = verify_webhook(
                secret,
                header(&headers, "webhook-id"),
                header(&headers, "webhook-timestamp"),
                header(&headers, "webhook-signature"),
                &body,
                Utc::now().timestamp(),
            )
            .inspect_err(|e: &WebhookError| tracing::warn!(error = ?e, "portone webhook rejected"))
            .map_err(|_| ApiError::Unauthorized);
            match verified {
                Ok(()) => s.providers.observe("portone_webhook", Ok(())).await,
                Err(e) => {
                    s.providers.observe("portone_webhook", Err("bad_signature")).await;
                    return Err(e);
                }
            }
        }
        (WebhookAuth::UnsignedDev, _) => {
            tracing::warn!("unsigned portone webhook accepted: DEV_MODE without PORTONE_WEBHOOK_SECRET");
        }
        _ => return Err(ApiError::Unavailable("provider_not_configured")),
    }
    let event: Event = serde_json::from_slice(&body).map_err(|_| ApiError::BadRequest("invalid_event"))?;
    once(&s, "portone", &event_id(header(&headers, "webhook-id"), &body), dispatch(&s, &event)).await?;
    Ok(Json(json!({ "ok": true })))
}

/// The ledger key of a delivery: the provider's own message id (PortOne's Standard Webhooks `webhook-id`, the same
/// on every retry), else a hash of the body. Toss sends no id; a retry carries the same body, and a real state change
/// always changes the body (status, timestamps), so a new event is never taken for a repeat.
pub fn event_id(provider_id: &str, body: &[u8]) -> String {
    if provider_id.is_empty() {
        format!("sha256:{}", crate::auth::sha256_hex(body))
    } else {
        provider_id.to_string()
    }
}

/// Runs `work` until one delivery of `event` from `adapter` has finished it; a repeat of a finished event is
/// acknowledged and does nothing. A delivery claims the ledger row before the work and marks it done after. A
/// repeat that finds an unfinished claim gets a 503, so the provider sends it again: the first attempt may still
/// fail. A claim older than the request timeout was cut off (its future dropped or the process gone), so it is
/// taken again. If the work fails, the row is removed, so the provider's retry is handled.
async fn once(s: &AppState, adapter: &str, event: &str, work: impl Future<Output = ApiResult<()>>) -> ApiResult<()> {
    let claimed = sqlx::query(
        "INSERT INTO webhook_events (adapter_id, event_id) VALUES ($1, $2)
         ON CONFLICT (adapter_id, event_id) DO UPDATE SET received_at = now()
         WHERE webhook_events.done_at IS NULL AND webhook_events.received_at < now() - make_interval(secs => $3)",
    )
    .bind(adapter)
    .bind(event)
    .bind(super::REQUEST_TIMEOUT.as_secs_f64())
    .execute(&s.db)
    .await?
    .rows_affected()
        == 1;
    if !claimed {
        let done: Option<bool> = sqlx::query_scalar("SELECT done_at IS NOT NULL FROM webhook_events WHERE adapter_id = $1 AND event_id = $2")
            .bind(adapter)
            .bind(event)
            .fetch_optional(&s.db)
            .await?;
        if done == Some(true) {
            tracing::info!(adapter, event, "webhook redelivery ignored");
            return Ok(());
        }
        tracing::info!(adapter, event, "webhook redelivery while the first is in progress: asked to retry");
        return Err(ApiError::Unavailable("webhook_in_progress"));
    }
    if let Err(e) = work.await {
        if let Err(del) = sqlx::query("DELETE FROM webhook_events WHERE adapter_id = $1 AND event_id = $2")
            .bind(adapter)
            .bind(event)
            .execute(&s.db)
            .await
        {
            tracing::error!(error = %del, adapter, event, "failed webhook claim kept: its retry runs once the claim is stale");
        }
        return Err(e);
    }
    sqlx::query("UPDATE webhook_events SET done_at = now() WHERE adapter_id = $1 AND event_id = $2")
        .bind(adapter)
        .bind(event)
        .execute(&s.db)
        .await?;
    Ok(())
}

/// Hands a verified event to the handler that owns it. Unrelated events are acknowledged and ignored.
async fn dispatch(s: &AppState, event: &Event) -> ApiResult<()> {
    let Some(payment_id) = event.data.payment_id.as_deref() else {
        return Ok(());
    };
    if is_cancel_event(&event.kind) {
        sync_cancellation(s, &["portone"], payment_id, &event.kind).await
    } else if is_payment_event(&event.kind) {
        handle_payment(s, payment_id, true).await
    } else {
        Ok(())
    }
}

/// What a provider cancellation changes on our side.
#[derive(Debug, PartialEq, Eq)]
enum CancelPlan {
    /// The provider holds what we recorded: our own refund, or a replayed event.
    Unchanged,
    /// The provider took back more than we recorded: a refund made outside the app.
    External { status: &'static str },
}

/// Pure rule. `provider_cancelled` is the cancelled amount PortOne reports (-1 when it is missing). A missing or
/// impossible amount is refused, so PortOne re-sends the event instead of the payment being recorded wrongly.
fn plan_cancellation(amount: i64, recorded: i64, provider_cancelled: i64) -> ApiResult<CancelPlan> {
    if provider_cancelled < 0 || provider_cancelled > amount {
        return Err(ApiError::Internal(format!(
            "portone cancelled amount {provider_cancelled} does not fit the payment"
        )));
    }
    if provider_cancelled <= recorded {
        return Ok(CancelPlan::Unchanged);
    }
    let status = if provider_cancelled == amount { "refunded" } else { "partially_refunded" };
    Ok(CancelPlan::External { status })
}

/// Records a cancellation at the amount PortOne reports, so a replay sets the same values again. A cancel the app
/// already recorded changes nothing. Any other cancel opens a dispute and a staff case once per cumulative
/// cancelled amount: PortOne's transaction id can be the original payment's on every cancel, so it is not used.
/// `providers` are the `payments.provider` codes of the gateway that sent the event.
async fn sync_cancellation(s: &AppState, providers: &[&str], payment_id: &str, kind: &str) -> ApiResult<()> {
    let mut tx = s.db.begin().await?;
    // Booking, then payment: the order apply_cancel and the refund paths lock in, so the two cannot deadlock.
    let booking: Option<Uuid> = sqlx::query_scalar(
        "SELECT b.id FROM bookings b JOIN payments p ON p.booking_id = b.id WHERE p.provider = ANY($2) AND p.provider_tx_id = $1 FOR UPDATE OF b",
    )
    .bind(payment_id)
    .bind(providers)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(booking) = booking else {
        tracing::info!(payment_id, "gateway cancellation for a payment with no booking");
        return Ok(());
    };
    // Locked until commit: a refund the app is making right now is recorded before this reads the amount.
    let row: Option<(Uuid, i64, i64, String)> =
        sqlx::query_as("SELECT id, amount, refunded_amount, provider FROM payments WHERE provider = ANY($2) AND provider_tx_id = $1 FOR UPDATE")
            .bind(payment_id)
            .bind(providers)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((payment, amount, recorded, provider)) = row else {
        return Ok(());
    };
    let cancelled = match s.providers.payment(&provider, payment_id, amount).await {
        Ok(info) => info.cancelled,
        Err(ProviderError::NotFound) => {
            tracing::warn!(payment_id, provider, "gateway cancellation for a payment the gateway does not know");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let CancelPlan::External { status } = plan_cancellation(amount, recorded, cancelled)? else {
        return Ok(());
    };
    record_external_cancel(
        &mut tx,
        Recorded {
            payment,
            booking,
            kind,
            cancelled,
            status,
            gateway: if provider == "portone" { PORTONE_NAME } else { TOSS_NAME },
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// One cancellation made outside the app, as it is written to the payment, its dispute and its staff case.
struct Recorded<'a> {
    payment: Uuid,
    booking: Uuid,
    kind: &'a str,
    cancelled: i64,
    status: &'static str,
    /// The gateway's name, for the staff case.
    gateway: &'static str,
}

async fn record_external_cancel(tx: &mut sqlx::PgConnection, c: Recorded<'_>) -> ApiResult<()> {
    sqlx::query("UPDATE payments SET refunded_amount = $2, status = $3 WHERE id = $1")
        .bind(c.payment)
        .bind(c.cancelled)
        .bind(c.status)
        .execute(&mut *tx)
        .await?;
    if c.status == "refunded" {
        tax_invoices::cancel_for_refunded_payment(&mut *tx, c.payment, Actor { role: "system", id: None }).await?;
    }
    let key = format!("{}:{}", c.payment, c.cancelled);
    let opened: Option<Uuid> =
        sqlx::query_scalar("INSERT INTO disputes (payment_id, provider_tx_id, kind) VALUES ($1, $2, $3) ON CONFLICT (provider_tx_id) DO NOTHING RETURNING id")
            .bind(c.payment)
            .bind(&key)
            .bind(c.kind)
            .fetch_optional(&mut *tx)
            .await?;
    if opened.is_some() {
        cases::insert(
            &mut *tx,
            NewCase {
                booking: Some(c.booking),
                kind: "payment_dispute",
                source: "provider",
                reporter: "system",
                reporter_id: None,
                summary: &format!("Refunded in the {} console, outside the app. Check the booking and its payout.", c.gateway),
                photos: json!([]),
            },
        )
        .await?;
    }
    Ok(())
}

/// Settles the checkout a payment event is about. Unknown payments are acknowledged and ignored. Only PortOne
/// (`ask_portone`) is asked for the checkout a foreign id was created with; a Toss orderId is the checkout id.
async fn handle_payment(s: &AppState, payment_id: &str, ask_portone: bool) -> ApiResult<()> {
    let found = if ask_portone {
        locate(s, payment_id).await?
    } else {
        find_by_payment(&s.db, payment_id).await?
    };
    let Some(ck) = found else {
        tracing::info!(payment_id, "payment event for an unknown checkout");
        return Ok(());
    };
    if !ask_portone && ck.status == "paid" {
        return deposit_reversed(s, &ck, payment_id).await;
    }
    match settle(s, &ck, payment_id).await {
        Ok(_) => Ok(()),
        // Retry later: PortOne re-sends on a 5xx.
        Err(e @ (ApiError::Db(_) | ApiError::Internal(_) | ApiError::Unavailable(_))) => Err(e),
        // Business refusals (slot full, amount mismatch, not paid yet) are handled inside settle.
        Err(e) => {
            tracing::warn!(error = %e, checkout = %ck.id, "gateway payment not fulfilled");
            Ok(())
        }
    }
}

/// A paid Toss checkout called about again: 입금 오류 moves a virtual-account deposit from DONE back to
/// WAITING_FOR_DEPOSIT. The booking stays (staff decide); one case per order, and the owner is alerted.
async fn deposit_reversed(s: &AppState, ck: &CheckoutRow, order: &str) -> ApiResult<()> {
    let info = match s.providers.payment(&ck.gateway, order, ck.charge()).await {
        Ok(info) => info,
        Err(ProviderError::NotFound) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if info.status != "VIRTUAL_ACCOUNT_ISSUED" {
        return Ok(());
    }
    let summary = format!(
        "The virtual-account deposit for order {order} was reversed at {TOSS_NAME} (deposit error): the booking is marked paid with no money behind it. Check the payment in the {TOSS_NAME} console and contact the customer."
    );
    let mut tx = s.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("deposit_reversed:{order}"))
        .execute(&mut *tx)
        .await?;
    let open: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM cases WHERE kind = 'payment_dispute' AND summary = $1)")
        .bind(&summary)
        .fetch_one(&mut *tx)
        .await?;
    if !open {
        cases::insert(
            &mut *tx,
            NewCase {
                booking: ck.result_booking_id,
                kind: "payment_dispute",
                source: "provider",
                reporter: "system",
                reporter_id: None,
                summary: &summary,
                photos: json!([]),
            },
        )
        .await?;
    }
    tx.commit().await?;
    tracing::error!(checkout = %ck.id, order, "virtual-account deposit reversed after the booking was paid");
    crate::ops::record(&s.db, "payment_stuck", Err("deposit_reversed")).await;
    Ok(())
}

/// By checkout id or stored payment id first; then by the `checkoutId` the payment was created with.
async fn locate(s: &AppState, payment_id: &str) -> ApiResult<Option<CheckoutRow>> {
    if let Some(ck) = find_by_payment(&s.db, payment_id).await? {
        return Ok(Some(ck));
    }
    match portone::custom_checkout_id(&s.providers, payment_id).await? {
        Some(checkout) => find_by_id(&s.db, checkout).await,
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_id_is_the_provider_id_or_a_body_hash() {
        assert_eq!(event_id("msg_1", b"{}"), "msg_1");
        assert_eq!(event_id("", b"{}"), event_id("", b"{}"));
        assert_ne!(event_id("", br#"{"status":"DONE"}"#), event_id("", br#"{"status":"CANCELED"}"#));
        assert!(event_id("", b"{}").starts_with("sha256:"));
    }

    #[test]
    fn signature_gate_truth_table() {
        assert_eq!(webhook_auth(true, Some("whsec_x")), WebhookAuth::Verify);
        assert_eq!(webhook_auth(false, Some("whsec_x")), WebhookAuth::Verify);
        assert_eq!(webhook_auth(true, None), WebhookAuth::UnsignedDev);
        assert_eq!(webhook_auth(false, None), WebhookAuth::Unavailable);
    }

    #[test]
    fn toss_gate_truth_table() {
        assert_eq!(toss_gate(false, true, true, true), TossGate::Verify, "key and header: verify");
        assert_eq!(toss_gate(true, true, true, true), TossGate::Verify, "dev never skips a real key");
        assert_eq!(toss_gate(false, true, true, false), TossGate::Hint, "no header: a hint, re-read");
        assert_eq!(toss_gate(false, true, false, true), TossGate::Hint, "no key: a hint, re-read");
        assert_eq!(toss_gate(false, false, false, false), TossGate::Unavailable, "prod without a card");
        assert_eq!(toss_gate(false, false, true, true), TossGate::Unavailable);
        assert_eq!(toss_gate(true, false, false, false), TossGate::Hint, "dev accepts unsigned");
    }

    #[test]
    fn toss_events_route_by_status_hint() {
        let changed = |status: &str| json!({ "eventType": "PAYMENT_STATUS_CHANGED", "data": { "orderId": "o-1", "status": status } });
        assert_eq!(toss_event(&changed("DONE")), Some(("o-1".into(), None)));
        assert_eq!(toss_event(&changed("CANCELED")), Some(("o-1".into(), Some("Transaction.Cancelled"))));
        assert_eq!(
            toss_event(&changed("PARTIAL_CANCELED")),
            Some(("o-1".into(), Some("Transaction.PartialCancelled")))
        );
        let deposit = json!({ "createdAt": "x", "secret": "s", "status": "DONE", "transactionKey": "t", "orderId": "o-2" });
        assert_eq!(toss_event(&deposit), Some(("o-2".into(), None)), "DEPOSIT_CALLBACK has no eventType");
        assert_eq!(toss_event(&json!({ "eventType": "BILLING_DELETED", "data": { "billingKey": "k" } })), None);
        assert_eq!(toss_event(&json!({ "eventType": "PAYMENT_STATUS_CHANGED", "data": {} })), None, "no order");
    }

    #[test]
    fn only_payment_state_events_are_handled() {
        assert!(is_payment_event("Transaction.Paid"));
        assert!(is_payment_event("Transaction.VirtualAccountIssued"));
        assert!(!is_payment_event("Transaction.Cancelled"), "a cancellation never settles a checkout");
        assert!(!is_payment_event("Transaction.Ready"));
        assert!(!is_payment_event("BillingKey.Issued"));
        assert!(!is_payment_event(""));
    }

    #[test]
    fn cancellations_are_routed_apart_from_settlement() {
        assert!(is_cancel_event("Transaction.Cancelled"));
        assert!(is_cancel_event("Transaction.PartialCancelled"));
        assert!(!is_cancel_event("Transaction.Paid"));
    }

    #[test]
    fn cancellation_plan_follows_the_provider_balance() {
        let total = 110_000;
        assert_eq!(
            plan_cancellation(total, 30_000, 30_000).expect("plan"),
            CancelPlan::Unchanged,
            "our own refund or a replay"
        );
        assert_eq!(plan_cancellation(total, 0, 0).expect("plan"), CancelPlan::Unchanged);
        assert_eq!(
            plan_cancellation(total, 0, 30_000).expect("plan"),
            CancelPlan::External { status: "partially_refunded" }
        );
        assert_eq!(
            plan_cancellation(total, 30_000, total).expect("plan"),
            CancelPlan::External { status: "refunded" }
        );
        assert!(
            plan_cancellation(total, 0, -1).is_err(),
            "a missing amount is retried, never recorded as nothing"
        );
        assert!(plan_cancellation(total, 0, total + 1).is_err(), "more than was paid is refused");
    }
}
