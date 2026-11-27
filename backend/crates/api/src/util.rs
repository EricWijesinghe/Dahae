// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Small helpers shared by route modules.

use crate::{
    error::ApiResult,
    providers::{ProviderError, portone::BALANCE_MOVED},
    routes::cases::{self, NewCase},
    state::AppState,
};
use chrono::{DateTime, NaiveDate, Utc};
use domain::zone::{self, Tz};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Mutex, OnceLock, PoisonError, RwLock},
};
use uuid::Uuid;

/// The install's time zone (`install.tz`), fixed at boot by [`set_install_tz`]; the default zone until then.
/// Every pool connection carries the same zone as `dahae.tz` (`db::connect`), which SQL reads through `dahae_tz()`.
static INSTALL_TZ: RwLock<Option<Tz>> = RwLock::new(None);

/// The zone business dates and wall-clock hours are read in.
pub fn install_tz() -> Tz {
    INSTALL_TZ
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_else(zone::default_tz)
}

/// Sets the process zone. Call it before opening the pool: connections opened earlier keep the zone they started with.
pub fn set_install_tz(tz: Tz) {
    *INSTALL_TZ.write().unwrap_or_else(PoisonError::into_inner) = Some(tz);
}

/// Reads `install.tz`. Unset (a fresh install, or one from before time zones) is the default zone, so nothing changes;
/// a value that is not a known zone is logged and also read as the default rather than stopping the boot.
pub async fn load_install_tz(db: &PgPool) -> Result<Tz, sqlx::Error> {
    let stored: Option<String> = sqlx::query_scalar("SELECT value #>> '{}' FROM settings WHERE key = $1")
        .bind(crate::settings_registry::storage_key("install.tz"))
        .fetch_optional(db)
        .await?
        .flatten();
    Ok(match stored.as_deref().filter(|z| !z.is_empty()) {
        None => zone::default_tz(),
        Some(name) => zone::get(name).unwrap_or_else(|| {
            tracing::error!(zone = name, "install.tz is not a known time zone; using the default zone");
            zone::default_tz()
        }),
    })
}

/// Business dates are dates in the install's zone, whatever timezone the server or DB runs in.
pub fn local_today() -> NaiveDate {
    local_today_at(Utc::now())
}

/// [`local_today`] at a given instant.
pub fn local_today_at(now: DateTime<Utc>) -> NaiveDate {
    zone::today(&install_tz(), now)
}

/// SQL equivalent of [`local_today`]; never use bare `current_date` (it follows the DB timezone).
/// `dahae_tz()` (migration 0511) is `current_setting('dahae.tz')`, or the default zone on a connection without it.
pub const LOCAL_TODAY: &str = "(now() AT TIME ZONE dahae_tz())::date";

/// [`local_today`] under its name from before install time zones.
pub fn today_kst() -> NaiveDate {
    local_today()
}

/// [`local_today_at`] under its name from before install time zones.
pub fn today_kst_at(now: DateTime<Utc>) -> NaiveDate {
    local_today_at(now)
}

/// [`LOCAL_TODAY`] under its name from before install time zones.
pub const KST_TODAY: &str = LOCAL_TODAY;

/// Notifications and chat fan-out run after the money is committed: a push or
/// socket hiccup must not turn a successful booking into an error (and a retry).
pub async fn best_effort(what: &'static str, f: impl Future<Output = ApiResult<()>>) {
    if let Err(e) = f.await {
        tracing::warn!(error = %e, what, "post-commit side effect failed");
    }
}

/// Where a compensating refund stands. `Pending`: the provider has not confirmed it; it waits in
/// `refund_queue` and `checkout_recon::drain_refunds` retries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refund {
    Done,
    Pending,
}

/// Failed tries after which a stuck refund alerts the owner (ops rule `payment_stuck`), on every further failure.
pub const REFUND_ALERT_AFTER: i32 = 3;
/// A claimed refund is left alone this long, so a slow provider call is never sent twice.
pub const REFUND_LEASE_MINS: i32 = 5;
const REFUND_BACKOFF_MAX_MINS: i32 = 360;

/// Minutes to the next try after `attempts` tries: 2, 4, 8, ... capped at six hours.
pub fn refund_backoff_mins(attempts: i32) -> i32 {
    (1i32 << attempts.clamp(1, 9)).min(REFUND_BACKOFF_MAX_MINS)
}

/// One `refund_queue` row.
#[derive(sqlx::FromRow)]
pub struct QueuedRefund {
    pub id: Uuid,
    pub provider: String,
    pub tx_id: String,
    pub amount: i64,
    /// The gateway balance this refund expects; the provider refuses the cancel once it has moved.
    pub cancellable: i64,
    pub attempts: i32,
    pub checkout_id: Option<Uuid>,
}

