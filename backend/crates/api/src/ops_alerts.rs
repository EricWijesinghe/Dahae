// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Owner alerts. A pass every few minutes checks a handful of rules over the ops failure log, the
//! backup status and free disk. A firing rule goes to the console inbox, to the owner's phone (SMS)
//! and to staff push devices, then sits out a cooldown so one incident sends one message per cooldown.
//! Quiet hours (KST) hold everything until they end; the condition is still there then.

use crate::{
    config::Config,
    error::{ApiError, ApiResult},
    integrations::Section,
    lifecycle::next_tick,
    metrics::job_pass,
    ops::{self, Level},
    realtime::{Recipient, notify},
    settings_registry,
    state::AppState,
};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration as StdDuration,
};
use tokio::task::JoinHandle;

pub const RULES: [&str; 17] = [
    "sms_failing",
    "push_failing",
    "payment_failing",
    "payment_stuck",
    "payment_mismatch",
    "social_failing",
    "auth_suspicious",
    "otp_volume",
    "backup_stale",
    "backup_offsite_failing",
    "backup_offsite_not_configured",
    "backup_uploads_failing",
    "disk_low",
    "unassigned_stale",
    "same_day_cancel",
    "tax_deadline",
    "payout_unconfirmed",
];
pub const COOLDOWN_HOURS: i64 = 6;
/// Local backups running with offsite never set up for this long raise the owner alert.
const OFFSITE_UNSET_ALERT_HOURS: f64 = 48.0;
/// Hourly sign-in code sends (all numbers) that raise the owner's `otp_volume` alert. Nothing is refused at this level.
pub const OTP_GLOBAL_WARN_PER_HOUR: i64 = 3_000;
/// Hard stop on hourly sign-in code sends (all numbers): 10x the warn level, far above normal peaks, so spend is bounded.
/// Past it no number gets a code, account or not, and every request gets the same refusal.
pub const OTP_GLOBAL_CEILING_PER_HOUR: i64 = 10 * OTP_GLOBAL_WARN_PER_HOUR;
/// Hourly sign-in codes from one client address (several customers can share one behind a carrier NAT).
pub const OTP_MAX_PER_IP_PER_HOUR: i64 = 100;
/// Hourly sign-in codes from one IPv4 /24: carriers put many customers behind shared IPv4 (CGNAT), so this sits above
/// a whole company's peak hour of sign-ins.
pub const OTP_MAX_PER_IPV4_NET_PER_HOUR: i64 = 1_000;
/// Hourly sign-in codes from one IPv6 /64 (one subscriber's prefix), so its endless addresses cannot spend the ceiling.
pub const OTP_MAX_PER_IPV6_NET_PER_HOUR: i64 = 300;
const PREFS_KEY: &str = "ops_alerts";
const BUSINESS_KEY: &str = "business";
const PASS_EVERY: StdDuration = StdDuration::from_secs(300);
const SMS_WINDOW_MIN: i64 = 15;
const SMS_FAILURES: i64 = 3;
const PUSH_WINDOW_MIN: i64 = 30;
const PUSH_FAILURES: i64 = 5;
const PAYMENT_WINDOW_MIN: i64 = 15;
const PAYMENT_FAILURES: i64 = 3;
const STUCK_WINDOW_MIN: i64 = 60;
/// The daily PortOne diff logs its mismatches once a day; the alert reads the last day of them.
const MISMATCH_WINDOW_MIN: i64 = 24 * 60;
const DEAD_LETTER_WINDOW_MIN: i64 = 60;
const SOCIAL_WINDOW_MIN: i64 = 30;
const SOCIAL_FAILURES: i64 = 5;
const SOCIAL_PROVIDERS: [&str; 4] = ["kakao", "naver", "google", "apple"];
/// Lockouts, refresh-token reuse and MFA lockouts across the window before the owner hears about it.
const AUTH_WINDOW_MIN: i64 = 60;
const AUTH_EVENTS: i64 = 5;
const KST_OFFSET_SECS: i32 = 9 * 3600;
/// A same-day cancel rate needs this many bookings for the day before it can alert.
const CANCEL_MIN_BOOKINGS: i64 = 5;
/// Quarterly VAT filing months; the filing day is the 25th of each.
const VAT_MONTHS: [u32; 4] = [1, 4, 7, 10];

