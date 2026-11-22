// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Checkout recovery and listing: the reconciler pass (unfinished checkouts and queued refunds)
//! and the customer and console checkout lists. Settlement itself lives in `checkouts`.

use super::checkouts::{CHECKOUT_COLS, CheckoutRow, apply, find_by_id, refund_late, virtual_account_view};
use crate::{
    auth::{Customer, Staff},
    contract::{cursor_part, decode_cursor, encode_cursor, page_limit, paged, ts_key},
    error::{ApiError, ApiResult},
    ops,
    providers::{
        Gateway, ProviderError, TOSS, TOSS_BILLING,
        portone::{self, ProviderPayment},
        toss_payments::{self, Mid},
    },
    state::AppState,
    util::{QueuedRefund, REFUND_COLS, REFUND_LEASE_MINS, Refund, retry_refund},
};
use axum::{
    extract::{Query, State},
    response::Response,
};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Checkouts re-checked with the provider once per cooldown, and only after this much time.
const RECON_BATCH: i64 = 20;
const RECON_COOLDOWN_SECS: i64 = 300;
const RECON_MIN_AGE_SECS: i64 = 120;
pub(crate) const RECON_MAX_AGE_HOURS: i64 = 24;
/// The stale sweep stops at this age: older unfinished checkouts are left alone so the provider
/// is not re-polled forever. Anything older than this needs the owner to look at it by hand.
const RECON_STALE_MAX_DAYS: i64 = 30;
const REFUND_BATCH: i64 = 20;
/// PortOne states that mean money was received for a payment (paid, or paid and later cancelled).
const RECEIVED: [&str; 3] = ["PAID", "PARTIAL_CANCELLED", "CANCELLED"];
const DIFF_PAGE: u32 = 100;
/// Caps the daily pull at 20k payments a day. Hitting it fails the pass instead of diffing half a day.
const DIFF_MAX_PAGES: u32 = 200;
const DIFF_STATE_KEY: &str = "payment_diff";
/// The diff waits until this KST hour so webhooks and deposits for the closed day have landed.
const DIFF_SETTLE_HOUR_KST: u32 = 3;
/// Both sides of the diff window are widened by this much, so a payment created just before
/// midnight and listed just after it (or the reverse) is matched on both sides.
const DIFF_GRACE_MINS: i64 = 60;

/// One reconciler pass: retry due refunds, sweep pending checkouts older than a day to 'expired',
/// then claim a batch of recent unfinished checkouts and settle each against the provider.
/// Returns how many refunds and checkouts were checked.
pub async fn reconcile_pass(s: &AppState) -> ApiResult<usize> {
    let refunds = drain_refunds(s).await?;
    let now = Utc::now();
    sqlx::query(
        "UPDATE checkouts SET status = 'expired' WHERE id IN (
            SELECT id FROM checkouts WHERE status = 'pending' AND created_at < $1
            LIMIT 200 FOR UPDATE SKIP LOCKED)",
    )
    .bind(now - Duration::hours(RECON_MAX_AGE_HOURS))
    .execute(&s.db)
    .await?;
    // Claim: the stamp is the lease. SKIP LOCKED keeps two replicas off the same rows.
    let claimed: Vec<CheckoutRow> = sqlx::query_as(&format!(
        "UPDATE checkouts SET reconciled_at = now() WHERE id IN (
            SELECT id FROM checkouts
            WHERE status IN ('pending','awaiting_deposit','expired') AND created_at < $1 AND created_at > $2
              AND (reconciled_at IS NULL OR reconciled_at < $3)
            ORDER BY reconciled_at NULLS FIRST, created_at
            LIMIT $4 FOR UPDATE SKIP LOCKED)
         RETURNING {CHECKOUT_COLS}"
    ))
    .bind(now - Duration::seconds(RECON_MIN_AGE_SECS))
    .bind(now - Duration::hours(RECON_MAX_AGE_HOURS))
    .bind(now - Duration::seconds(RECON_COOLDOWN_SECS))
    .bind(RECON_BATCH)
    .fetch_all(&s.db)
    .await?;
    for ck in &claimed {
        reconcile_one(s, ck).await;
    }
    Ok(refunds + claimed.len())
}

