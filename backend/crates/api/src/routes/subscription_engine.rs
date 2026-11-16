//! Subscription engine, one pass every few minutes:
//! 1. plans visits up to `HORIZON_DAYS` ahead for active subscriptions (preferred arrival window first, then any free one);
//! 2. charges each planned visit that is due tomorrow: booking and charge in one transaction, so a declined card leaves no booking;
//! 3. a failed charge retries after `RETRY_AFTER_HOURS`; `PAUSE_AFTER_FAILURES` in a row pauses the subscription;
//! 4. planned visits whose day has arrived without a charge become `missed`.

use super::{
    booking_core::{BookingBody, Prepared, after_booking_created, counts_from_row, fixed_peak_bps, insert_paid, lock_slot, prepare, record_payment},
    partner_jobs::VISIBLE,
};
use crate::{
    error::{ApiError, ApiResult},
    providers::ProviderError,
    realtime::{Recipient, notify},
    schedule::{clash, lock_schedule},
    state::AppState,
    util::{KST_TODAY, best_effort, lock_and_count_slot, refund_or_alert, today_kst},
};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use domain::{
    DomainError, Won,
    pricing::{PriceBreakdown, price_booking_with},
    schedule::{STANDARD_SLOTS, check_capacity},
    subscription::{Cadence, HORIZON_DAYS, Retry, after_failure, visit_dates, visit_price},
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::time::Duration as StdDuration;
use tokio::task::JoinHandle;
use uuid::Uuid;

const PASS_MINUTES: i64 = 5;
const PASS_EVERY: StdDuration = StdDuration::from_secs(PASS_MINUTES as u64 * 60);
const DUE_BATCH: i64 = 200;
/// What a dev lookup reports as captured for its mock ids; PortOne itself reports the real amount.
const DEV_LOOKUP_TOTAL: i64 = 1;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PassReport {
    pub planned: usize,
    pub booked: usize,
    pub failed: usize,
    pub missed: usize,
}

#[derive(sqlx::FromRow)]
struct Due {
    visit_id: Uuid,
    sub_id: Uuid,
    user_id: Uuid,
    visit_date: NaiveDate,
    start_hour: i16,
}

#[derive(sqlx::FromRow)]
struct SubRow {
    user_id: Uuid,
    service_code: String,
    unit_index: Option<i16>,
    unit_counts: Option<Vec<i16>>,
    address_id: Uuid,
    payment_method_id: Uuid,
    preferred_pro_id: Option<Uuid>,
    discount_percent: i16,
}

/// Why a charge did not book. Declines and blocked attempts both retry; only a decline is a card problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Declined(String),
    /// The slot filled after the charge was sent. Not a card problem: it is refunded, retried, and never pauses.
    Full,
    /// PortOne captured another amount than today's price. Refunded and retried; not a card problem either.
    Repriced,
    Blocked,
}

impl Failure {
    /// Only a decline or a refused booking counts toward the pause: a full slot or a price change says nothing about the card.
    fn counts(&self) -> bool {
        matches!(self, Failure::Declined(_) | Failure::Blocked)
    }