/// What the owner chose on the console's alert card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Rule key -> on/off. A missing rule is on.
    pub rules: BTreeMap<String, bool>,
    pub quiet_start: u32,
    pub quiet_end: u32,
    /// Digits only. Empty means use the business support phone.
    pub alert_phone: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            rules: BTreeMap::new(),
            quiet_start: 22,
            quiet_end: 7,
            alert_phone: None,
        }
    }
}

impl Prefs {
    pub fn enabled(&self, rule: &str) -> bool {
        self.rules.get(rule).copied().unwrap_or(true)
    }
}

/// Validates the console's input. Unknown rule keys are dropped; hours must be 0..=23.
pub fn parse_input(v: &Value) -> Result<Prefs, &'static str> {
    let mut prefs = Prefs::default();
    for rule in RULES {
        if let Some(on) = v["rules"].get(rule).and_then(Value::as_bool) {
            prefs.rules.insert(rule.to_string(), on);
        }
    }
    prefs.quiet_start = hour(&v["quiet_start"]).ok_or("invalid_quiet_hours")?;
    prefs.quiet_end = hour(&v["quiet_end"]).ok_or("invalid_quiet_hours")?;
    prefs.alert_phone = match v["alert_phone"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(raw) => {
            let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
            let allowed = raw.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ');
            // Korean mobiles only (010, 011, 016-019): never premium-rate 060 or landlines.
            let mobile = digits.starts_with("01") && matches!(digits.as_bytes().get(2), Some(b'0' | b'1' | b'6'..=b'9'));
            if !allowed || !mobile || !(10..=11).contains(&digits.len()) {
                return Err("invalid_phone");
            }
            Some(digits)
        }
    };
    Ok(prefs)
}

fn hour(v: &Value) -> Option<u32> {
    v.as_u64().filter(|h| *h <= 23).map(|h| h as u32)
}

/// True when `hour` is inside [start, end). A window such as 22..7 wraps midnight; equal bounds mean none.
pub fn in_quiet_hours(hour: u32, start: u32, end: u32) -> bool {
    match start.cmp(&end) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => (start..end).contains(&hour),
        std::cmp::Ordering::Greater => hour >= start || hour < end,
    }
}

/// A rule may fire again only after the cooldown since its last message.
pub fn due(now: DateTime<Utc>, last_sent: Option<DateTime<Utc>>) -> bool {
    last_sent.is_none_or(|t| now - t >= Duration::hours(COOLDOWN_HOURS))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

/// The rules that should fire now, before cooldown. Pure over the inputs, so the dedup logic is testable.
pub fn select<'a>(findings: &'a [Finding], prefs: &Prefs, last: &HashMap<String, DateTime<Utc>>, now: DateTime<Utc>, quiet: bool) -> Vec<&'a Finding> {
    if quiet {
        return Vec::new();
    }
    findings
        .iter()
        .filter(|f| prefs.enabled(f.rule) && due(now, last.get(f.rule).copied()))
        .collect()
}

async fn count(db: &PgPool, integrations: &[&str], minutes: i64) -> Result<i64, sqlx::Error> {
    ops::failures_within(db, integrations, minutes).await
}