async fn reconcile_one(s: &AppState, ck: &CheckoutRow) {
    // Payment id = checkout id, as the app pays with it.
    let payment_id = ck.id.to_string();
    let info = match s.providers.payment(&ck.gateway, &payment_id, ck.charge()).await {
        Ok(info) => Some(info),
        Err(ProviderError::NotFound) => None,
        Err(e) => {
            tracing::warn!(error = %e, checkout = %ck.id, "reconcile: provider lookup failed");
            return;
        }
    };
    if let Err(e) = apply(s, ck, &payment_id, info.as_ref()).await {
        tracing::debug!(error = %e, checkout = %ck.id, "reconcile: checkout left as it is");
    }
}

/// Retries due compensating refunds (lease + SKIP LOCKED, like the checkout claim). A refund that
/// lands moves its checkout from 'refund_pending' to 'refunded' and tells the customer, once.
/// Returns how many were tried.
pub async fn drain_refunds(s: &AppState) -> ApiResult<usize> {
    let due: Vec<QueuedRefund> = sqlx::query_as(&format!(
        "UPDATE refund_queue SET next_at = now() + make_interval(mins => $2) WHERE id IN (
            SELECT id FROM refund_queue WHERE done_at IS NULL AND NOT review_required AND next_at <= now()
            ORDER BY next_at LIMIT $1 FOR UPDATE SKIP LOCKED)
         RETURNING {REFUND_COLS}"
    ))
    .bind(REFUND_BATCH)
    .bind(REFUND_LEASE_MINS)
    .fetch_all(&s.db)
    .await?;
    for r in &due {
        if retry_refund(s, r).await == Refund::Done
            && let Some(id) = r.checkout_id
            && let Err(e) = refunded_later(s, id).await
        {
            tracing::error!(error = %e, checkout = %id, "refund landed but the checkout still says refund_pending");
        }
    }
    Ok(due.len())
}

async fn refunded_later(s: &AppState, id: Uuid) -> ApiResult<()> {
    match find_by_id(&s.db, id).await? {
        Some(ck) => refund_late(s, &ck, None, Refund::Done).await,
        None => Ok(()),
    }
}

/// Checkouts with a provider payment that stayed unfinished for more than a day (the 24 h sweep
/// above never looks at them). Each one that PortOne reports as paid raises the owner's
/// `payment_stuck` alert and gets one more fulfil attempt. Returns how many were checked.
pub async fn reconcile_stale(s: &AppState) -> ApiResult<usize> {
    let now = Utc::now();
    let claimed: Vec<CheckoutRow> = sqlx::query_as(&format!(
        "UPDATE checkouts SET reconciled_at = now() WHERE id IN (
            SELECT id FROM checkouts
            WHERE provider_payment_id IS NOT NULL AND status IN ('pending','awaiting_deposit','expired')
              AND created_at < $1 AND created_at > $2 AND (reconciled_at IS NULL OR reconciled_at < $3)
            ORDER BY reconciled_at NULLS FIRST, created_at
            LIMIT $4 FOR UPDATE SKIP LOCKED)
         RETURNING {CHECKOUT_COLS}"
    ))
    .bind(now - Duration::hours(RECON_MAX_AGE_HOURS))
    .bind(now - Duration::days(RECON_STALE_MAX_DAYS))
    .bind(now - Duration::seconds(RECON_COOLDOWN_SECS))
    .bind(RECON_BATCH)
    .fetch_all(&s.db)
    .await?;
    for ck in &claimed {
        reconcile_stale_one(s, ck).await;
    }
    Ok(claimed.len())
}