    /// The customer notice as (kind, title, body). Each kind has this one text, owner-editable (`i18n::EVENTS`).
    fn notice(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            Failure::Declined(_) => ("subscription.payment_failed", "정기 청소 결제가 실패했어요", "카드 정보를 확인해 주세요"),
            Failure::Full => ("subscription.slot_full", "정기 청소 시간이 가득 찼어요", "결제된 금액은 환불해 드려요"),
            Failure::Repriced => ("subscription.price_changed", "정기 청소 금액이 바뀌었어요", "먼저 결제된 금액은 환불해 드려요"),
            Failure::Blocked => (
                "subscription.visit_blocked",
                "정기 청소 예약을 진행하지 못했어요",
                "주소와 결제 수단을 확인해 주세요",
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Attempt {
    Booked(Uuid),
    Failed(Failure),
    /// The charge outcome is unknown; the stored payment id is checked again on the next pass.
    Pending,
    /// The subscription was paused or cancelled, or the visit moved, since the pass started.
    Skipped,
}

pub fn spawn(state: AppState) -> JoinHandle<()> {
    let mut stop = state.hub.drain_signal();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(PASS_EVERY);
        while crate::lifecycle::next_tick(&mut tick, &mut stop).await {
            crate::jobs::run(&state, "subscription_engine", pass(&state)).await;
        }
    })
}

/// One full pass. Each step logs its own errors so a bad row cannot stop the others.
pub async fn pass(s: &AppState) -> ApiResult<PassReport> {
    pass_at(s, Utc::now()).await
}

/// [`pass`] with the clock that schedules retries passed in, so the end of the charge window can be tested.
#[doc(hidden)]
pub async fn pass_at(s: &AppState, now: DateTime<Utc>) -> ApiResult<PassReport> {
    let planned = plan(s).await?;
    let (booked, failed) = charge_due(s, now).await?;
    let missed = expire(s).await?;
    Ok(PassReport {
        planned,
        booked,
        failed,
        missed,
    })
}

/// Plans visits for every active subscription inside the horizon. Returns how many were added.
async fn plan(s: &AppState) -> ApiResult<usize> {
    let subs: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM subscriptions WHERE status = 'active'").fetch_all(&s.db).await?;
    let mut added = 0;
    for (sub,) in subs {
        match plan_subscription(s, sub).await {
            Ok(n) => added += n,
            Err(e) => tracing::warn!(error = %e, subscription = %sub, "planning a subscription failed"),
        }
    }
    Ok(added)
}

/// Plans one active subscription's visits inside the horizon. Used on create and on every pass.
pub async fn plan_subscription(s: &AppState, sub: Uuid) -> ApiResult<usize> {
    let row: Option<(i16, NaiveDate, i16, Uuid)> =
        sqlx::query_as("SELECT cadence_weeks, anchor_date, start_hour, user_id FROM subscriptions WHERE id = $1 AND status = 'active'")
            .bind(sub)
            .fetch_optional(&s.db)
            .await?;
    let Some((weeks, anchor, hour, user)) = row else {
        return Ok(0);
    };
    let cadence = Cadence::from_weeks(i64::from(weeks))?;
    let (from, until) = (today_kst() + Duration::days(1), today_kst() + Duration::days(HORIZON_DAYS));
    let mut added = 0;
    for date in visit_dates(anchor, cadence, from, until) {
        if plan_visit(s, sub, user, date, hour as u8).await? {
            added += 1;
        }
    }
    Ok(added)
}

/// Adds one planned visit. Only a newly added visit is announced, so later passes never repeat the notice.
async fn plan_visit(s: &AppState, sub: Uuid, user: Uuid, date: NaiveDate, preferred: u8) -> ApiResult<bool> {
    let mut tx = s.db.begin().await?;
    let placed = free_hour(&mut tx, date, preferred).await?;
    let inserted = sqlx::query(
        "INSERT INTO subscription_visits (subscription_id, visit_date, start_hour) VALUES ($1, $2, $3) ON CONFLICT (subscription_id, visit_date) DO NOTHING",
    )
    .bind(sub)
    .bind(date)
    .bind(i16::from(placed.unwrap_or(preferred)))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    let added = inserted == 1;
    if added {
        announce_placement(s, user, sub, date, preferred, placed).await;
    }
    Ok(added)
}

/// A visit moved to another window tells the customer. A day with no free window tells the customer and staff.
async fn announce_placement(s: &AppState, user: Uuid, sub: Uuid, date: NaiveDate, preferred: u8, placed: Option<u8>) {
    let data = json!({ "subscription_id": sub, "visit_date": date });
    match placed {
        Some(hour) if hour != preferred => {
            let moved = json!({ "subscription_id": sub, "visit_date": date, "start_hour": hour });
            let title = "정기 청소 시간이 바뀌었어요";
            let body = "요청하신 시간이 가득 차 가까운 시간으로 옮겼어요";
            best_effort("notify", notify(s, Recipient::User(user), "subscription.visit_moved", title, body, moved)).await;
        }
        Some(_) => {}
        None => {
            let title = "정기 청소 시간을 잡지 못했어요";
            best_effort(
                "notify",
                notify(
                    s,
                    Recipient::User(user),
                    "subscription.visit_unplaced",
                    title,
                    "고객센터로 문의해 주세요",
                    data.clone(),
                ),
            )
            .await;
            let title = "정기 청소 정원이 가득 찼어요";
            best_effort(
                "notify",
                notify(s, Recipient::Staff, "subscription.capacity_full", title, "배정 시간을 확인해 주세요", data),
            )
            .await;
        }
    }
}

/// The customer's arrival window when it has room, else the first standard window that does.
/// `None` when every window is full: the visit keeps the preferred window and the charge step reports the shortage.
async fn free_hour(conn: &mut PgConnection, date: NaiveDate, preferred: u8) -> ApiResult<Option<u8>> {
    let order = std::iter::once(preferred).chain(STANDARD_SLOTS.iter().copied().filter(|h| *h != preferred));
    for hour in order {
        let (used, cap) = lock_and_count_slot(conn, date, hour, None).await?;
        if check_capacity(used, cap).is_ok() {
            return Ok(Some(hour));
        }
    }
    Ok(None)
}

/// Charges every planned visit due tomorrow (KST) whose retry time has come. Returns (booked, failed).
async fn charge_due(s: &AppState, now: DateTime<Utc>) -> ApiResult<(usize, usize)> {
    let due: Vec<Due> = sqlx::query_as(&format!(
        "SELECT v.id AS visit_id, s.id AS sub_id, s.user_id, v.visit_date, v.start_hour
         FROM subscription_visits v JOIN subscriptions s ON s.id = v.subscription_id
         WHERE v.state = 'planned' AND s.status = 'active' AND v.visit_date = {KST_TODAY} + 1
           AND (v.retry_at IS NULL OR v.retry_at <= now())
         ORDER BY v.start_hour, v.id LIMIT $1"
    ))
    .bind(DUE_BATCH)
    .fetch_all(&s.db)
    .await?;
    let (mut booked, mut failed) = (0, 0);
    for d in &due {
        match charge_visit(s, d).await {
            Ok(Attempt::Booked(_)) => booked += 1,
            Ok(Attempt::Failed(f)) => {
                failed += 1;
                if let Err(e) = record_failure(s, d, &f, now).await {
                    tracing::warn!(error = %e, visit = %d.visit_id, "recording a failed charge failed");
                }
            }
            Ok(Attempt::Skipped | Attempt::Pending) => {}
            Err(e) => tracing::warn!(error = %e, visit = %d.visit_id, "subscription charge errored; retried next pass"),
        }
    }
    Ok((booked, failed))
}

/// Converts a booking-side refusal (bad address, full slot, missing card) into a retryable failure.
/// Internal and database errors are not refusals, so they propagate.
fn blocked(e: ApiError) -> ApiResult<Attempt> {
    match e {
        ApiError::Db(_) | ApiError::Internal(_) => Err(e),
        other => {
            tracing::warn!(error = %other, "subscription visit blocked");
            Ok(Attempt::Failed(Failure::Blocked))
        }
    }
}

/// Everything one charge needs, read once per attempt.
struct Charge<'a> {
    d: &'a Due,
    sub: &'a SubRow,
    prep: &'a Prepared,
    body: &'a BookingBody,
    price: &'a PriceBreakdown,
    key: &'a str,
}