/// Evaluates every rule against current data.
pub async fn evaluate(s: &AppState) -> ApiResult<Vec<Finding>> {
    let db = &s.db;
    let mut out = Vec::new();
    if count(db, &["solapi"], SMS_WINDOW_MIN).await? >= SMS_FAILURES {
        out.push(Finding {
            rule: "sms_failing",
            title: "문자 발송이 계속 실패해요",
            body: "설정 > 문자(Solapi)에서 키와 발신번호를 확인하고 연결 테스트를 해 주세요.",
        });
    }
    if count(db, &["push"], PUSH_WINDOW_MIN).await? >= PUSH_FAILURES {
        out.push(Finding {
            rule: "push_failing",
            title: "앱 알림(푸시)이 계속 실패해요",
            body: "설정 > 앱 알림(Firebase) 키 파일을 확인하고 연결 테스트를 해 주세요.",
        });
    }
    if count(db, &["portone", "portone_webhook", "tosspayments", "tosspayments_webhook"], PAYMENT_WINDOW_MIN).await? >= PAYMENT_FAILURES {
        out.push(Finding {
            rule: "payment_failing",
            title: "결제 연동에 문제가 생겼어요",
            body: "설정 > 결제(PortOne/토스페이먼츠)의 키와 웹훅 설정을 확인하세요. 고객 결제가 실패할 수 있어요.",
        });
    }
    if count(db, &["payment_stuck"], STUCK_WINDOW_MIN).await? >= 1 {
        out.push(Finding {
            rule: "payment_stuck",
            title: "결제는 됐지만 자동 처리가 안 됐어요",
            body: "결제(PortOne/토스페이먼츠) 관리자 화면에서 해당 결제를 찾아 예약 생성 또는 환불 여부를 직접 확인하세요.",
        });
    }
    if count(db, &["sms_dead_letter"], DEAD_LETTER_WINDOW_MIN).await? >= 1 {
        out.push(Finding {
            rule: "sms_dead_letter",
            title: "문자 발송이 끝내 실패했어요",
            body: "고객에게 가지 못한 문자가 있어요. 설정 > 문자(Solapi)에서 키와 발신번호를 확인하고 연결 테스트를 해 주세요.",
        });
    }
    if count(db, &["payment_mismatch"], MISMATCH_WINDOW_MIN).await? >= 1 {
        out.push(Finding {
            rule: "payment_mismatch",
            title: "PG 결제 내역이 우리 기록과 달라요",
            body: "결제(PortOne/토스페이먼츠) 관리자 화면에서 어제 결제 목록을 확인하고, 예약·결제 내역과 금액이 맞지 않는 건을 직접 확인하세요.",
        });
    }
    if count(db, &SOCIAL_PROVIDERS, SOCIAL_WINDOW_MIN).await? >= SOCIAL_FAILURES {
        out.push(Finding {
            rule: "social_failing",
            title: "소셜 로그인이 계속 실패해요",
            body: "설정에서 카카오·네이버·구글·애플 로그인 키(클라이언트 ID)를 확인하세요.",
        });
    }
    if count(db, &["auth_abuse"], AUTH_WINDOW_MIN).await? >= AUTH_EVENTS {
        out.push(Finding {
            rule: "auth_suspicious",
            title: "로그인 시도가 비정상적으로 많아요",
            body: "직원 로그인 또는 인증 번호에 반복 실패가 있어 잠금이 걸렸어요. 감사 로그에서 대상 계정과 시간을 확인하고, 본인이 아니면 비밀번호를 바꾸세요.",
        });
    }
    out.extend(otp_volume(db).await?);
    out.extend(payout_findings(db).await?);
    out.extend(business_findings_or_skip(db, &s.config).await);
    out.extend(job_finding());
    let backup = ops::backup_status(db).await?;
    let age = ops::backup_age_hours(backup.as_ref(), Utc::now());
    let t = ops::thresholds(s).await?;
    if ops::backup_level(age, &t) == Level::Red {
        out.push(Finding {
            rule: "backup_stale",
            title: "자동 백업이 기준 시간보다 오래 없어요",
            body: "개발사에 백업 확인을 요청하세요. 외부 백업을 켜 두었다면 설정 > 외부 백업 (S3 호환)에서 연결 상태를 확인하세요.",
        });
    }
    let configured = s.providers.integ().has(Section::Backup);
    out.extend(offsite_backup_findings(db, configured).await?);
    let disk = ops::disk_usage(s.config.upload_dir.clone()).await;
    if disk.is_some_and(|d| ops::disk_level(d.free_pct(), &t) == Level::Red) {
        out.push(Finding {
            rule: "disk_low",
            title: "서버 디스크 여유 공간이 기준보다 적어요",
            body: "오래된 백업 파일을 정리하거나 서버 디스크를 늘려 주세요.",
        });
    }
    Ok(out)
}

/// Sign-in code sends in the last hour across all numbers, at or over the warn level.
pub async fn otp_volume(db: &PgPool) -> ApiResult<Vec<Finding>> {
    let sent: i64 = sqlx::query_scalar("SELECT count(*) FROM otp_codes WHERE created_at > now() - interval '1 hour'")
        .fetch_one(db)
        .await?;
    if sent < OTP_GLOBAL_WARN_PER_HOUR {
        return Ok(Vec::new());
    }
    if sent >= OTP_GLOBAL_CEILING_PER_HOUR {
        return Ok(vec![Finding {
            rule: "otp_volume",
            title: "인증 번호 발송이 상한에 닿아 모든 로그인을 멈췄어요",
            body: "한 시간 동안 인증 번호가 상한만큼 나가서, 지금은 가입한 고객과 파트너를 포함해 모든 번호에 인증 번호를 보내지 않아요. 한 시간이 지나 발송 수가 상한 아래로 내려가면 다시 보내요. 같은 IP나 번호에서 반복 요청이 없는지 확인하세요.",
        }]);
    }
    Ok(vec![Finding {
        rule: "otp_volume",
        title: "인증 번호 발송이 평소보다 많아요",
        body: "한 시간 동안 인증 번호가 아주 많이 나갔어요. 같은 IP나 번호에서 반복 요청이 없는지 확인하세요. 계속되면 문자 비용이 늘어나요.",
    }])
}

