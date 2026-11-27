// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Owner tax exports for an outside accountant or payroll service: the 지급명세서 CSV for withheld partners, and
//! one row per person with paid amounts and withholding. Employees are included with their amounts; the system
//! does not run payroll itself.

use super::{
    audit,
    finance::{PeriodQuery, csv_response, export_name, resolve},
};
use crate::{auth::Admin, error::ApiResult, state::AppState};
use axum::{
    extract::{Query, State},
    response::Response,
};
use chrono::NaiveDate;
use domain::{
    Won, finance,
    tax::{self, PayrollLine},
};
use serde_json::json;

/// Paid payout with its payee: pro id, name, team, employment type (if recorded), amount, tax type snapshot.
type PaidRow = (String, String, String, Option<String>, i64, String);

/// 지급명세서 for paid payouts to withheld partners, by local payment date. The monthly 간이지급명세서 and the
/// annual 지급명세서 are both this range. Layout UNVERIFIED; see domain::tax::WITHHOLDING_HEADERS.
pub(super) async fn export_withholding(State(s): State<AppState>, Admin(a): Admin, Query(q): Query<PeriodQuery>) -> ApiResult<Response> {
    crate::auth::live_admin(&s, a.id).await?;
    super::staff_mfa::require_recent_stepup(&s, a.id).await?;
    let (from, to) = resolve(&q)?;
    let mut tx = s.db.begin().await?;
    let rows: Vec<(NaiveDate, String, String, i64, String)> = sqlx::query_as(
        "SELECT (p.paid_at AT TIME ZONE dahae_tz())::date, pr.name, pr.team_name, p.amount, p.tax_type
         FROM payouts p JOIN pros pr ON pr.id = p.pro_id
         WHERE p.state = 'paid' AND p.tax_type IN ('freelancer', 'unset')
           AND p.paid_at >= ($1::date::timestamp AT TIME ZONE dahae_tz()) AND p.paid_at < (($2::date + 1)::timestamp AT TIME ZONE dahae_tz())
         ORDER BY p.paid_at, p.id",
    )
    .bind(from)
    .bind(to)
    .fetch_all(&mut *tx)
    .await?;
    let lines: Vec<tax::WithholdingLine> = rows
        .into_iter()
        .map(|(day, payee, team, gross, tax_type)| tax::WithholdingLine {
            day,
            payee,
            team,
            split: tax::payout_withholding_under(s.pack.tax.withholding.as_ref(), &tax_type, Won(gross)),
        })
        .collect();
    audit(
        &mut *tx,
        "staff",
        Some(a.id),
        "finance.export",
        "withholding",
        json!({ "from": from, "to": to }),
    )
    .await?;
    tx.commit().await?;
    csv_response(&export_name("withholding", from, to), finance::csv_document(&tax::withholding_document(&lines)))
}

/// Paid payouts in the local date range, split per payout, then summed per person by `tax::payroll_document`.
pub async fn export_payroll(State(s): State<AppState>, Admin(a): Admin, Query(q): Query<PeriodQuery>) -> ApiResult<Response> {
    crate::auth::live_admin(&s, a.id).await?;
    super::staff_mfa::require_recent_stepup(&s, a.id).await?;
    let (from, to) = resolve(&q)?;
    let mut tx = s.db.begin().await?;
    let rows: Vec<PaidRow> = sqlx::query_as(
        "SELECT pr.id::text, pr.name, pr.team_name, pp.employment_type, p.amount, p.tax_type
         FROM payouts p
         JOIN pros pr ON pr.id = p.pro_id
         LEFT JOIN pro_profiles pp ON pp.pro_id = pr.id
         WHERE p.state = 'paid'
           AND p.paid_at >= ($1::date::timestamp AT TIME ZONE dahae_tz()) AND p.paid_at < (($2::date + 1)::timestamp AT TIME ZONE dahae_tz())
         ORDER BY p.paid_at, p.id",
    )
    .bind(from)
    .bind(to)
    .fetch_all(&mut *tx)
    .await?;
    let lines = payroll_lines(s.pack.tax.withholding.as_ref(), rows);
    audit(&mut *tx, "staff", Some(a.id), "finance.export", "payroll", json!({ "from": from, "to": to })).await?;
    tx.commit().await?;
    csv_response(&export_name("payroll", from, to), finance::csv_document(&tax::payroll_document(&lines)))
}

fn payroll_lines(rule: Option<&tax::WithholdingRule>, rows: Vec<PaidRow>) -> Vec<PayrollLine> {
    rows.into_iter()
        .map(|(pro_id, name, team, employment, gross, tax_type)| PayrollLine {
            pro_id,
            name,
            team,
            employment,
            split: tax::payout_withholding_under(rule, &tax_type, Won(gross)),
        })
        .collect()
}