async fn reconcile_stale_one(s: &AppState, ck: &CheckoutRow) {
    let Some(payment_id) = ck.provider_payment_id.as_deref() else {
        return;
    };
    let info = match s.providers.payment(&ck.gateway, payment_id, ck.charge()).await {
        Ok(info) => info,
        Err(ProviderError::NotFound) => return,
        Err(e) => {
            tracing::warn!(error = %e, checkout = %ck.id, "stale reconcile: provider lookup failed");
            return;
        }
    };
    if info.status == "PAID" {
        ops::record(&s.db, "payment_stuck", Err("paid_unfulfilled_after_day")).await;
    }
    if let Err(e) = apply(s, ck, payment_id, Some(&info)).await {
        tracing::debug!(error = %e, checkout = %ck.id, "stale reconcile: checkout left as it is");
    }
}

/// One provider payment against the local record of the same id. Pure, so the rules are testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPayment {
    pub id: String,
    /// Money was received locally: checkout paid or refunded, or a payment row exists.
    pub received: bool,
    pub amount: i64,
}

/// One disagreement between PortOne and local records. `code` is the ops failure code the owner alert counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub id: String,
    pub code: &'static str,
}

/// Matches provider payments to local records by payment id and reports every disagreement.
/// A payment PortOne shows as paid but we never recorded is a mismatch. So is a received local
/// payment PortOne does not list, a differing amount, or a received flag that disagrees.
pub fn diff_payments(provider: &[ProviderPayment], local: &[LocalPayment]) -> Vec<Mismatch> {
    let by_id: HashMap<&str, &LocalPayment> = local.iter().map(|l| (l.id.as_str(), l)).collect();
    let listed: HashSet<&str> = provider.iter().map(|p| p.id.as_str()).collect();
    let mut out = Vec::new();
    for p in provider {
        let received = RECEIVED.contains(&p.status.as_str());
        let code = match by_id.get(p.id.as_str()) {
            None if p.status == "PAID" => Some("missing_locally"),
            None => None,
            Some(l) if l.received != received => Some("status_differs"),
            Some(l) if received && l.amount != p.total => Some("amount_differs"),
            Some(_) => None,
        };
        out.extend(code.map(|code| Mismatch { id: p.id.clone(), code }));
    }
    out.extend(local.iter().filter(|l| l.received && !listed.contains(l.id.as_str())).map(|l| Mismatch {
        id: l.id.clone(),
        code: "missing_at_provider",
    }));
    out
}

/// The UTC range `[start, end)` of one KST calendar day.
pub fn kst_day_bounds(day: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    // KST is UTC+9 with no daylight time, so the day starts at 09:00 UTC of the day before.
    let start = day.and_time(NaiveTime::MIN).and_utc() - Duration::hours(9);
    (start, start + Duration::days(1))
}

/// The KST day the daily pass reconciles at `now`: always the day before today in KST.
fn previous_kst_day(now: DateTime<Utc>) -> NaiveDate {
    (now + Duration::hours(9)).date_naive() - Duration::days(1)
}

/// The window both sides are read over: the KST day widened by `DIFF_GRACE_MINS` at each end.
pub fn diff_window(day: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let (from, until) = kst_day_bounds(day);
    let grace = Duration::minutes(DIFF_GRACE_MINS);
    (from - grace, until + grace)
}

/// True when the day's pass should run at `now`: past the settle hour, not already done, and not
/// attempted within `interval`.
fn diff_due(now: DateTime<Utc>, day: NaiveDate, st: &DiffState, interval: Duration) -> bool {
    let kst = now + Duration::hours(9);
    kst.hour() >= DIFF_SETTLE_HOUR_KST && st.done_day != Some(day) && !st.attempted_at.is_some_and(|t| now - t < interval)
}

/// Lists every PortOne payment created in the window, following the pages to the end.
async fn fetch_provider_day(s: &AppState, from: DateTime<Utc>, until: DateTime<Utc>) -> ApiResult<Vec<ProviderPayment>> {
    let (from, until) = (from.to_rfc3339(), until.to_rfc3339());
    let mut all = Vec::new();
    for page in 0..DIFF_MAX_PAGES {
        let (items, total) = portone::list_payments(&s.providers, &from, &until, page, DIFF_PAGE).await?;
        let last = items.is_empty();
        all.extend(items);
        if last || i64::try_from(all.len()).unwrap_or(i64::MAX) >= total {
            return Ok(all);
        }
    }
    Err(ApiError::Internal("payment list exceeds the page cap".into()))
}

