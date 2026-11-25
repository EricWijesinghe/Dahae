// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Korea golden outputs of the pure rules, frozen before the country-pack refactor (PLAN section 8.1 step 1).
//! Each test renders a fixed input set to JSON and compares it byte for byte with
//! `crates/api/tests/golden/kr/<name>.json`. A difference is a regression: fix the code, not the golden.
//! Regenerate only on purpose, with `UPDATE_GOLDENS=1`, and review the diff.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use domain::{
    DomainError, Won,
    booking::{BookingStatus, cancel_quote},
    cancel_policy::{CancelPolicy, Tier},
    catalog::{Pricing, Service, default_catalog},
    coupon::{Coupon, WelcomeCoupon, WelcomePack},
    finance::{self, Cell},
    partner,
    peak::{self, PeakConfig},
    points::{self, Lot, PointsPolicy},
    pricing::{PriceBreakdown, price_booking, price_booking_with},
    quote::{self, QuoteAnswers, QuoteFormula},
    subscription,
    tax::{self, InvoiceRow, PayrollLine, VatInput, VatMethod, WithholdingLine},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Compares `value` with the stored golden, or rewrites it when `UPDATE_GOLDENS=1`.
fn golden(name: &str, value: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../api/tests/golden/kr")
        .join(format!("{name}.json"));
    let actual = serde_json::to_string_pretty(value).expect("golden JSON") + "\n";
    if std::env::var("UPDATE_GOLDENS").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}; run with UPDATE_GOLDENS=1 to create it", path.display()));
    assert!(
        expected == actual,
        "{} differs from today's output (run with UPDATE_GOLDENS=1 only for a reviewed change):\n{actual}",
        path.display()
    );
}

fn ok_or_code<T: Serialize>(r: Result<T, DomainError>) -> Value {
    match r {
        Ok(v) => serde_json::to_value(v).expect("serialise"),
        Err(e) => json!({ "error": e.code(), "detail": e.to_string() }),
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 10, 1, 0, 0).single().expect("instant")
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("date")
}

const AMOUNTS: [i64; 22] = [
    -5_000_000, -150, -51, -50, -49, -1, 0, 1, 49, 50, 51, 99, 100, 149, 150, 9_999, 52_000, 110_000, 580_849, 580_850, 1_234_567, 5_000_000,
];
const RATES_BPS: [i64; 11] = [0, 1, 100, 300, 500, 1_000, 1_100, 1_500, 2_000, 5_000, 10_000];

#[test]
fn money_rounding() {
    let rounding: Vec<Value> = AMOUNTS.iter().map(|&a| json!([a, Won(a).round100().0])).collect();
    let bps: Vec<Value> = AMOUNTS
        .iter()
        .map(|&a| json!({ "amount": a, "times_bps": RATES_BPS.iter().map(|&r| json!([r, Won(a).times_bps(r).0])).collect::<Vec<_>>() }))
        .collect();
    golden("money", &json!({ "round100": rounding, "times_bps": bps }));
}

fn breakdown(p: Result<PriceBreakdown, DomainError>) -> Value {
    match p {
        Ok(p) => json!({
            "list_price": p.list_price.0,
            "multi_discount": p.multi_discount.0,
            "discount": p.discount.0,
            "total_discount": p.total_discount().0,
            "total": p.total.0,
            "minutes": p.minutes,
        }),
        Err(e) => ok_or_code::<()>(Err(e)),
    }
}

/// Count vectors for a counts service: one of each option alone, all at once, two of the first, the cap, and invalid shapes.
fn count_cases(options: usize, max_each: Option<u8>) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = (0..options)
        .map(|i| {
            let mut v = vec![0; options];
            v[i] = 1;
            v
        })
        .collect();
    out.push(vec![1; options]);
    let mut two = vec![0; options];
    two[0] = 2;
    out.push(two);
    let cap = max_each.unwrap_or(domain::catalog::MAX_COUNT);
    out.push(vec![cap; options]);
    out.push(vec![cap.saturating_add(1); options]);
    out.push(vec![0; options]);
    out.push(vec![1; options + 1]);
    out
}