/// Business rules fail soft: a failed KPI query is logged and skipped, so the infrastructure alerts still run.
async fn business_findings_or_skip(db: &PgPool, cfg: &Config) -> Vec<Finding> {
    business_findings(db, cfg).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "business alert rules skipped");
        Vec::new()
    })
}

/// Both offsite uploads (database dump and photo archive): each rule is the status page's red rule as an owner alert.
/// Offsite never set up while local backups run is its own rule.
pub async fn offsite_backup_findings(db: &PgPool, backup_configured: bool) -> ApiResult<Vec<Finding>> {
    let mut out = Vec::new();
    let status = ops::backup_status(db).await?;
    if !backup_configured && ops::offsite_unset_hours(status.as_ref(), Utc::now()).is_some_and(|h| h > OFFSITE_UNSET_ALERT_HOURS) {
        out.push(Finding {
            rule: "backup_offsite_not_configured",
            title: "외부 백업을 아직 설정하지 않았어요",
            body: "설정 > 외부 백업 (S3 호환) 에서 저장소를 연결하고 연결 테스트를 해 주세요. 지금은 백업이 서버 안에만 있어요.",
        });
    }
    out.extend(
        offsite_failure(
            db,
            ops::PHOTOS_INTEGRATION,
            "backup_uploads_failing",
            "사진 외부 백업이 실패했어요",
            "설정 > 외부 백업에서 연결을 확인하고 연결 테스트를 해 주세요. 사진 파일이 아직 외부 저장소에 복사되지 않았어요.",
        )
        .await?,
    );
    out.extend(
        offsite_failure(
            db,
            "backup",
            "backup_offsite_failing",
            "데이터베이스 외부 백업이 실패했어요",
            "설정 > 외부 백업에서 연결을 확인하고 연결 테스트를 해 주세요. 데이터베이스 백업이 아직 외부 저장소에 복사되지 않았어요.",
        )
        .await?,
    );
    Ok(out)
}

/// The finding for one offsite integration: red when its last failure is within a day with no success after it.
async fn offsite_failure(db: &PgPool, integration: &str, rule: &'static str, title: &'static str, body: &'static str) -> ApiResult<Vec<Finding>> {
    let (ok, fail) = ops::last_outcome(db, integration).await?;
    if ops::integration_level(true, ok, fail, Utc::now()) != Level::Red {
        return Ok(Vec::new());
    }
    Ok(vec![Finding { rule, title, body }])
}

/// Business rules, owner-toggleable: confirmed bookings left without a pro, a bad same-day cancel rate, and the VAT deadline.
/// shortcut: VAT only; withholding (원천세) dates depend on the business's filing cycle, the owner confirms those.
/// shortcut: VAT dates are the corporate quarterly months for everyone; individual taxpayers file 1·7월 only, the owner's tax adviser confirms, then make the months a setting.
pub async fn business_findings(db: &PgPool, cfg: &Config) -> ApiResult<Vec<Finding>> {
    let mut out = Vec::new();
    let hours = settings_registry::get(db, cfg, "unassigned_alert_hours").await?;
    if ops::unassigned_older_than(db, hours).await? > 0 {
        out.push(Finding {
            rule: "unassigned_stale",
            title: "기사님이 정해지지 않은 예약이 있어요",
            body: "확정된 예약 중 오래 배정되지 않은 건이 있어요. 예약 목록에서 기사님이나 팀을 지정해 주세요.",
        });
    }
    let (cancelled, total) = ops::same_day_cancels(db).await?;
    let pct = settings_registry::get(db, cfg, "same_day_cancel_alert_pct").await?;
    if same_day_cancel_high(cancelled, total, pct) {
        out.push(Finding {
            rule: "same_day_cancel",
            title: "오늘 당일 취소가 많아요",
            body: "오늘 방문 예정 예약의 당일 취소 비율이 기준보다 높아요. 취소 사유를 확인하고 기사님 배정을 점검하세요.",
        });
    }
    let days = settings_registry::get(db, cfg, "tax_deadline_remind_days").await?;
    if vat_deadline_due(today_kst(), days) {
        out.push(Finding {
            rule: "tax_deadline",
            title: "부가가치세 신고 기한이 다가와요",
            body: "부가가치세 신고 기한이 다가와요. 날짜는 사업자 유형에 따라 달라요. 법인은 1·4·7·10월 25일, 개인 일반과세자는 보통 1·7월 25일이에요. 세무 담당자와 확인하세요.",
        });
    }
    Ok(out)
}