/// Lists every transaction of one Toss MID in the window (as KST wall time, which Toss takes), one row per order.
async fn fetch_toss_day(s: &AppState, mid: Mid, from: DateTime<Utc>, until: DateTime<Utc>) -> ApiResult<Vec<ProviderPayment>> {
    let kst = |t: DateTime<Utc>| (t + Duration::hours(9)).naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string();
    Ok(toss_payments::transactions(&s.providers, mid, &kst(from), &kst(until)).await?)
}

/// Local records for the window on one gateway leg: checkouts of `checkout_gateway` with a provider payment id,
/// plus payment rows of `providers` that no checkout covers. One entry per payment id. Without the gateway filter
/// the PortOne diff would report every Toss checkout as missing at PortOne.
async fn local_payments(
    db: &PgPool,
    from: DateTime<Utc>,
    until: DateTime<Utc>,
    providers: &[&str],
    checkout_gateway: Option<&str>,
) -> ApiResult<Vec<LocalPayment>> {
    let checkouts: Vec<(String, bool, i64)> = sqlx::query_as(
        "SELECT provider_payment_id, status IN ('paid','refund_pending','refunded'), amount - points
         FROM checkouts WHERE provider_payment_id IS NOT NULL AND created_at >= $1 AND created_at < $2 AND gateway = $3",
    )
    .bind(from)
    .bind(until)
    .bind(checkout_gateway)
    .fetch_all(db)
    .await?;
    let payments: Vec<(String, bool, i64)> = sqlx::query_as(
        "SELECT provider_tx_id, status IN ('paid','partially_refunded','refunded'), amount
         FROM payments WHERE provider = ANY($3) AND provider_tx_id IS NOT NULL AND created_at >= $1 AND created_at < $2",
    )
    .bind(from)
    .bind(until)
    .bind(providers)
    .fetch_all(db)
    .await?;
    let mut seen = HashSet::new();
    Ok(checkouts
        .into_iter()
        .chain(payments)
        .filter(|(id, _, _)| seen.insert(id.clone()))
        .map(|(id, received, amount)| LocalPayment { id, received, amount })
        .collect())
}

/// One diff per configured gateway leg: PortOne, the Toss window MID (its checkouts and payments), and the Toss
/// billing MID (saved-card charges only). A failing leg is reported and keeps the others' mismatches; the bool says
/// whether every leg ran.
async fn diff_day(s: &AppState, day: NaiveDate) -> (Vec<Mismatch>, bool) {
    let (from, until) = diff_window(day);
    let integ = s.providers.integ();
    let mut found = Vec::new();
    let mut complete = true;
    let mut leg = |name: &'static str, result: ApiResult<Vec<Mismatch>>| match result {
        Ok(m) => found.extend(m),
        Err(e) => {
            tracing::error!(%day, leg = name, error = %e, "payment diff leg failed; the day is tried again");
            complete = false;
        }
    };
    if integ.portone.api_secret.is_some() {
        let r = async {
            let provider = fetch_provider_day(s, from, until).await?;
            let local = local_payments(&s.db, from, until, &["portone"], Some(Gateway::Portone.code())).await?;
            Ok(diff_leg(day, "portone", &provider, &local))
        };
        leg("portone", r.await);
    }
    if let Some(toss) = &integ.toss_payments {
        let r = async {
            let provider = fetch_toss_day(s, Mid::Window, from, until).await?;
            let local = local_payments(&s.db, from, until, &[TOSS], Some(TOSS)).await?;
            Ok(diff_leg(day, TOSS, &provider, &local))
        };
        leg(TOSS, r.await);
        if toss.billing.is_some() {
            let r = async {
                let provider = fetch_toss_day(s, Mid::Billing, from, until).await?;
                let local = local_payments(&s.db, from, until, &[TOSS_BILLING], None).await?;
                Ok(diff_leg(day, TOSS_BILLING, &provider, &local))
            };
            leg(TOSS_BILLING, r.await);
        }
    }
    (found, complete)
}