fn service_cases(s: &Service) -> Value {
    let kind = match &s.pricing {
        Pricing::Fixed { .. } => "fixed",
        Pricing::Unit { .. } => "unit",
        Pricing::Counts { .. } => "counts",
        Pricing::Quote => "quote",
    };
    let mut cases = Vec::new();
    let peaks = [0, 1_100, 2_000];
    let units: Vec<Option<usize>> = match s.pricing.options() {
        Some(o) => (0..=o.len()).map(Some).collect(),
        None => vec![None, Some(0)],
    };
    for unit in &units {
        for peak in peaks {
            cases.push(json!({ "unit_index": unit, "peak_bps": peak, "price": breakdown(price_booking(s, *unit, None, peak, now())) }));
        }
    }
    if let Pricing::Counts { units, max_each, .. } = &s.pricing {
        for counts in count_cases(units.len(), *max_each) {
            cases.push(json!({ "counts": counts, "peak_bps": 0, "price": breakdown(price_booking_with(s, None, &counts, None, 0, now())) }));
        }
    }
    json!({ "code": s.code, "kind": kind, "pricing": serde_json::to_value(&s.pricing).expect("pricing"), "cases": cases })
}

#[test]
fn price_quotes_for_every_pricing_kind() {
    let services: Vec<Value> = default_catalog().iter().map(service_cases).collect();
    let peak_bounds = [10_001, -1].map(|bps| json!({ "peak_bps": bps, "price": breakdown(price_booking(&default_catalog()[0], None, None, bps, now())) }));
    golden("pricing", &json!({ "services": services, "peak_out_of_range": peak_bounds }));
}

fn answers(v: Value) -> QuoteAnswers {
    serde_json::from_value(v).expect("quote answers")
}

#[test]
fn move_quote_estimates_and_deposits() {
    let formula = QuoteFormula::default();
    let aircon = default_catalog().into_iter().find(|s| s.code == "aircon").expect("aircon");
    let base = json!({"cleaning_type": "standard", "luggage": "empty", "building": "apartment", "layout": "studio",
                      "duplex": false, "bathrooms": 1, "balconies": 0, "elevator": true, "floor_3_or_higher": false});
    let mut cases = Vec::new();
    let variants: Vec<(&str, Value)> = vec![
        ("studio_plain", json!({})),
        ("one_and_half", json!({"layout": "one_and_half"})),
        ("two_rooms_same_day", json!({"layout": "two_rooms", "cleaning_type": "same_day_move"})),
        (
            "three_rooms_after_interior",
            json!({"layout": "three_rooms", "cleaning_type": "after_interior", "bathrooms": 2, "balconies": 2}),
        ),
        (
            "four_plus_lived_in",
            json!({"layout": "four_plus", "cleaning_type": "lived_in_deep_clean", "luggage": "fully_lived_in", "duplex": true}),
        ),
        (
            "walk_up_high_floor",
            json!({"layout": "two_rooms", "elevator": false, "floor_3_or_higher": true, "luggage": "not_unpacked"}),
        ),
        (
            "some_luggage_unknown",
            json!({"cleaning_type": "some_luggage_left", "luggage": "unknown", "building": "villa"}),
        ),
        (
            "partly_unpacked_move_in",
            json!({"cleaning_type": "normal_move_in", "luggage": "partly_unpacked", "building": "officetel"}),
        ),
        ("max_rooms", json!({"layout": "four_plus", "bathrooms": 5, "balconies": 5})),
        ("too_many_bathrooms", json!({"bathrooms": 6})),
        (
            "with_ac_addons",
            json!({"layout": "two_rooms", "ac_addons": [{"unit": aircon.pricing.options().expect("units")[0].label_key, "count": 2}]}),
        ),
    ];
    for (name, patch) in variants {
        let mut v = base.clone();
        for (k, val) in patch.as_object().expect("patch") {
            v[k] = val.clone();
        }
        let a = answers(v);
        let lines = quote::resolve_ac_addons(&aircon, &a.ac_addons);
        for peak in [0, 1_000, 3_000] {
            let estimate = match &lines {
                Ok(l) => ok_or_code(formula.estimate_with_addons(&a, peak, l)),
                Err(e) => ok_or_code::<()>(Err(e.clone())),
            };
            cases.push(json!({ "case": name, "peak_bps": peak, "estimate": estimate }));
        }
        let whole_home = quote::fixed_answers_for("whole_home", a.clone());
        cases.push(
            json!({ "case": name, "service": "whole_home", "estimate": ok_or_code(formula.estimate(&QuoteAnswers { ac_addons: vec![], ..whole_home })) }),
        );
    }
    let deposits: Vec<Value> = [0, 99_000, 180_000, 234_560, 1_000_049]
        .iter()
        .map(|&f| json!([f, formula.deposit(Won(f)).0]))
        .collect();
    let addon_totals: Vec<Value> = [0, 1_000, 10_000, 10_001]
        .iter()
        .map(|&d| json!({ "discount_bps": d, "total": ok_or_code(quote::addon_total(&[(Won(99_000), 2), (Won(129_000), 1)], d)) }))
        .collect();
    golden(
        "quote_estimates",
        &json!({ "formula": serde_json::to_value(&formula).expect("formula"), "cases": cases, "deposits": deposits, "addon_totals": addon_totals }),
    );
}