/// The PortOne payment id for one try. Stable per visit and attempt count, so a retry reuses it and PortOne cannot charge twice.
fn charge_ref(visit: Uuid, attempts: i32) -> String {
    format!("subvisit-{visit}-{attempts}")
}

/// The id to charge with: the one an earlier pass already sent, else this attempt's fresh one.
fn payment_ref_for(stored: Option<String>, visit: Uuid, attempts: i32) -> String {
    stored.unwrap_or_else(|| charge_ref(visit, attempts))
}

/// How a charge call ended. Only a confirmed decline counts as a failed attempt; anything else is checked again next pass.
#[derive(Debug, PartialEq, Eq)]
enum Settled {
    Paid(String),
    Declined(String),
    Unknown,
}

fn settle(r: Result<String, ProviderError>) -> Settled {
    match r {
        Ok(tx) => Settled::Paid(tx),
        Err(ProviderError::Declined(code)) => Settled::Declined(code),
        Err(_) => Settled::Unknown,
    }
}

/// What PortOne's answer for a stored payment id means. `expected` is the price today.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Book,
    /// Captured for another amount than today's price: refund the captured amount.
    Refund(i64),
    Declined(String),
    Open,
}

fn verdict(status: &str, paid: i64, expected: i64) -> Verdict {
    match status {
        "PAID" if paid == expected => Verdict::Book,
        "PAID" => Verdict::Refund(paid),
        "FAILED" | "CANCELLED" => Verdict::Declined(format!("payment_{}", status.to_lowercase())),
        _ => Verdict::Open,
    }
}