fn diff_leg(day: NaiveDate, leg: &str, provider: &[ProviderPayment], local: &[LocalPayment]) -> Vec<Mismatch> {
    let found = diff_payments(provider, local);
    tracing::info!(%day, leg, listed = provider.len(), mismatches = found.len(), "payment diff pass");
    found
}

/// Pass state in `settings`: the last KST day fully reconciled, and when an attempt last started
/// (so a failing day retries after the interval, not every tick).
#[derive(Debug, Default, Serialize, Deserialize)]
struct DiffState {
    done_day: Option<NaiveDate>,
    attempted_at: Option<DateTime<Utc>>,
}

async fn load_diff_state(db: &PgPool) -> ApiResult<DiffState> {
    let raw: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(DIFF_STATE_KEY)
        .fetch_optional(db)
        .await?;
    // An unreadable state reads as "never ran": at worst the day is diffed again.
    Ok(raw.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
}

async fn save_diff_state(db: &PgPool, st: &DiffState) -> ApiResult<()> {
    let value = serde_json::to_value(st).map_err(|e| ApiError::Internal(e.to_string()))?;
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(DIFF_STATE_KEY)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}

/// Daily PortOne pull: diffs the previous KST day against local payments and checkouts. Each
/// mismatch is logged as a `payment_mismatch` failure, which the owner's alert rule counts.
/// Runs at most once per day after `DIFF_SETTLE_HOUR_KST`, retries after `payment_diff_interval_secs`
/// when a pass fails, and returns how many mismatches it found (0 when nothing was due).
pub async fn payment_diff_pass(s: &AppState) -> ApiResult<usize> {
    payment_diff_at(s, Utc::now()).await
}

/// `payment_diff_pass` as of `now`, so the schedule can be tested without the wall clock.
pub async fn payment_diff_at(s: &AppState, now: DateTime<Utc>) -> ApiResult<usize> {
    let day = previous_kst_day(now);
    let mut st = load_diff_state(&s.db).await?;
    let interval = Duration::seconds(i64::try_from(s.config.payment_diff_interval_secs).unwrap_or(i64::MAX));
    if !diff_due(now, day, &st, interval) {
        return Ok(0);
    }
    st.attempted_at = Some(now);
    save_diff_state(&s.db, &st).await?;
    let (found, complete) = diff_day(s, day).await;
    for m in &found {
        ops::record_failure(&s.db, "payment_mismatch", m.code).await?;
    }
    if !complete {
        // Counted like a mismatch, so the owner's alert sees a gateway the diff cannot read. A retry may record the
        // other legs' mismatches again; the alert is a threshold.
        ops::record_failure(&s.db, "payment_mismatch", "diff_leg_failed").await?;
        return Ok(found.len());
    }
    st.done_day = Some(day);
    save_diff_state(&s.db, &st).await?;
    Ok(found.len())
}

#[derive(Deserialize)]
pub(super) struct ListQuery {
    status: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct CheckoutView {
    id: Uuid,
    kind: String,
    booking_id: Option<Uuid>,
    method: String,
    amount: i64,
    #[serde(skip)]
    points: i64,
    status: String,
    virtual_account: Option<Value>,
    result_booking_id: Option<Uuid>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

/// Newest first; keyset cursor over (created_at, id). `status` must be a known checkout status.
pub(super) async fn list(State(s): State<AppState>, Customer(a): Customer, Query(q): Query<ListQuery>) -> ApiResult<Response> {
    let limit = page_limit(q.limit)?;
    let status = checked_status(q.status.as_deref())?;
    let (ts, id) = keyset(q.cursor.as_deref())?;
    sqlx::query("UPDATE checkouts SET status = 'expired' WHERE user_id = $1 AND status = 'pending' AND expires_at < now()")
        .bind(a.id)
        .execute(&s.db)
        .await?;
    let rows: Vec<CheckoutView> = sqlx::query_as(
        "SELECT id, kind, booking_id, method, amount, points, status, virtual_account, result_booking_id, expires_at, created_at
         FROM checkouts WHERE user_id = $1 AND ($2::text IS NULL OR status = $2)
           AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4)) ORDER BY created_at DESC, id DESC LIMIT $5",
    )
    .bind(a.id)
    .bind(status)
    .bind(ts)
    .bind(id)
    .bind(limit)
    .fetch_all(&s.db)
    .await?;
    // Stored as PortOne sent it; shown like the confirm answer, with what is left to deposit.
    let rows: Vec<CheckoutView> = rows
        .into_iter()
        .map(|r| CheckoutView {
            virtual_account: r.virtual_account.as_ref().map(|m| virtual_account_view(m, r.amount - r.points)),
            ..r
        })
        .collect();
    Ok(paged(rows, limit, |r| encode_cursor(&[&ts_key(r.created_at), &r.id.to_string()])))
}

/// Every status a checkout can hold; list filters outside this set are a client error.
const STATUSES: [&str; 8] = [
    "pending",
    "awaiting_deposit",
    "paid",
    "refund_pending",
    "refunded",
    "expired",
    "failed",
    "cancelled",
];

fn checked_status(status: Option<&str>) -> ApiResult<Option<String>> {
    match status {
        Some(s) if !STATUSES.contains(&s) => Err(ApiError::BadRequest("invalid_status")),
        other => Ok(other.map(str::to_owned)),
    }
}

fn keyset(cursor: Option<&str>) -> ApiResult<(Option<DateTime<Utc>>, Option<Uuid>)> {
    Ok(match decode_cursor(cursor, 2)? {
        Some(p) => (Some(cursor_part(&p[0])?), Some(cursor_part(&p[1])?)),
        None => (None, None),
    })
}

#[derive(Serialize, sqlx::FromRow)]
struct AdminCheckoutView {
    id: Uuid,
    kind: String,
    method: String,
    amount: i64,
    status: String,
    created_at: DateTime<Utc>,
    booking_id: Option<Uuid>,
    result_booking_id: Option<Uuid>,
    user_id: Uuid,
}

/// Console view of checkouts, newest first. Staff only.
pub(super) async fn admin_list(State(s): State<AppState>, _staff: Staff, Query(q): Query<ListQuery>) -> ApiResult<Response> {
    let limit = page_limit(q.limit)?;
    let status = checked_status(q.status.as_deref())?;
    let (ts, id) = keyset(q.cursor.as_deref())?;
    let rows: Vec<AdminCheckoutView> = sqlx::query_as(
        "SELECT id, kind, method, amount, status, created_at, booking_id, result_booking_id, user_id
         FROM checkouts WHERE ($1::text IS NULL OR status = $1)
           AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3)) ORDER BY created_at DESC, id DESC LIMIT $4",
    )
    .bind(status)
    .bind(ts)
    .bind(id)
    .bind(limit)
    .fetch_all(&s.db)
    .await?;
    Ok(paged(rows, limit, |r| encode_cursor(&[&ts_key(r.created_at), &r.id.to_string()])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refund_pending_is_a_listable_status() {
        assert_eq!(checked_status(Some("refund_pending")).expect("known"), Some("refund_pending".into()));
        assert!(checked_status(Some("nope")).is_err());
    }

    fn provider(id: &str, status: &str, total: i64) -> ProviderPayment {
        ProviderPayment {
            id: id.into(),
            status: status.into(),
            total,
        }
    }

    fn local(id: &str, received: bool, amount: i64) -> LocalPayment {
        LocalPayment {
            id: id.into(),
            received,
            amount,
        }
    }

    fn codes(m: &[Mismatch]) -> Vec<(&str, &str)> {
        m.iter().map(|x| (x.id.as_str(), x.code)).collect()
    }

    #[test]
    fn matching_payments_produce_no_mismatch() {
        let p = [provider("a", "PAID", 9000), provider("b", "VIRTUAL_ACCOUNT_ISSUED", 5000)];
        let l = [local("a", true, 9000), local("b", false, 5000)];
        assert!(diff_payments(&p, &l).is_empty());
    }

    #[test]
    fn paid_at_provider_but_unknown_locally_is_flagged() {
        let p = [provider("ghost", "PAID", 7000)];
        assert_eq!(codes(&diff_payments(&p, &[])), vec![("ghost", "missing_locally")]);
    }

    #[test]
    fn unknown_failed_provider_payment_is_not_flagged() {
        let p = [provider("dropped", "FAILED", 7000)];
        assert!(diff_payments(&p, &[]).is_empty());
    }

    #[test]
    fn local_receipt_without_provider_payment_is_flagged() {
        let l = [local("lost", true, 4000)];
        assert_eq!(codes(&diff_payments(&[], &l)), vec![("lost", "missing_at_provider")]);
    }

    #[test]
    fn amount_difference_is_flagged_once() {
        let p = [provider("a", "PAID", 9000)];
        let l = [local("a", true, 8500)];
        assert_eq!(codes(&diff_payments(&p, &l)), vec![("a", "amount_differs")]);
    }

    #[test]
    fn received_on_one_side_only_is_a_status_difference() {
        let p = [provider("a", "PAID", 9000)];
        let l = [local("a", false, 9000)];
        assert_eq!(codes(&diff_payments(&p, &l)), vec![("a", "status_differs")]);
    }

    #[test]
    fn later_cancellation_still_counts_as_received() {
        let p = [provider("a", "CANCELLED", 9000)];
        let l = [local("a", true, 9000)];
        assert!(diff_payments(&p, &l).is_empty());
    }

    #[test]
    fn kst_day_bounds_run_from_kst_midnight_to_midnight() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 16).expect("date");
        assert_eq!(kst_day_bounds(now).0.to_rfc3339(), "2026-11-15T15:00:00+00:00");
        assert_eq!(kst_day_bounds(now).1.to_rfc3339(), "2026-11-16T15:00:00+00:00");
    }

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).expect("rfc3339").with_timezone(&Utc)
    }

    #[test]
    fn diff_window_pads_both_ends_by_the_grace_period() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 15).expect("date");
        let (from, until) = diff_window(day);
        assert_eq!(from, utc("2026-11-14T14:00:00Z"));
        assert_eq!(until, utc("2026-11-15T16:00:00Z"));
    }

    #[test]
    fn payment_just_before_kst_midnight_is_in_both_windows() {
        // Created 23:59 KST on the 15th, listed 00:01 KST on the 16th: the 15th's padded window
        // covers both timestamps, so the two sides agree.
        let day = NaiveDate::from_ymd_opt(2026, 11, 15).expect("date");
        let (_, until) = diff_window(day);
        let created = utc("2026-11-15T14:59:00Z");
        let listed = utc("2026-11-15T15:01:00Z");
        assert!(created < until && listed < until);
    }

    #[test]
    fn diff_waits_for_the_settle_hour_kst() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 15).expect("date");
        let fresh = DiffState::default();
        let hour = Duration::hours(1);
        assert!(!diff_due(utc("2026-11-16T17:59:00Z"), day, &fresh, hour), "02:59 KST is too early");
        assert!(diff_due(utc("2026-11-16T18:00:00Z"), day, &fresh, hour), "03:00 KST runs");
    }

    #[test]
    fn diff_skips_a_done_day_and_a_recent_attempt() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 15).expect("date");
        let now = utc("2026-11-16T20:00:00Z");
        let done = DiffState {
            done_day: Some(day),
            attempted_at: None,
        };
        assert!(!diff_due(now, day, &done, Duration::hours(1)));
        let tried = DiffState {
            done_day: None,
            attempted_at: Some(now - Duration::minutes(10)),
        };
        assert!(!diff_due(now, day, &tried, Duration::hours(1)), "failed attempts wait the interval");
        assert!(diff_due(now, day, &tried, Duration::minutes(5)));
    }

    #[test]
    fn previous_day_is_the_closed_kst_day() {
        assert_eq!(
            previous_kst_day(utc("2026-11-16T18:00:00Z")),
            NaiveDate::from_ymd_opt(2026, 11, 16).expect("date")
        );
        assert_eq!(
            previous_kst_day(utc("2026-11-16T14:59:00Z")),
            NaiveDate::from_ymd_opt(2026, 11, 15).expect("date")
        );
    }
}