pub const REFUND_COLS: &str = "id, provider, tx_id, amount, cancellable, attempts, checkout_id";

/// Compensating refund when a charge succeeded but saving it did not.
pub async fn refund_or_alert(s: &AppState, provider: &str, tx_id: &str, amount: i64) -> Refund {
    refund_durably(s, provider, tx_id, amount, None).await
}

/// Compensating refund, recorded in `refund_queue` before the provider is called: a failure is retried
/// with backoff, and one (provider, tx, amount) is refunded at most once however often a settle replays.
pub async fn refund_durably(s: &AppState, provider: &str, tx_id: &str, amount: i64, checkout: Option<Uuid>) -> Refund {
    // Queued refunds are full refunds: the balance they expect is the amount itself.
    let queued = sqlx::query_as::<_, QueuedRefund>(&format!(
        "INSERT INTO refund_queue (provider, tx_id, amount, cancellable, checkout_id, next_at) VALUES ($1, $2, $3, $3, $4, now() + make_interval(mins => $5))
         ON CONFLICT (provider, tx_id, amount) DO NOTHING RETURNING {REFUND_COLS}"
    ))
    .bind(provider)
    .bind(tx_id)
    .bind(amount)
    .bind(checkout)
    .bind(REFUND_LEASE_MINS)
    .fetch_optional(&s.db)
    .await;
    match queued {
        Ok(Some(row)) => retry_refund(s, &row).await,
        // Already asked for: never call the provider twice; report where the first one stands.
        Ok(None) => match sqlx::query_scalar("SELECT done_at IS NOT NULL FROM refund_queue WHERE provider = $1 AND tx_id = $2 AND amount = $3")
            .bind(provider)
            .bind(tx_id)
            .bind(amount)
            .fetch_one(&s.db)
            .await
        {
            Ok(true) => Refund::Done,
            Ok(false) => Refund::Pending,
            Err(e) => {
                tracing::error!(error = %e, provider, tx_id, "refund queue read failed");
                Refund::Pending
            }
        },
        Err(e) => {
            // No retry record possible: refund now, and a failure must reach a human.
            tracing::error!(error = %e, provider, tx_id, amount, "refund queue unavailable");
            let row = QueuedRefund {
                id: Uuid::nil(),
                provider: provider.into(),
                tx_id: tx_id.into(),
                amount,
                cancellable: amount,
                attempts: 0,
                checkout_id: checkout,
            };
            match attempt_refund(s, &row).await {
                Ok(()) => Refund::Done,
                Err(e) => {
                    tracing::error!(error = %e, provider, tx_id, amount, "COMPENSATING REFUND FAILED - reconcile manually");
                    crate::ops::record(&s.db, "payment_stuck", Err("refund_failed")).await;
                    Refund::Pending
                }
            }
        }
    }
}

/// One try at a queued refund, saved on its row. A DB error leaves the row leased for a later pass.
pub async fn retry_refund(s: &AppState, r: &QueuedRefund) -> Refund {
    let result = attempt_refund(s, r).await;
    let attempts = r.attempts + 1;
    let state = if result.is_ok() { Refund::Done } else { Refund::Pending };
    // A refusal for a moved balance never succeeds on a retry: staff decide, the drain stops.
    let parked = matches!(&result, Err(ProviderError::Declined(code)) if code == BALANCE_MOVED);
    let error = result.err().map(|e| e.to_string());
    let saved = sqlx::query(
        "UPDATE refund_queue SET attempts = $2, done_at = CASE WHEN $3 THEN now() END, last_error = $4,
            next_at = now() + make_interval(mins => $5), review_required = review_required OR $6 WHERE id = $1",
    )
    .bind(r.id)
    .bind(attempts)
    .bind(state == Refund::Done)
    .bind(&error)
    .bind(refund_backoff_mins(attempts))
    .bind(parked)
    .execute(&s.db)
    .await;
    if let Err(e) = saved {
        tracing::error!(error = %e, refund = %r.id, "refund outcome not saved");
    }
    if parked {
        park_for_staff(s, r).await;
    }
    if let Some(error) = error {
        let (provider, tx_id, amount) = (r.provider.as_str(), r.tx_id.as_str(), r.amount);
        if attempts >= REFUND_ALERT_AFTER {
            tracing::error!(
                error,
                provider,
                tx_id,
                amount,
                attempts,
                "COMPENSATING REFUND STILL FAILING - check the PG console"
            );
            crate::ops::record(&s.db, "payment_stuck", Err("refund_failed")).await;
        } else {
            tracing::warn!(error, provider, tx_id, amount, attempts, "compensating refund failed; queued for retry");
        }
    }
    state
}