#[test]
fn peak_surcharge_on_kst_calendar() {
    let config = PeakConfig::default();
    // Weekday, Saturday, Sunday, a public holiday, a lunar block day, and evening hours on each side of the window.
    let slots = [
        (day(2026, 3, 10), 9),
        (day(2026, 3, 10), 17),
        (day(2026, 3, 10), 18),
        (day(2026, 3, 10), 21),
        (day(2026, 3, 10), 22),
        (day(2026, 3, 14), 10),
        (day(2026, 3, 15), 19),
        (day(2026, 1, 1), 10),
        (day(2026, 2, 17), 20),
        (day(2026, 12, 25), 18),
    ];
    let rows: Vec<Value> = slots
        .iter()
        .map(|&(d, h)| {
            let utc = peak::kst_to_utc(d, h).expect("kst slot");
            let s = peak::peak_surcharge(&config, utc);
            json!({ "kst_date": d, "kst_hour": h, "utc": utc.to_rfc3339(), "surcharge": s, "on_100000": s.amount_on(Won(100_000)).0 })
        })
        .collect();
    golden("peak", &json!({ "config": serde_json::to_value(&config).expect("config"), "slots": rows }));
}

const VAT_TOTALS: [i64; 16] = [
    0, 1, 10, 11, 12, 21, 22, 100, 1_000, 9_999, 10_000, 52_000, 110_000, 111_111, 1_000_001, 5_000_000,
];

#[test]
fn vat_split_and_estimate() {
    let split: Vec<Value> = VAT_TOTALS
        .iter()
        .chain([-1, -11, -110_000].iter())
        .map(|&t| {
            let (supply, vat) = tax::split_vat_inclusive(t);
            json!({ "total": t, "supply": supply, "vat": vat })
        })
        .collect();
    let estimate: Vec<Value> = VAT_TOTALS.iter().map(|&f| json!([f, finance::vat_estimate(Won(f)).0])).collect();
    golden(
        "vat",
        &json!({ "vat_bps": finance::VAT_BPS, "split_vat_inclusive": split, "vat_estimate": estimate }),
    );
}

fn withholding(w: tax::Withholding) -> Value {
    json!({ "gross": w.gross.0, "income_tax": w.income_tax.0, "local_tax": w.local_tax.0, "net": w.net.0 })
}

#[test]
fn partner_payout_and_withholding() {
    let grosses = [
        -1_000, 0, 1, 33, 99, 333, 1_000, 3_333, 9_999, 52_000, 80_000, 100_000, 123_456, 1_000_000, 5_000_000,
    ];
    let business: Vec<Value> = grosses.iter().map(|&g| withholding(tax::business_withholding(Won(g)))).collect();
    let by_type: Vec<Value> = ["freelancer", "unset", "business", "untracked", "other"]
        .iter()
        .map(|t| json!({ "tax_type": t, "on_123456": withholding(tax::payout_withholding(t, Won(123_456))) }))
        .collect();
    let payouts: Vec<Value> = grosses
        .iter()
        .flat_map(|&g| [0, partner::DEFAULT_FEE_BPS, 1_500, 3_333].map(move |bps| (g, bps)))
        .map(|(g, bps)| json!({ "gross": g, "fee_bps": bps, "payout": partner::payout(Won(g), bps) }))
        .collect();
    let dates: Vec<Value> = (0..8)
        .map(|i| day(2026, 3, 9) + Duration::days(i))
        .map(|d| json!([d, partner::payout_date(d)]))
        .collect();
    golden(
        "payout_withholding",
        &json!({ "default_fee_bps": partner::DEFAULT_FEE_BPS, "business_withholding": business, "payout_withholding": by_type,
                 "payout": payouts, "payout_date": dates }),
    );
}