/// When to retry a visit's charge, or `None` when no pass is left to do it. Charges go out the day before the visit (KST),
/// so the window closes at KST midnight of the visit. A retry is held one pass interval earlier so the last pass still sees it due.
fn next_try(now: DateTime<Utc>, hours: i64, visit: NaiveDate) -> Option<DateTime<Utc>> {
    let last = visit.and_time(NaiveTime::MIN).and_utc() - Duration::hours(9) - Duration::minutes(PASS_MINUTES);
    let at = (now + Duration::hours(hours)).min(last);
    (at > now).then_some(at)
}

async fn charge_visit(s: &AppState, d: &Due) -> ApiResult<Attempt> {
    let sub: SubRow = sqlx::query_as(
        "SELECT user_id, service_code, unit_index, unit_counts, address_id, payment_method_id, preferred_pro_id, discount_percent FROM subscriptions WHERE id = $1",
    )
    .bind(d.sub_id)
    .fetch_one(&s.db)
    .await?;
    let body = BookingBody {
        service_code: sub.service_code.clone(),
        unit_index: sub.unit_index.map(|u| u as usize),
        counts: counts_from_row(sub.unit_counts.clone()),
        address_id: sub.address_id,
        visit_date: d.visit_date,
        start_hour: d.start_hour as u8,
        coupon_id: None,
        answers: None,
        visit_notes: Value::Null,
        household: None,
    };
    let prep = match prepare(&s.db, sub.user_id, &body).await {
        Ok(p) => p,
        Err(e) => return blocked(e),
    };
    let peak = fixed_peak_bps(&s.db, body.visit_date, body.start_hour).await?;
    let list = price_booking_with(&prep.service, body.unit_index, &body.counts, None, peak, Utc::now())?;
    // The visit discount applies after the multi-item discount, which `list.total` already holds.
    let vp = visit_price(list.total.0, u8::try_from(sub.discount_percent).unwrap_or(0))?;
    let price = PriceBreakdown {
        list_price: list.list_price,
        multi_discount: list.multi_discount,
        discount: Won(vp.discount),
        total: Won(vp.total),
        minutes: list.minutes,
    };
    let key: Option<String> = sqlx::query_scalar("SELECT billing_key FROM payment_methods WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL")
        .bind(sub.payment_method_id)
        .bind(sub.user_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(key) = key else {
        return Ok(Attempt::Failed(Failure::Blocked));
    };
    let c = Charge {
        d,
        sub: &sub,
        prep: &prep,
        body: &body,
        price: &price,
        key: &key,
    };
    if price.total.0 == 0 {
        return finalize(s, &c, None).await;
    }
    let (attempts, stored): (i32, Option<String>) = sqlx::query_as("SELECT charge_attempts, payment_ref FROM subscription_visits WHERE id = $1")
        .bind(d.visit_id)
        .fetch_one(&s.db)
        .await?;
    let sent = stored.is_some();
    let pid = payment_ref_for(stored, d.visit_id, attempts);
    if sent {
        return reconcile(s, &c, &pid).await;
    }
    if !persist_ref(s, d.visit_id, &pid).await? {
        return Ok(Attempt::Skipped);
    }
    charge_once(s, &c, &pid).await
}

/// Saves the payment id in its own commit before PortOne is called, so a timeout or crash leaves it to reconcile.
async fn persist_ref(s: &AppState, visit: Uuid, pid: &str) -> ApiResult<bool> {
    let n = sqlx::query("UPDATE subscription_visits SET payment_ref = $2, updated_at = now() WHERE id = $1 AND state = 'planned' AND payment_ref IS NULL")
        .bind(visit)
        .bind(pid)
        .execute(&s.db)
        .await?
        .rows_affected();
    Ok(n == 1)
}

/// An earlier pass already sent `pid`: ask PortOne what happened instead of charging again.
async fn reconcile(s: &AppState, c: &Charge<'_>, pid: &str) -> ApiResult<Attempt> {
    let total = c.price.total.0;
    match s.providers.payment(pid, total).await {
        Ok(info) => match verdict(&info.status, info.total, total) {
            Verdict::Book => finalize(s, c, Some((pid, pid))).await,
            // Captured at another price: give the money back, then the next try charges today's price under a new id.
            Verdict::Refund(paid) => {
                refund_or_alert(s, "portone", pid, paid).await;
                Ok(Attempt::Failed(Failure::Repriced))
            }
            Verdict::Declined(code) => Ok(Attempt::Failed(Failure::Declined(code))),
            Verdict::Open => {
                tracing::warn!(status = %info.status, payment = pid, "subscription payment not settled; checked again next pass");
                Ok(Attempt::Pending)
            }
        },
        // PortOne has no record of this id, so nothing was charged and sending it again is safe.
        Err(ProviderError::NotFound) => charge_once(s, c, pid).await,
        Err(e) => {
            tracing::warn!(error = %e, payment = pid, "subscription payment lookup failed; checked again next pass");
            Ok(Attempt::Pending)
        }
    }
}

async fn charge_once(s: &AppState, c: &Charge<'_>, pid: &str) -> ApiResult<Attempt> {
    let r = s.providers.charge_billing(c.key, c.price.total.0, pid, "정기 청소", c.d.user_id).await;
    match settle(r) {
        Settled::Paid(tx) => finalize(s, c, Some((pid, &tx))).await,
        Settled::Declined(code) => Ok(Attempt::Failed(Failure::Declined(code))),
        Settled::Unknown => {
            tracing::warn!(payment = pid, "subscription charge unconfirmed; checked again next pass");
            Ok(Attempt::Pending)
        }
    }
}

/// Books the visit once the money is settled. The charge already happened, so any outcome other than a booking refunds it.
async fn finalize(s: &AppState, c: &Charge<'_>, paid: Option<(&str, &str)>) -> ApiResult<Attempt> {
    let outcome = book_visit(s, c, paid).await;
    if !matches!(outcome, Ok(Attempt::Booked(_)))
        && let Some((_, tx)) = paid
    {
        refund_or_alert(s, "portone", tx, c.price.total.0).await;
    }
    outcome
}

/// One transaction: lock the subscription, take the slot, insert the paid booking and its payment, then commit.
/// No row lock is held across the PortOne call; the lock is taken only after money has moved.
async fn book_visit(s: &AppState, c: &Charge<'_>, paid: Option<(&str, &str)>) -> ApiResult<Attempt> {
    let id = Uuid::now_v7();
    let mut tx = s.db.begin().await?;
    let live: Option<(String, String)> = sqlx::query_as(
        "SELECT s.status, v.state FROM subscription_visits v JOIN subscriptions s ON s.id = v.subscription_id
         WHERE v.id = $1 FOR UPDATE OF s, v",
    )
    .bind(c.d.visit_id)
    .fetch_optional(&mut *tx)
    .await?;
    if !matches!(&live, Some((status, state)) if status == "active" && state == "planned") {
        return Ok(Attempt::Skipped);
    }
    match lock_slot(&mut tx, c.d.visit_date, c.d.start_hour as u8, Some(c.d.visit_id)).await {
        Ok(()) => {}
        Err(ApiError::Domain(DomainError::SlotFull)) => return Ok(Attempt::Failed(Failure::Full)),
        Err(e) => return blocked(e),
    }
    let number = insert_paid(&mut tx, &s.config.data_key, id, c.d.user_id, c.prep, c.body, c.price).await?;
    if let Some((pid, tx_id)) = paid {
        record_payment(&mut tx, id, c.price.total.0, "full", "portone", tx_id, pid).await?;
    }
    commit_booking(tx, id, c.d, c.sub).await?;
    after_booking_created(s, id, &number).await;
    Ok(Attempt::Booked(id))
}

/// Links the booking to the subscription, pins the preferred partner when free, and marks the visit booked.
async fn commit_booking(mut tx: sqlx::Transaction<'_, sqlx::Postgres>, id: Uuid, d: &Due, sub: &SubRow) -> ApiResult<()> {
    sqlx::query("UPDATE bookings SET subscription_id = $2 WHERE id = $1")
        .bind(id)
        .bind(d.sub_id)
        .execute(&mut *tx)
        .await?;
    if let Some(pro) = sub.preferred_pro_id {
        lock_schedule(&mut tx, pro).await?;
        if pro_free_for(&mut tx, id, pro).await? {
            sqlx::query(
                "UPDATE bookings SET pro_id = $2, status = 2, accepted_at = now(), updated_at = now(),
                   assignee_id = (SELECT id FROM team_members WHERE pro_id = $2 AND role = 'owner')
                 WHERE id = $1",
            )
            .bind(id)
            .bind(pro)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("UPDATE subscription_visits SET state = 'booked', booking_id = $2, charge_attempts = charge_attempts + 1, retry_at = NULL, payment_ref = NULL, updated_at = now() WHERE id = $1")
        .bind(d.visit_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE subscriptions SET consecutive_failures = 0, updated_at = now() WHERE id = $1")
        .bind(d.sub_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Whether the preferred partner may take this booking: verified, prefs match, and no live job overlapping its window.
/// The caller holds `lock_schedule` for the partner.
async fn pro_free_for(conn: &mut PgConnection, booking: Uuid, pro: Uuid) -> ApiResult<bool> {
    let ok: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS (SELECT 1 FROM bookings b, pros p JOIN partner_prefs pp ON pp.pro_id = p.id
           WHERE b.id = $1 AND p.id = $2 AND {VISIBLE}
             AND NOT EXISTS (SELECT 1 FROM bookings o WHERE o.pro_id = p.id AND {}))",
        clash("b")
    ))
    .bind(booking)
    .bind(pro)
    .fetch_one(&mut *conn)
    .await?;
    Ok(ok)
}

/// Counts a failed attempt, schedules the retry, and pauses after repeated failures. The customer is told each time,
/// and also told the visit is lost when no retry fits before the charge window closes.
async fn record_failure(s: &AppState, d: &Due, failure: &Failure, now: DateTime<Utc>) -> ApiResult<()> {
    let failures: i32 = sqlx::query_scalar(
        "UPDATE subscriptions SET consecutive_failures = consecutive_failures + $2, updated_at = now() WHERE id = $1 RETURNING consecutive_failures",
    )
    .bind(d.sub_id)
    .bind(i32::from(failure.counts()))
    .fetch_one(&s.db)
    .await?;
    let retry = after_failure(u32::try_from(failures).unwrap_or(u32::MAX));
    let due = match retry {
        Retry::After { hours } => next_try(now, hours, d.visit_date),
        Retry::Pause => None,
    };
    let lost = retry != Retry::Pause && due.is_none();
    sqlx::query(
        "UPDATE subscription_visits SET charge_attempts = charge_attempts + 1, payment_ref = NULL, retry_at = $2,
           state = CASE WHEN $3 AND state = 'planned' THEN 'missed' ELSE state END, updated_at = now()
         WHERE id = $1",
    )
    .bind(d.visit_id)
    .bind(due)
    .bind(lost)
    .execute(&s.db)
    .await?;
    if retry == Retry::Pause {
        pause(s, d, failure).await?;
    }
    let (kind, title, body) = failure.notice();
    let data = json!({ "subscription_id": d.sub_id, "visit_date": d.visit_date });
    best_effort("notify", notify(s, Recipient::User(d.user_id), kind, title, body, data)).await;
    if lost {
        announce_missed(s, d.user_id).await;
    }
    Ok(())
}

/// Pauses the subscription after repeated failures and tells the customer.
async fn pause(s: &AppState, d: &Due, failure: &Failure) -> ApiResult<()> {
    let reason = match failure {
        Failure::Declined(_) => "payment",
        Failure::Full | Failure::Repriced | Failure::Blocked => "booking",
    };
    let paused = sqlx::query("UPDATE subscriptions SET status = 'paused', paused_reason = $2, updated_at = now() WHERE id = $1 AND status = 'active'")
        .bind(d.sub_id)
        .bind(reason)
        .execute(&s.db)
        .await?
        .rows_affected();
    if paused == 1 {
        best_effort(
            "notify",
            notify(
                s,
                Recipient::User(d.user_id),
                "subscription.paused",
                "정기 청소가 일시 정지됐어요",
                "결제 수단을 확인한 뒤 재개해 주세요",
                json!({ "subscription_id": d.sub_id }),
            ),
        )
        .await;
    }
    Ok(())
}

async fn announce_missed(s: &AppState, user: Uuid) {
    best_effort(
        "notify",
        notify(
            s,
            Recipient::User(user),
            "subscription.visit_missed",
            "이번 정기 청소를 잡지 못했어요",
            "고객센터로 문의해 주세요",
            json!({}),
        ),
    )
    .await;
}

/// Planned visits whose day has arrived without a charge become `missed`. Returns how many.
/// A visit still holding a charge id is settled first, so money captured for it is never left unrefunded.
async fn expire(s: &AppState) -> ApiResult<usize> {
    settle_open_charges(s).await?;
    let missed: Vec<(Uuid,)> = sqlx::query_as(&format!(
        "UPDATE subscription_visits v SET state = 'missed', updated_at = now() FROM subscriptions s
         WHERE s.id = v.subscription_id AND v.state = 'planned' AND v.visit_date <= {KST_TODAY} AND v.payment_ref IS NULL
         RETURNING s.user_id"
    ))
    .fetch_all(&s.db)
    .await?;
    for (user,) in &missed {
        announce_missed(s, *user).await;
    }
    Ok(missed.len())
}

/// Visits whose charge id is still stored and that will not be booked (day come, or skipped or cancelled meanwhile):
/// PortOne is asked once per pass. A capture is refunded, a failure or an unknown id clears the id, an open answer waits.
async fn settle_open_charges(s: &AppState) -> ApiResult<()> {
    let open: Vec<(Uuid, String)> = sqlx::query_as(&format!(
        "SELECT id, payment_ref FROM subscription_visits
         WHERE payment_ref IS NOT NULL AND state <> 'booked' AND (state <> 'planned' OR visit_date <= {KST_TODAY})"
    ))
    .fetch_all(&s.db)
    .await?;
    for (visit, pid) in open {
        // The visit does not store its price; the refund takes what PortOne says it captured.
        let clear = match s.providers.payment(&pid, DEV_LOOKUP_TOTAL).await {
            Ok(info) => match verdict(&info.status, info.total, info.total) {
                // The visit can no longer be booked, so a capture goes back to the customer.
                Verdict::Book | Verdict::Refund(_) => {
                    refund_or_alert(s, "portone", &pid, info.total).await;
                    true
                }
                Verdict::Declined(_) => true,
                Verdict::Open => false,
            },
            Err(ProviderError::NotFound) => true,
            Err(e) => {
                tracing::warn!(error = %e, payment = %pid, "open subscription payment lookup failed; checked again next pass");
                false
            }
        };
        if clear {
            sqlx::query("UPDATE subscription_visits SET payment_ref = NULL, updated_at = now() WHERE id = $1 AND payment_ref = $2")
                .bind(visit)
                .bind(&pid)
                .execute(&s.db)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn retry_reuses_the_stored_payment_id() {
        let visit = Uuid::nil();
        let first = payment_ref_for(None, visit, 0);
        // Upstream failed after the id was saved: the next pass must send the same id.
        assert_eq!(payment_ref_for(Some(first.clone()), visit, 0), first);
        // A confirmed decline bumps the attempt count, which gives the next try a new id.
        assert_ne!(payment_ref_for(None, visit, 1), first);
    }

    #[test]
    fn a_payment_that_settled_at_another_amount_is_refunded_not_booked() {
        assert_eq!(verdict("PAID", 52_000, 52_000), Verdict::Book);
        assert_eq!(verdict("PAID", 52_000, 47_000), Verdict::Refund(52_000));
        assert_eq!(verdict("FAILED", 0, 47_000), Verdict::Declined("payment_failed".into()));
        assert_eq!(verdict("VIRTUAL_ACCOUNT_ISSUED", 0, 47_000), Verdict::Open);
    }

    #[test]
    fn a_retry_never_falls_after_the_charge_window_closes() {
        // Declined 19:00 KST on 11-19 for a 11-20 visit: the window closes at KST midnight, 15:00 UTC.
        let visit = NaiveDate::from_ymd_opt(2026, 11, 20).expect("date");
        let declined = Utc.with_ymd_and_hms(2026, 11, 19, 10, 0, 0).single().expect("time");
        let retry = next_try(declined, 6, visit);
        // Last pass before midnight is at most one pass interval earlier; the retry must be due by then.
        assert_eq!(retry, Utc.with_ymd_and_hms(2026, 11, 19, 14, 55, 0).single());
        // A retry that fits inside the window keeps the normal delay.
        let early = Utc.with_ymd_and_hms(2026, 11, 19, 6, 0, 0).single().expect("time");
        assert_eq!(next_try(early, 6, visit), Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).single());
    }

    #[test]
    fn a_decline_in_the_last_minutes_before_midnight_has_no_retry_in_the_past() {
        // Declined 23:57 KST on 11-19 for a 11-20 visit: no pass before midnight is left for a retry.
        let visit = NaiveDate::from_ymd_opt(2026, 11, 20).expect("date");
        let late = Utc.with_ymd_and_hms(2026, 11, 19, 14, 57, 0).single().expect("time");
        assert_eq!(next_try(late, 6, visit), None, "a retry time already past is never charged");
    }

    #[test]
    fn only_a_card_or_booking_refusal_counts_and_each_failure_says_its_reason() {
        let declined = Failure::Declined("card_declined".into());
        assert!(declined.counts() && Failure::Blocked.counts());
        assert!(!Failure::Repriced.counts() && !Failure::Full.counts());
        assert_eq!(declined.notice().0, "subscription.payment_failed");
        for f in [Failure::Repriced, Failure::Full, Failure::Blocked] {
            let (kind, title, body) = f.notice();
            assert_ne!(kind, "subscription.payment_failed", "{f:?}");
            assert_ne!(body, declined.notice().2, "{f:?}");
            let ev = crate::i18n::event(kind).expect("listed for the owner");
            assert_eq!((ev.title, ev.body), (title, body), "{f:?}");
        }
    }

    #[test]
    fn upstream_is_unknown_and_only_a_decline_fails_the_attempt() {
        assert_eq!(settle(Err(ProviderError::Upstream("timeout".into()))), Settled::Unknown);
        assert_eq!(
            settle(Err(ProviderError::Declined("card_declined".into()))),
            Settled::Declined("card_declined".into())
        );
        assert_eq!(settle(Ok("tx_1".into())), Settled::Paid("tx_1".into()));
    }
}