/// Opens the staff case and the ops alert for a refund that will not go through on its own.
async fn park_for_staff(s: &AppState, r: &QueuedRefund) {
    let opened = cases::insert(
        &s.db,
        NewCase {
            booking: None,
            kind: "payment_dispute",
            source: "provider",
            reporter: "system",
            reporter_id: None,
            summary: "A refund was refused by the payment gateway because the payment balance changed (a cancel made outside the app). Check the payment in the payment gateway's console and refund the rest by hand.",
            photos: json!([]),
        },
    )
    .await;
    if let Err(e) = opened {
        tracing::error!(error = %e, refund = %r.id, "refund review case not opened");
    }
    crate::ops::record(&s.db, "payment_stuck", Err("refund_failed")).await;
}

async fn attempt_refund(s: &AppState, r: &QueuedRefund) -> Result<(), ProviderError> {
    if s.config.dev_mode && dev_refund_fails(&r.tx_id) {
        return Err(ProviderError::Upstream("dev refund failure".into()));
    }
    // After an ambiguous failure the gateway may have cancelled it already. Queued refunds are full refunds.
    let real_gateway = matches!(r.provider.as_str(), "portone" | crate::providers::TOSS | crate::providers::TOSS_BILLING);
    if r.attempts > 0 && real_gateway {
        match s.providers.payment(&r.provider, &r.tx_id, r.amount).await {
            Ok(info) if info.status == "CANCELLED" => return Ok(()),
            // A saved-card charge Toss still does not know minutes later never happened (Toss answers within 60 s):
            // nothing to refund. Stops before the try that would alert.
            Err(ProviderError::NotFound) if r.provider == crate::providers::TOSS_BILLING && r.attempts + 1 >= REFUND_ALERT_AFTER => {
                tracing::warn!(tx_id = %r.tx_id, "queued refund of a Toss charge that never happened; closing it");
                return Ok(());
            }
            Ok(_) | Err(ProviderError::NotFound) => {}
            Err(e) => return Err(e),
        }
    }
    s.providers.refund_checked(&r.provider, &r.tx_id, r.amount, r.cancellable).await
}

static DEV_REFUND_FAILURES: Mutex<BTreeMap<String, u32>> = Mutex::new(BTreeMap::new());

/// Dev fakes only: the next `times` refunds of `tx_id` fail like a provider outage. For tests.
#[doc(hidden)]
pub fn fail_dev_refunds(tx_id: &str, times: u32) {
    if let Ok(mut failing) = DEV_REFUND_FAILURES.lock() {
        failing.insert(tx_id.to_string(), times);
    }
}

fn dev_refund_fails(tx_id: &str) -> bool {
    let Ok(mut failing) = DEV_REFUND_FAILURES.lock() else { return false };
    match failing.get_mut(tx_id) {
        Some(left) if *left > 0 => {
            *left -= 1;
            true
        }
        _ => false,
    }
}

/// Serialises bookings per (date, hour) for the rest of the transaction, then counts
/// live bookings in that slot, optionally ignoring one booking (for reschedules).
/// Takes the slot lock and counts what holds seats: live bookings, planned subscription visits and waitlist offers.
/// `except` leaves out one booking or subscription visit id, the one being moved or charged.
pub async fn lock_and_count_slot(conn: &mut PgConnection, date: NaiveDate, hour: u8, except: Option<Uuid>) -> ApiResult<(u32, Option<u32>)> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("slot:{date}:{hour}"))
        .execute(&mut *conn)
        .await?;
    let used: i64 =
        sqlx::query_scalar("SELECT count(*) FROM bookings WHERE visit_date = $1 AND start_hour = $2 AND status >= 0 AND ($3::uuid IS NULL OR id <> $3)")
            .bind(date)
            .bind(i16::from(hour))
            .bind(except)
            .fetch_one(&mut *conn)
            .await?;
    // A planned visit of an active subscription holds its seat until it is charged and booked; a paused one releases it.
    let planned: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM subscription_visits v JOIN subscriptions s ON s.id = v.subscription_id
         WHERE v.state = 'planned' AND s.status = 'active' AND v.visit_date = $1 AND v.start_hour = $2 AND ($3::uuid IS NULL OR v.id <> $3)",
    )
    .bind(date)
    .bind(i16::from(hour))
    .bind(except)
    .fetch_one(&mut *conn)
    .await?;
    // A live waitlist offer holds a seat for 15 minutes.
    let holds: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM waitlist_entries WHERE status = 'offered' AND offer_expires_at > now()
           AND visit_date = $1 AND start_hour <= $2 AND end_hour > $2",
    )
    .bind(date)
    .bind(i16::from(hour))
    .fetch_one(&mut *conn)
    .await?;
    let used = used + planned + holds;
    let cap: Option<i32> = sqlx::query_scalar("SELECT capacity FROM slot_capacity WHERE start_hour = $1")
        .bind(i16::from(hour))
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
    Ok((u32::try_from(used).unwrap_or(u32::MAX), cap.map(|c| u32::try_from(c).unwrap_or(0))))
}