fn policy(tiers: &[(i64, Option<u8>)], min_fee: i64) -> CancelPolicy {
    CancelPolicy {
        tiers: tiers.iter().map(|&(min_days, percent)| Tier { min_days, percent }).collect(),
        min_fee,
        reschedule_min_days: 2,
    }
}

#[test]
fn cancel_policy_refunds() {
    let policies = [
        ("default", CancelPolicy::default()),
        ("tiered_min_fee", policy(&[(7, Some(0)), (3, Some(10)), (1, Some(30)), (0, Some(50))], 5_000)),
        ("same_day_closed", policy(&[(1, Some(0)), (0, None)], 0)),
        ("invalid_rising_refund", policy(&[(3, Some(50)), (1, Some(10))], 0)),
    ];
    let paid = [0, 3_000, 52_000, 110_000];
    let statuses = [BookingStatus::Consultation, BookingStatus::Confirmed];
    let out: Vec<Value> = policies
        .iter()
        .map(|(name, p)| {
            let days: Vec<Value> = (-2..=8)
                .map(|d| {
                    let quotes: Vec<Value> = paid
                        .iter()
                        .flat_map(|&amount| statuses.map(|st| (amount, st)))
                        .map(|(amount, st)| json!({ "status": st.as_i16(), "paid": amount, "quote": cancel_quote(p, st, d, amount) }))
                        .collect();
                    json!({ "days_before": d, "outcome": p.outcome(d), "reschedule_allowed": p.reschedule_allowed(d), "quotes": quotes })
                })
                .collect();
            json!({ "name": name, "policy": p, "valid": p.validate().err(), "days": days })
        })
        .collect();
    golden(
        "cancel_policy",
        &json!({ "default_json": domain::cancel_policy::DEFAULT_JSON, "policies": out }),
    );
}

