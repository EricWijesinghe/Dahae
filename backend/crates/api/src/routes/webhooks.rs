// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Provider callbacks. The Standard Webhooks signature is the gate. Without a configured
//! secret only dev mode accepts unsigned events (smoke tests); production refuses them.
//! Events never move money on their own: the payment is re-read from the provider.

use super::bookings::Actor;
use super::cases::{self, NewCase};
use super::checkouts::{CheckoutRow, find_by_id, find_by_payment, settle};
use super::tax_invoices;
use crate::{
    error::{ApiError, ApiResult},
    providers::{
        ProviderError,
        portone::{self, WebhookError, verify_webhook},
        toss_webhook,
    },
    state::AppState,
};
use axum::{Json, Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

const PAYMENT_EVENTS: [&str; 2] = ["Transaction.Paid", "Transaction.VirtualAccountIssued"];
/// Cancellations PortOne reports: a refund made in its console, or a partial one. They never settle a checkout.
const CANCEL_EVENTS: [&str; 2] = ["Transaction.Cancelled", "Transaction.PartialCancelled"];

pub fn routes() -> Router<AppState> {
    Router::new().route("/webhooks/portone", post(portone)).route("/webhooks/toss", post(toss))
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
    crate::payout_sync::kick(s);
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
    dispatch(&s, &event).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Hands a verified event to the handler that owns it. Unrelated events are acknowledged and ignored.
async fn dispatch(s: &AppState, event: &Event) -> ApiResult<()> {
    let Some(payment_id) = event.data.payment_id.as_deref() else {
        return Ok(());
    };
    if is_cancel_event(&event.kind) {
        sync_cancellation(s, payment_id, &event.kind).await
    } else if is_payment_event(&event.kind) {
        handle_payment(s, payment_id).await
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
async fn sync_cancellation(s: &AppState, payment_id: &str, kind: &str) -> ApiResult<()> {
    let mut tx = s.db.begin().await?;
    // Booking, then payment: the order apply_cancel and the refund paths lock in, so the two cannot deadlock.
    let booking: Option<Uuid> = sqlx::query_scalar(
        "SELECT b.id FROM bookings b JOIN payments p ON p.booking_id = b.id WHERE p.provider = 'portone' AND p.provider_tx_id = $1 FOR UPDATE OF b",
    )
    .bind(payment_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(booking) = booking else {
        tracing::info!(payment_id, "portone cancellation for a payment with no booking");
        return Ok(());
    };
    // Locked until commit: a refund the app is making right now is recorded before this reads the amount.
    let row: Option<(Uuid, i64, i64)> =
        sqlx::query_as("SELECT id, amount, refunded_amount FROM payments WHERE provider = 'portone' AND provider_tx_id = $1 FOR UPDATE")
            .bind(payment_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((payment, amount, recorded)) = row else {
        return Ok(());
    };
    let cancelled = match s.providers.payment(payment_id, amount).await {
        Ok(info) => info.cancelled,
        Err(ProviderError::NotFound) => {
            tracing::warn!(payment_id, "portone cancellation for a payment PortOne does not know");
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
                summary: "Refunded in the PortOne console, outside the app. Check the booking and its payout.",
                photos: json!([]),
            },
        )
        .await?;
    }
    Ok(())
}

/// Settles the checkout a payment event is about. Unknown payments are acknowledged and ignored.
async fn handle_payment(s: &AppState, payment_id: &str) -> ApiResult<()> {
    let Some(ck) = locate(s, payment_id).await? else {
        tracing::info!(payment_id, "portone event for an unknown checkout");
        return Ok(());
    };
    match settle(s, &ck, payment_id).await {
        Ok(_) => Ok(()),
        // Retry later: PortOne re-sends on a 5xx.
        Err(e @ (ApiError::Db(_) | ApiError::Internal(_) | ApiError::Unavailable(_))) => Err(e),
        // Business refusals (slot full, amount mismatch, not paid yet) are handled inside settle.
        Err(e) => {
            tracing::warn!(error = %e, checkout = %ck.id, "portone payment not fulfilled");
            Ok(())
        }
    }
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
    fn signature_gate_truth_table() {
        assert_eq!(webhook_auth(true, Some("whsec_x")), WebhookAuth::Verify);
        assert_eq!(webhook_auth(false, Some("whsec_x")), WebhookAuth::Verify);
        assert_eq!(webhook_auth(true, None), WebhookAuth::UnsignedDev);
        assert_eq!(webhook_auth(false, None), WebhookAuth::Unavailable);
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
