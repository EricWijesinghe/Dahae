// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Client error reports from every app (core `ErrorReporter`): `POST /v1/client-errors` stores a capped,
//! PII-scrubbed row; the console lists the latest ones. `app` is the surface: customer web, Windows,
//! Android and iOS, the console, and the partner app per platform.
use crate::{
    auth::Admin,
    error::{ApiError, ApiResult},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use domain::redact::{mask_digit_runs, mask_words};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const APPS: [&str; 7] = ["web", "windows", "console", "android", "ios", "partner_android", "partner_ios"];
const MAX_MESSAGE_CHARS: usize = 1000;
const MAX_STACK_CHARS: usize = 4000;
const MAX_VERSION_CHARS: usize = 40;
/// Global, not per IP: a flood of reports must not fill the table. Ordinary traffic is far below this.
const MAX_PER_MINUTE: u32 = 60;
const RETENTION_DAYS: i32 = 14;
const LIST_DEFAULT: i64 = 50;
const LIST_MAX: i64 = 200;

static WINDOW: Mutex<(Option<Instant>, u32)> = Mutex::new((None, 0));

pub fn routes() -> Router<AppState> {
    Router::new().route("/client-errors", post(report))
}

pub fn admin_routes() -> Router<AppState> {
    Router::new().route("/client-errors", get(list))
}

#[derive(Deserialize)]
pub struct Report {
    app: String,
    version: String,
    message: String,
    #[serde(default)]
    stack: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Row {
    id: i64,
    at: chrono::DateTime<chrono::Utc>,
    app: String,
    version: String,
    message: String,
    stack: String,
}

/// Blanks out e-mail addresses (full-width `＠` and `．` too) and phone-like digit runs (9+ digits, groups joined
/// by a space, dash, dot, slash or parens, full-width digits too). Dates, times and booking numbers are kept, and
/// so are whole hex tokens (a release stack trace's addresses, which `flutter symbolize` needs). Lines and spacing
/// are kept. Addresses (street names) cannot be detected reliably: the apps must not send them.
pub fn scrub(text: &str) -> String {
    let is_email = |w: &str| w.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    mask_digit_runs(&mask_words(text, "[redacted]", is_email), 9, "[redacted]", true)
}

fn cap(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Validates and normalises one report. Errors are the only thing the client learns.
pub fn clean(r: Report) -> Result<(String, String, String, String), ApiError> {
    if !APPS.contains(&r.app.as_str()) {
        return Err(ApiError::BadRequest("invalid_app"));
    }
    let version = r.version.trim();
    if version.is_empty() || version.chars().count() > MAX_VERSION_CHARS {
        return Err(ApiError::BadRequest("invalid_version"));
    }
    let message = scrub(&cap(r.message.trim(), MAX_MESSAGE_CHARS));
    if message.is_empty() {
        return Err(ApiError::BadRequest("invalid_input"));
    }
    let stack = scrub(&cap(&r.stack, MAX_STACK_CHARS));
    Ok((r.app, cap(version, MAX_VERSION_CHARS), message, stack))
}

/// Fixed one-minute window. Returns false once the global budget for this minute is spent.
fn admit(now: Instant) -> bool {
    let Ok(mut w) = WINDOW.lock() else { return false };
    match w.0 {
        Some(start) if now.duration_since(start) < Duration::from_secs(60) => {}
        _ => *w = (Some(now), 0),
    }
    if w.1 >= MAX_PER_MINUTE {
        return false;
    }
    w.1 += 1;
    true
}

async fn report(State(s): State<AppState>, Json(r): Json<Report>) -> ApiResult<StatusCode> {
    let (app, version, message, stack) = clean(r)?;
    if !admit(Instant::now()) {
        return Err(ApiError::RateLimited);
    }
    sqlx::query("INSERT INTO client_errors (app, version, message, stack) VALUES ($1, $2, $3, $4)")
        .bind(app)
        .bind(version)
        .bind(message)
        .bind(stack)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<i64>,
}

async fn list(State(s): State<AppState>, Admin(_): Admin, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    let limit = q.limit.unwrap_or(LIST_DEFAULT).clamp(1, LIST_MAX);
    let rows: Vec<Row> = sqlx::query_as("SELECT id, at, app, version, message, stack FROM client_errors ORDER BY at DESC LIMIT $1")
        .bind(limit)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(serde_json::json!({ "items": rows })))
}

/// Called from the retention purge.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query("DELETE FROM client_errors WHERE at < now() - make_interval(days => $1)")
        .bind(RETENTION_DAYS)
        .execute(db)
        .await?;
    Ok(done.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(app: &str, message: &str) -> Report {
        Report {
            app: app.into(),
            version: "1.0.0".into(),
            message: message.into(),
            stack: String::new(),
        }
    }

    #[test]
    fn unknown_app_is_rejected() {
        assert!(matches!(clean(report("customer", "boom")), Err(ApiError::BadRequest("invalid_app"))));
    }

    #[test]
    fn every_app_surface_is_accepted() {
        for app in APPS {
            assert!(clean(report(app, "boom")).is_ok(), "{app}");
        }
    }

    #[test]
    fn empty_message_is_rejected() {
        assert!(clean(report("web", "   ")).is_err());
    }

    /// Each case is (input, scrubbed). Kept cases have input == output.
    fn check(cases: &[(&str, &str)]) {
        for (input, want) in cases {
            assert_eq!(scrub(input), *want, "{input:?}");
        }
    }

    #[test]
    fn scrub_masks_phone_numbers_with_any_common_separator() {
        check(&[
            ("01012345678", "[redacted]"),
            ("연락처 010 1234 5678", "연락처 [redacted]"),
            ("연락처 010 12 34 56 78", "연락처 [redacted]"),
            ("연락처 010-1234 5678", "연락처 [redacted]"),
            ("연락처 010 1234-5678", "연락처 [redacted]"),
            ("연락처: 010 1234 5678.", "연락처: [redacted]."),
            ("010 1234 5678입니다", "[redacted]입니다"),
            ("(010) 1234-5678", "([redacted]"),
            ("010 1234.5678", "[redacted]"),
            ("010/1234/5678", "[redacted]"),
            ("010 - 1234 - 5678", "[redacted]"),
            ("010\u{2013}1234\u{2013}5678", "[redacted]"),
            ("010\u{2014}1234\u{2014}5678", "[redacted]"),
            ("010\u{2212}1234\u{2212}5678", "[redacted]"),
            ("010\u{b7}1234\u{b7}5678", "[redacted]"),
            ("010\u{a0}1234\u{a0}5678", "[redacted]"),
            ("010  1234 5678", "[redacted]"),
            ("+82 10 1234 5678", "+[redacted]"),
        ]);
    }

    #[test]
    fn scrub_masks_full_width_phone_numbers() {
        check(&[
            ("０１０-１２３４-５６７８", "[redacted]"),
            ("번호는 ０１０１２３４５６７８ 입니다", "번호는 [redacted] 입니다"),
            ("０１０　１２３４　５６７８", "[redacted]"),
        ]);
    }

    #[test]
    fn scrub_keeps_short_numbers() {
        check(&[
            ("step 2 of 3", "step 2 of 3"),
            ("retry 3 of 5 (code 42)", "retry 3 of 5 (code 42)"),
            ("page 1/2", "page 1/2"),
            ("v1.5.0 - build 120", "v1.5.0 - build 120"),
            ("at foo.js:1234:56", "at foo.js:1234:56"),
        ]);
    }

    #[test]
    fn scrub_keeps_dates_and_times() {
        check(&[
            ("2026-11-17 09:34:51", "2026-11-17 09:34:51"),
            ("at 2026-11-17 09:34:51 KST", "at 2026-11-17 09:34:51 KST"),
            ("2026-11-17T09:34:51.123Z", "2026-11-17T09:34:51.123Z"),
            ("2026/11/17 09:34", "2026/11/17 09:34"),
            ("2026.11.17 9:05", "2026.11.17 9:05"),
            ("2026. 11. 17. 09:34", "2026. 11. 17. 09:34"),
            ("09:34:51 2026-11-17", "09:34:51 2026-11-17"),
            ("2026-11-17 12345", "2026-11-17 12345"),
        ]);
    }

    #[test]
    fn scrub_masks_phone_numbers_next_to_dates_and_times() {
        check(&[
            ("2026-11-17 010 1234 5678", "2026-11-17 [redacted]"),
            ("09:34 010 1234 5678", "09:34 [redacted]"),
            ("010 1990-12-31", "[redacted]"),
            ("1:010 1234 5678", "1:[redacted]"),
        ]);
    }

    #[test]
    fn scrub_keeps_booking_numbers() {
        check(&[("BK2611-00012", "BK2611-00012"), ("booking BK2611-00012 failed", "booking BK2611-00012 failed")]);
    }

    #[test]
    fn scrub_masks_phone_numbers_next_to_booking_numbers_and_unprefixed_ones() {
        check(&[
            ("BK2611-00012 010 1234 5678", "BK2611-00012 [redacted]"),
            ("2611-00012", "[redacted]"),
            ("XK2611-00012", "XK[redacted]"),
        ]);
    }

    #[test]
    fn scrub_keeps_whole_hex_tokens() {
        check(&[(
            "#00 abs 0000007a1c3d4e5f virt 00000000001b2c3d _kDartIsolateSnapshotInstructions+0x1a2b3c",
            "#00 abs 0000007a1c3d4e5f virt 00000000001b2c3d _kDartIsolateSnapshotInstructions+0x1a2b3c",
        )]);
    }

    #[test]
    fn scrub_masks_phone_numbers_glued_to_hex_letters_inside_a_longer_token() {
        check(&[
            ("tel:a01012345678", "tel:a[redacted]"),
            ("id=b01012345678", "id=b[redacted]"),
            ("a010-1234-5678", "a[redacted]"),
        ]);
    }

    #[test]
    fn scrub_masks_emails_including_full_width_signs() {
        check(&[
            ("call a@b.co now", "call [redacted] now"),
            ("kim＠mail.com", "[redacted]"),
            ("mail kim＠mail．com please", "mail [redacted] please"),
        ]);
    }

    #[test]
    fn scrub_keeps_at_signs_that_are_not_emails() {
        check(&[
            ("@user", "@user"),
            ("＠home", "＠home"),
            ("x@localhost", "x@localhost"),
            ("Instance of 'A'", "Instance of 'A'"),
        ]);
    }

    #[test]
    fn scrub_keeps_lines_and_spacing() {
        check(&[
            (
                "call 010-1234-5678 or a@b.co now\n  at foo.js:12",
                "call [redacted] or [redacted] now\n  at foo.js:12",
            ),
            ("x\na@b.co\nnext", "x\n[redacted]\nnext"),
            ("Error: x\na@b.co", "Error: x\n[redacted]"),
            ("a@b.co\n  at foo", "[redacted]\n  at foo"),
            ("a@b.co\tx", "[redacted]\tx"),
            ("010-1234-5678\n010-1234-5678", "[redacted]\n[redacted]"),
        ]);
    }

    #[test]
    fn message_and_stack_are_capped() {
        let big = "x".repeat(MAX_MESSAGE_CHARS + 50);
        let (_, _, msg, _) = clean(Report {
            app: "web".into(),
            version: "1".into(),
            message: big,
            stack: "s".repeat(MAX_STACK_CHARS + 9),
        })
        .unwrap();
        assert_eq!(msg.chars().count(), MAX_MESSAGE_CHARS);
    }

    #[test]
    fn global_budget_is_enforced_per_window() {
        let now = Instant::now();
        *WINDOW.lock().unwrap() = (Some(now), MAX_PER_MINUTE);
        assert!(!admit(now));
        assert!(admit(now + Duration::from_secs(61)));
    }
}