#[test]
fn coupon_and_points_application() {
    let bathroom = default_catalog().into_iter().find(|s| s.code == "bathroom").expect("bathroom");
    let aircon = default_catalog().into_iter().find(|s| s.code == "aircon").expect("aircon");
    let mut used = Coupon::issue(Won(5_000), None, now());
    used.used = true;
    let coupons = [
        ("any_5000", Coupon::issue(Won(5_000), None, now())),
        ("bathroom_10000", Coupon::issue(Won(10_000), Some("bathroom".into()), now())),
        ("other_service", Coupon::issue(Won(10_000), Some("aircon".into()), now())),
        ("over_price", Coupon::issue(Won(1_000_000), None, now())),
        ("expired", Coupon::issue(Won(5_000), None, now() - Duration::days(14))),
        ("used", used),
    ];
    let applied: Vec<Value> = coupons
        .iter()
        .map(|(name, c)| {
            json!({
                "coupon": name,
                "bathroom_unit1": breakdown(price_booking(&bathroom, Some(1), Some(c), 0, now())),
                "bathroom_unit1_peak2000": breakdown(price_booking(&bathroom, Some(1), Some(c), 2_000, now())),
                "aircon_counts_0210": breakdown(price_booking_with(&aircon, None, &[0, 2, 1, 0], Some(c), 0, now())),
            })
        })
        .collect();
    let pack = WelcomePack::default();
    let packs = [
        ("default", pack.clone()),
        (
            "three_mixed",
            WelcomePack(vec![welcome(20_000, None), welcome(15_000, Some("aircon")), welcome(15_000, Some("bathroom"))]),
        ),
        ("over_total", WelcomePack(vec![welcome(30_000, None), welcome(30_000, None)])),
        ("not_hundreds", WelcomePack(vec![welcome(15_050, None)])),
    ];
    let welcome_packs: Vec<Value> = packs
        .iter()
        .map(|(name, p)| {
            let issued: Vec<Value> = p
                .issue(now())
                .iter()
                .map(|c| json!({ "amount": c.amount.0, "service_code": c.service_code, "expires_at": c.expires_at.to_rfc3339() }))
                .collect();
            json!({ "name": name, "valid": p.validate().err().map(|e| e.to_string()), "issued": issued })
        })
        .collect();
    golden(
        "coupons",
        &json!({ "lifetime_days": domain::coupon::COUPON_LIFETIME_DAYS, "applied": applied, "welcome_packs": welcome_packs }),
    );

    let policy = PointsPolicy::default();
    let splits: Vec<Value> = [
        (52_000, 0, 10_000, 10_000),
        (52_000, 10_000, 20_000, 10_000),
        (52_000, 30_000, 20_000, 10_000),
        (52_000, 30_000, 50_000, 5_000),
        (52_000, -1, 0, 10_000),
        (99_999, 33_333, 40_000, 3_333),
    ]
    .iter()
    .map(|&(total, req, avail, cap)| {
        let r = points::split_payment(Won(total), req, avail, cap);
        json!({ "total": total, "requested": req, "available": avail, "max_use_bps": cap,
                    "split": r.map(|s| json!({ "points": s.points, "pg": s.pg.0 })).unwrap_or_else(|e| json!({ "error": e.code() })) })
    })
    .collect();
    let cashback: Vec<Value> = [0, 1, 99, 100, 52_000, 375_300]
        .iter()
        .map(|&c| json!([c, points::cashback(Won(c), policy.earn_bps), points::cashback(Won(c), 1_000)]))
        .collect();
    let shares: Vec<Value> = [
        (46_800, 52_000, 0),
        (46_800, 52_000, 26_000),
        (52_000, 52_000, 10_000),
        (100, 3, 1),
        (100, 0, 5),
        (10_000, 52_000, 60_000),
    ]
    .iter()
    .map(|&(a, p, pts)| json!({ "amount": a, "paid": p, "points": pts, "cash": points::cash_share(a, p, pts) }))
    .collect();
    let lots = [
        Lot {
            id: 1,
            issued: 3_000,
            consumed: 1_000,
            expires_at: now() + Duration::days(30),
        },
        Lot {
            id: 2,
            issued: 5_000,
            consumed: 0,
            expires_at: now() + Duration::days(5),
        },
        Lot {
            id: 3,
            issued: 2_000,
            consumed: 0,
            expires_at: now() - Duration::days(1),
        },
        Lot {
            id: 4,
            issued: 1_000,
            consumed: 1_000,
            expires_at: now() + Duration::days(1),
        },
    ];
    let allocations: Vec<Value> = [1, 4_999, 5_000, 6_500, 7_000, 7_001, 0]
        .iter()
        .map(|&amt| json!({ "amount": amt, "lots": points::allocate(&lots, amt, now()).map_err(|e| e.code()) }))
        .collect();
    golden(
        "points",
        &json!({ "policy": serde_json::to_value(policy).expect("policy"), "valid": policy.validate().err().map(|e| e.to_string()),
                 "expires_at": policy.expires_at(now()).to_rfc3339(), "split_payment": splits, "cashback": cashback,
                 "cash_share": shares, "available": points::available(&lots, now()),
                 "expiring_within_7": points::expiring_within(&lots, now(), 7), "allocate": allocations,
                 "lapsed": points::lapsed(&lots, now()) }),
    );
}

fn welcome(amount: i64, service: Option<&str>) -> WelcomeCoupon {
    WelcomeCoupon {
        amount: Won(amount),
        service_code: service.map(String::from),
    }
}

#[test]
fn subscription_visit_prices() {
    let prices: Vec<Value> = [0u8, 5, 10, 30, 31]
        .iter()
        .map(|&pct| json!({ "percent": pct, "on_52000": ok_or_code(subscription::visit_price(52_000, pct)), "on_99999": ok_or_code(subscription::visit_price(99_999, pct)) }))
        .collect();
    let monthly: Vec<Value> = [1, 2, 3, 4]
        .iter()
        .map(|&w| json!({ "weeks": w, "on_46800": subscription::Cadence::from_weeks(w).map(|c| subscription::monthly_equivalent(46_800, c)).map_err(|e| e.code()) }))
        .collect();
    let prorate: Vec<Value> = [(100_000, 0, 31), (100_000, 15, 31), (100_000, 31, 31), (99_999, 14, 28), (100_000, 5, 0)]
        .iter()
        .map(|&(m, a, d)| json!([m, a, d, subscription::prorate(m, a, d)]))
        .collect();
    golden(
        "subscriptions",
        &json!({ "visit_price": prices, "monthly_equivalent": monthly, "prorate": prorate }),
    );
}