fn today_kst() -> NaiveDate {
    (Utc::now() + Duration::seconds(i64::from(KST_OFFSET_SECS))).date_naive()
}

/// Same-day cancels above the threshold percent, once the day has enough bookings for a rate to mean something.
pub fn same_day_cancel_high(cancelled: i64, total: i64, threshold_pct: i64) -> bool {
    total >= CANCEL_MIN_BOOKINGS && cancelled * 100 > total * threshold_pct
}

/// True on a VAT filing day or within `days_before` days ahead of one. The setting caps at 14 days, so
/// the same year's dates are enough: the next January 25 is always more than 14 days away from December.
pub fn vat_deadline_due(today: NaiveDate, days_before: i64) -> bool {
    VAT_MONTHS
        .iter()
        .filter_map(|&m| NaiveDate::from_ymd_opt(today.year(), m, 25))
        .any(|d| (0..=days_before).contains(&(d - today).num_days()))
}

/// Payout alerts. Stuck and failed are not owner-toggleable: that is money the owner must look at. An
/// unconfirmed Toss send (`payout_unconfirmed`) is on by default and listed on the alert card.
async fn payout_findings(db: &PgPool) -> Result<Vec<Finding>, sqlx::Error> {
    let (stuck, failed) = crate::payout_sync::alert_counts(db).await?;
    let mut out = Vec::new();
    if stuck > 0 {
        out.push(Finding {
            rule: "payout_stuck",
            title: "정산 송금이 24시간 넘게 끝나지 않았어요",
            body: "정산 > 송금 목록에서 해당 건을 찾아 상태 동기화를 눌러 주세요. 계속 대기 중이면 PG 관리자 화면을 확인하세요.",
        });
    }
    if crate::payout_sync::unconfirmed_count(db).await? > 0 {
        out.push(Finding {
            rule: "payout_unconfirmed",
            title: "토스 정산 송금 결과를 확인하지 못했어요",
            body: "정산 > 지급 내역에서 'Toss에서 확인하기'를 눌러 주세요. 확인되지 않으면 토스 상점관리자의 지급대행 내역을 보고 '송금되지 않았음을 확인'을 누를 수 있어요.",
        });
    }
    if failed > 0 {
        out.push(Finding {
            rule: "payout_failed",
            title: "정산 송금이 실패했어요",
            body: "정산 > 송금 목록에서 실패 사유와 기사님 계좌를 확인한 뒤 재시도하세요.",
        });
    }
    Ok(out)
}

/// Background job alert. Not owner-toggleable: a stalled job means bookings, charges or notifications stop silently.
fn job_finding() -> Option<Finding> {
    let stalled = crate::jobs::stalled(Utc::now().timestamp() as f64);
    if stalled.is_empty() {
        return None;
    }
    tracing::error!(jobs = ?stalled, "background jobs stalled");
    Some(Finding {
        rule: "job_stalled",
        title: "자동 작업이 멈췄어요",
        body: "정기 결제, 알림, 노쇼 확인 같은 자동 작업이 한동안 성공하지 못했어요. 서버 로그를 확인한 뒤 서버를 다시 시작해 주세요.",
    })
}

pub async fn load_prefs(db: &PgPool) -> Result<Prefs, sqlx::Error> {
    let raw: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(PREFS_KEY)
        .fetch_optional(db)
        .await?;
    Ok(raw.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
}

pub async fn save_prefs(db: &PgPool, prefs: &Prefs) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(PREFS_KEY)
    .bind(serde_json::to_value(prefs).unwrap_or(Value::Null))
    .execute(db)
    .await
    .map(|_| ())
}