const MAX_NOTE_CHARS: usize = 300;
const NOTE_KEYS: [&str; 3] = ["entry", "parking", "other"];

/// Keeps only the known note fields, as bounded strings (prevents arbitrary JSON blobs).
pub fn sanitize_notes(v: &Value) -> Option<Value> {
    let mut out = serde_json::Map::new();
    for key in NOTE_KEYS {
        match v.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s.chars().count() <= MAX_NOTE_CHARS => {
                out.insert(key.into(), Value::String(s.trim().to_string()));
            }
            Some(_) => return None,
        }
    }
    Some(Value::Object(out))
}

pub const IMAGE_EXTS: [&str; 4] = ["jpg", "png", "webp", "heic"];

/// Origins of the former domains, set once at start (domains.rs). A signed URL keeps the host it was signed with, so
/// a photo uploaded or a page loaded before a move still sends one of ours.
pub static FORMER_BASES: OnceLock<Vec<String>> = OnceLock::new();

/// Stored form of one of our upload URLs (`/files/<32 hex>.<ext>`), or None for anything else.
/// Accepts the relative or base-prefixed form (current or former domain), with `/v1` and a signed query string; no
/// traversal, no foreign hosts.
pub fn own_upload(base: &str, url: &str) -> Option<String> {
    own_upload_in(base, FORMER_BASES.get().map_or(&[], Vec::as_slice), url)
}

fn own_upload_in(base: &str, former: &[String], url: &str) -> Option<String> {
    let path = url
        .strip_prefix(base)
        .or_else(|| former.iter().find_map(|b| url.strip_prefix(b.as_str())))
        .unwrap_or(url);
    let path = path.strip_prefix("/v1").unwrap_or(path);
    let name = path.strip_prefix("/files/")?.split('?').next()?;
    let (stem, ext) = name.split_once('.')?;
    let ok = stem.len() == 32 && stem.chars().all(|c| c.is_ascii_hexdigit()) && IMAGE_EXTS.contains(&ext);
    ok.then(|| format!("/files/{name}"))
}

/// Stored paths for a list of upload URLs; None if any entry is not one of ours.
pub fn own_uploads(base: &str, urls: &[String]) -> Option<Vec<String>> {
    urls.iter().map(|u| own_upload(base, u)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refund_retries_back_off_to_six_hours() {
        assert_eq!([0, 1, 2, 3].map(refund_backoff_mins), [2, 2, 4, 8]);
        assert_eq!(refund_backoff_mins(9), 360);
        assert_eq!(refund_backoff_mins(i32::MAX), 360);
    }

    #[test]
    fn dev_refund_failures_count_down_per_payment() {
        fail_dev_refunds("tx-a", 1);
        assert!(!dev_refund_fails("tx-b"));
        assert!(dev_refund_fails("tx-a"));
        assert!(!dev_refund_fails("tx-a"));
    }

    #[test]
    fn notes_are_bounded() {
        assert_eq!(sanitize_notes(&json!({"entry": "1234#", "junk": 1})), Some(json!({"entry": "1234#"})));
        assert_eq!(sanitize_notes(&json!({"entry": "x".repeat(301)})), None);
        assert_eq!(sanitize_notes(&json!({"parking": 5})), None);
    }

    #[test]
    fn upload_urls() {
        let b = "https://api.x";
        let stem = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            own_upload(b, &format!("https://api.x/files/{stem}.jpg")).as_deref(),
            Some(&*format!("/files/{stem}.jpg"))
        );
        assert_eq!(
            own_upload(b, &format!("https://api.x/v1/files/{stem}.png?exp=5&sig=ab")).as_deref(),
            Some(&*format!("/files/{stem}.png"))
        );
        assert_eq!(own_upload(b, &format!("/files/{stem}.webp")).as_deref(), Some(&*format!("/files/{stem}.webp")));
        assert!(own_upload(b, "https://api.x/files/../etc/passwd").is_none());
        assert!(own_upload(b, &format!("https://evil.x/files/{stem}.jpg")).is_none());
        assert!(own_upload(b, &format!("https://api.x.evil/files/{stem}.jpg")).is_none());
        assert!(own_upload(b, &format!("https://api.x/files/{stem}.svg")).is_none());
        let former = ["https://old.x".to_string()];
        assert_eq!(
            own_upload_in(b, &former, &format!("https://old.x/v1/files/{stem}.png?exp=5&sig=ab")).as_deref(),
            Some(&*format!("/files/{stem}.png")),
            "signed before a domain move"
        );
        assert!(own_upload_in(b, &former, &format!("https://old.x.evil/files/{stem}.png")).is_none());
    }
}