/// Splits a CSV document into its `\n`-terminated lines, so the golden diff reads line by line; joining with `\n` restores it.
fn csv_lines(rows: &[Vec<Cell>]) -> Vec<String> {
    finance::csv_document(rows).split('\n').map(String::from).collect()
}

fn split(gross: i64) -> tax::Withholding {
    tax::business_withholding(Won(gross))
}

#[test]
fn tax_export_documents() {
    let vat = VatInput {
        methods: vec![
            VatMethod {
                method: "card".into(),
                count: 3,
                amount: 330_000,
            },
            VatMethod {
                method: "kakaopay".into(),
                count: 1,
                amount: 52_000,
            },
            VatMethod {
                method: "=cmd".into(),
                count: 1,
                amount: 1,
            },
        ],
        platform_fee: Won(76_401),
        refunds: Won(50_000),
    };
    let invoice = |d: u32, company: &str, total: i64| InvoiceRow {
        day: day(2026, 3, d),
        business_number: "2208162517".into(),
        company_name: company.into(),
        representative: "홍길동".into(),
        business_type: "서비스".into(),
        business_item: "청소".into(),
        address: "서울특별시 강남구 테헤란로 1, 10층".into(),
        email: "tax@example.test".into(),
        item_name: "입주청소 \"프리미엄\"".into(),
        total,
    };
    let invoices = [invoice(5, "에이스(주)", 110_000), invoice(6, "+plus", 99_999), invoice(31, "줄\n바꿈", 1)];
    let lines = [
        WithholdingLine {
            day: day(2026, 3, 11),
            payee: "김파트너".into(),
            team: "깨끗팀".into(),
            split: split(80_000),
        },
        WithholdingLine {
            day: day(2026, 3, 18),
            payee: "-이름".into(),
            team: "팀,콤마".into(),
            split: split(123_456),
        },
    ];
    let payroll = [
        PayrollLine {
            pro_id: "b".into(),
            name: "박직원".into(),
            team: "A".into(),
            employment: Some("employee".into()),
            split: split(100_000),
        },
        PayrollLine {
            pro_id: "a".into(),
            name: "김파트너".into(),
            team: "B".into(),
            employment: Some("freelance".into()),
            split: split(80_000),
        },
        PayrollLine {
            pro_id: "a".into(),
            name: "김파트너".into(),
            team: "B".into(),
            employment: Some("freelance".into()),
            split: split(33_333),
        },
        PayrollLine {
            pro_id: "c".into(),
            name: "이계약".into(),
            team: "C".into(),
            employment: Some("contract".into()),
            split: tax::payout_withholding("business", Won(50_000)),
        },
        PayrollLine {
            pro_id: "d".into(),
            name: "최미등록".into(),
            team: "D".into(),
            employment: None,
            split: split(1),
        },
    ];
    let guarded = ["plain", "=SUM(A1)", "+1", "-1", "@x", "\tx", "a,b", "say \"hi\"", "line\nbreak", "합계"].map(|s| [s.to_string(), finance::csv_text(s)]);
    let quarters: Vec<Value> = (0..=5)
        .map(|q| json!({ "quarter": q, "range": tax::quarter_range(2026, q).map_err(|e| e.code()) }))
        .collect();
    golden(
        "tax_documents",
        &json!({
            "vat": csv_lines(&tax::vat_document(&vat)),
            "tax_invoices": csv_lines(&tax::tax_invoice_document(&invoices)),
            "withholding": csv_lines(&tax::withholding_document(&lines)),
            "payroll": csv_lines(&tax::payroll_document(&payroll)),
            "csv_text": guarded,
            "quarter_range": quarters,
        }),
    );
}