/// Business support phone, the default alert number.
pub async fn business_phone(db: &PgPool) -> Result<Option<String>, sqlx::Error> {
    let raw: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(BUSINESS_KEY)
        .fetch_optional(db)
        .await?;
    Ok(raw
        .and_then(|v| v["support_phone"].as_str().map(|p| p.chars().filter(char::is_ascii_digit).collect()))
        .filter(|p: &String| !p.is_empty()))
}

async fn last_sent(db: &PgPool) -> Result<HashMap<String, DateTime<Utc>>, sqlx::Error> {
    let rows: Vec<(String, DateTime<Utc>)> = sqlx::query_as("SELECT alert_key, last_sent_at FROM ops_alerts").fetch_all(db).await?;
    Ok(rows.into_iter().collect())
}

async fn mark_sent(db: &PgPool, rule: &str, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ops_alerts (alert_key, last_sent_at) VALUES ($1, $2)
         ON CONFLICT (alert_key) DO UPDATE SET last_sent_at = EXCLUDED.last_sent_at",
    )
    .bind(rule)
    .bind(now)
    .execute(db)
    .await
    .map(|_| ())
}

/// Inbox entry, SMS to the owner and push to staff devices. Each channel logs its own failure and the others still go.
async fn deliver(s: &AppState, phone: Option<&str>, f: &Finding) {
    if let Err(e) = notify(s, Recipient::Staff, "ops_alert", f.title, f.body, json!({ "rule": f.rule })).await {
        tracing::warn!(error = %e, rule = f.rule, "ops alert inbox failed");
    }
    let brand = crate::routes::setup::brand_name(&s.db).await;
    if let Some(phone) = phone {
        let text = format!("[{brand}] {}\n{}", f.title, f.body);
        if let Err(e) = s.providers.send_sms(phone, &text).await {
            tracing::warn!(error = %e, rule = f.rule, "ops alert sms failed");
        }
    }
    let tokens: Result<Vec<String>, sqlx::Error> = sqlx::query_scalar("SELECT token FROM push_tokens WHERE owner_role = 'staff'")
        .fetch_all(&s.db)
        .await;
    match tokens {
        Ok(t) if !t.is_empty() => {
            if let Err(e) = s
                .providers
                .push(&t, &format!("{brand} 운영 알림"), f.title, &json!({ "kind": "ops_alert", "rule": f.rule }))
                .await
            {
                tracing::warn!(error = %e, rule = f.rule, "ops alert push failed");
            }
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "ops alert push tokens unreadable"),
    }
}

/// One pass: evaluate, select, deliver, stamp. Returns how many rules fired.
pub async fn run_once(s: &AppState) -> ApiResult<usize> {
    let now = Utc::now();
    let prefs = load_prefs(&s.db).await?;
    let kst = FixedOffset::east_opt(KST_OFFSET_SECS).ok_or(ApiError::Internal("kst offset".into()))?;
    let quiet = in_quiet_hours(now.with_timezone(&kst).hour(), prefs.quiet_start, prefs.quiet_end);
    if quiet {
        return Ok(0);
    }
    ops::prune(&s.db).await?;
    let findings = evaluate(s).await?;
    let last = last_sent(&s.db).await?;
    let chosen = select(&findings, &prefs, &last, now, false);
    if chosen.is_empty() {
        return Ok(0);
    }
    let phone = match prefs.alert_phone.clone() {
        Some(p) => Some(p),
        None => business_phone(&s.db).await?,
    };
    for f in &chosen {
        deliver(s, phone.as_deref(), f).await;
        mark_sent(&s.db, f.rule, now).await?;
    }
    Ok(chosen.len())
}

/// Starts the periodic pass. Errors are logged and the loop goes on.
pub fn spawn(state: AppState) -> JoinHandle<()> {
    let mut stop = state.hub.drain_signal();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(PASS_EVERY);
        while next_tick(&mut tick, &mut stop).await {
            let ok = match run_once(&state).await {
                Ok(0) => true,
                Ok(fired) => {
                    tracing::warn!(fired, "owner alerts sent");
                    true
                }
                Err(e) => {
                    tracing::warn!(error = %e, "ops alert pass failed");
                    false
                }
            };
            job_pass("ops_alerts", ok);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 11, h, m, 0).unwrap()
    }

    fn finding(rule: &'static str) -> Finding {
        Finding { rule, title: "t", body: "b" }
    }

    #[test]
    fn cooldown_allows_one_message_per_window() {
        assert!(due(at(12, 0), None));
        assert!(!due(at(12, 0), Some(at(9, 0))), "three hours later: still cooling down");
        assert!(due(at(12, 0), Some(at(5, 59))), "six hours is enough");
    }

    #[test]
    fn repeated_passes_do_not_spam() {
        let prefs = Prefs::default();
        let findings = vec![finding("sms_failing")];
        let mut last = HashMap::new();
        let first = select(&findings, &prefs, &last, at(12, 0), false);
        assert_eq!(first.len(), 1);
        last.insert("sms_failing".to_string(), at(12, 0));
        assert!(select(&findings, &prefs, &last, at(12, 5), false).is_empty(), "five minutes later");
        assert!(select(&findings, &prefs, &last, at(13, 0), false).is_empty(), "an hour later");
        assert_eq!(select(&findings, &prefs, &last, at(18, 1), false).len(), 1, "after cooldown, if still failing");
    }

    #[test]
    fn quiet_hours_hold_everything_and_wrap_midnight() {
        assert!(in_quiet_hours(23, 22, 7));
        assert!(in_quiet_hours(3, 22, 7));
        assert!(!in_quiet_hours(7, 22, 7));
        assert!(!in_quiet_hours(12, 22, 7));
        assert!(in_quiet_hours(13, 12, 14));
        assert!(!in_quiet_hours(5, 4, 4), "equal bounds mean no quiet window");
        let prefs = Prefs::default();
        assert!(select(&[finding("disk_low")], &prefs, &HashMap::new(), at(12, 0), true).is_empty());
    }

    #[test]
    fn disabled_rules_stay_silent() {
        let mut prefs = Prefs::default();
        prefs.rules.insert("push_failing".into(), false);
        let findings = vec![finding("push_failing"), finding("disk_low")];
        let chosen = select(&findings, &prefs, &HashMap::new(), at(12, 0), false);
        assert_eq!(chosen.iter().map(|f| f.rule).collect::<Vec<_>>(), vec!["disk_low"]);
    }

    #[test]
    fn input_is_validated_and_unknown_rules_are_dropped() {
        let ok = parse_input(&json!({
            "rules": { "sms_failing": false, "made_up": false },
            "quiet_start": 23, "quiet_end": 6, "alert_phone": "010-1234-5678"
        }))
        .expect("valid");
        assert_eq!(ok.rules.len(), 1);
        assert!(!ok.enabled("sms_failing"));
        assert!(ok.enabled("backup_stale"));
        assert_eq!(ok.alert_phone.as_deref(), Some("01012345678"));
        assert_eq!(parse_input(&json!({"quiet_start": 24, "quiet_end": 6})), Err("invalid_quiet_hours"));
        assert_eq!(
            parse_input(&json!({"quiet_start": 22, "quiet_end": 7, "alert_phone": "12"})),
            Err("invalid_phone")
        );
        let none = parse_input(&json!({"quiet_start": 22, "quiet_end": 7, "alert_phone": " "})).expect("blank phone ok");
        assert_eq!(none.alert_phone, None);
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    #[test]
    fn same_day_cancel_needs_a_sample_and_more_than_the_threshold() {
        assert!(!same_day_cancel_high(4, 4, 30), "four bookings is too few to alert");
        assert!(!same_day_cancel_high(3, 10, 30), "exactly the threshold is not over it");
        assert!(same_day_cancel_high(4, 10, 30), "40% is over 30%");
        assert!(!same_day_cancel_high(0, 0, 5), "an empty day never alerts");
    }

    #[test]
    fn vat_reminder_fires_before_each_filing_day_only() {
        assert!(vat_deadline_due(day(2026, 4, 18), 7), "seven days ahead of 4/25");
        assert!(vat_deadline_due(day(2026, 4, 25), 7), "the filing day itself");
        assert!(!vat_deadline_due(day(2026, 4, 17), 7), "eight days ahead");
        assert!(!vat_deadline_due(day(2026, 5, 10), 7), "no filing day near May");
        assert!(vat_deadline_due(day(2027, 1, 20), 7), "January 25 is in reach");
        assert!(!vat_deadline_due(day(2026, 12, 20), 7), "January 25 is still five weeks off");
    }
}
