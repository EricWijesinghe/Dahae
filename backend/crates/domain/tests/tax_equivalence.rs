// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The generic tax rules equal the Korean functions they replaced, over every total that matters: each amount in
//! -5,000,000..=5,000,000 and the `i64` extremes. The old bodies are copied here verbatim as the reference.

use chrono::NaiveDate;
use domain::tax::{self, Rounding, TaxRate, WithholdingBase, WithholdingRule};
use domain::{Money, Won};

const RANGE: i64 = 5_000_000;

/// `split_vat_inclusive` before generalisation.
fn old_split_vat_inclusive(total: i64) -> (i64, i64) {
    let vat = total / 11;
    (total - vat, vat)
}

/// `business_withholding` before generalisation.
fn old_business_withholding(gross: Won) -> (i64, i64, i64, i64) {
    let gross = gross.0.max(0);
    let income_tax = gross * tax::BUSINESS_INCOME_TAX_BPS / 10_000;
    let local_tax = income_tax / 100 * 10;
    (gross, income_tax, local_tax, gross - income_tax - local_tax)
}

/// The KR rate and rule exactly as `packs/KR/pack.json` states them.
fn kr_rate() -> TaxRate {
    TaxRate {
        category: "service".into(),
        rate_bps: 1_000,
        from: NaiveDate::from_ymd_opt(1977, 7, 1).unwrap(),
        until: None,
    }
}

const KR_RULE: WithholdingRule = WithholdingRule {
    rate_bps: 300,
    local_surtax_bps: 1_000,
    tax_unit: 1,
    surtax_unit: 10,
    base: WithholdingBase::Gross,
};

fn parts(w: tax::Withholding) -> (i64, i64, i64, i64) {
    (w.gross.0, w.income_tax.0, w.local_tax.0, w.net.0)
}

fn extremes() -> [i64; 6] {
    [i64::MIN, i64::MIN + 1, -RANGE - 1, RANGE + 1, i64::MAX - 1, i64::MAX]
}

#[test]
fn split_inclusive_equals_old_vat_split() {
    let rate = kr_rate();
    for total in (-RANGE..=RANGE).chain(extremes()) {
        let (net, vat) = tax::split_inclusive(Money(total), &rate, Rounding::Floor);
        let want = old_split_vat_inclusive(total);
        assert_eq!((net.0, vat.0), want, "total {total}");
        assert_eq!(tax::split_vat_inclusive(total), want, "wrapper, total {total}");
    }
}

#[test]
fn withhold_equals_old_business_withholding() {
    assert_eq!(tax::KR_BUSINESS_WITHHOLDING, KR_RULE);
    // The old body overflows above i64::MAX / 300, so the top extremes are checked at its largest valid input.
    let top = i64::MAX / tax::BUSINESS_INCOME_TAX_BPS;
    let edges = [i64::MIN, i64::MIN + 1, -RANGE - 1, RANGE + 1, top - 1, top];
    for gross in (-RANGE..=RANGE).chain(edges) {
        let want = old_business_withholding(Won(gross));
        assert_eq!(parts(tax::withhold(Won(gross), &KR_RULE)), want, "gross {gross}");
        assert_eq!(parts(tax::business_withholding(Won(gross))), want, "wrapper, gross {gross}");
    }
}

#[test]
fn withhold_does_not_overflow_where_the_old_body_did() {
    let w = tax::withhold(Money(i64::MAX), &KR_RULE);
    assert_eq!(i128::from(w.income_tax.0), i128::from(i64::MAX) * 300 / 10_000);
    assert_eq!(w.gross.0, w.income_tax.0 + w.local_tax.0 + w.net.0);
}

#[test]
fn split_inclusive_rounding_modes() {
    // 20% VAT: tax on 1.00 is 100 * 2000 / 12000 = 16.67 minor units.
    let rate = TaxRate { rate_bps: 2_000, ..kr_rate() };
    let split = |t: i64, r| tax::split_inclusive(Money(t), &rate, r).1.0;
    assert_eq!(
        (split(100, Rounding::Floor), split(100, Rounding::HalfUp), split(100, Rounding::HalfEven)),
        (16, 17, 17)
    );
    // 3 * 2000 / 12000 = 0.5 exactly: half-up goes away from zero, half-even to the even 0.
    assert_eq!((split(3, Rounding::Floor), split(3, Rounding::HalfUp), split(3, Rounding::HalfEven)), (0, 1, 0));
    // 9 * 2000 / 12000 = 1.5: half-even goes to the even 2.
    assert_eq!(split(9, Rounding::HalfEven), 2);
    // A refund's tax mirrors the sale's.
    for r in [Rounding::Floor, Rounding::HalfUp, Rounding::HalfEven] {
        for t in 0..=10_000 {
            assert_eq!(split(-t, r), -split(t, r), "total {t}, {r:?}");
        }
    }
}

#[test]
fn rate_on_picks_the_period_in_force() {
    let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let rates = [
        TaxRate {
            category: "service".into(),
            rate_bps: 1_900,
            from: day(2007, 1, 1),
            until: Some(day(2020, 7, 1)),
        },
        TaxRate {
            category: "service".into(),
            rate_bps: 1_600,
            from: day(2020, 7, 1),
            until: Some(day(2021, 1, 1)),
        },
        TaxRate {
            category: "service".into(),
            rate_bps: 1_900,
            from: day(2021, 1, 1),
            until: None,
        },
        TaxRate {
            category: "food".into(),
            rate_bps: 700,
            from: day(2007, 1, 1),
            until: None,
        },
    ];
    let bps = |c: &str, d| tax::rate_on(&rates, c, d).map(|r| r.rate_bps);
    assert_eq!(bps("service", day(2006, 12, 31)), None);
    assert_eq!(bps("service", day(2020, 6, 30)), Some(1_900));
    assert_eq!(bps("service", day(2020, 7, 1)), Some(1_600), "until is exclusive");
    assert_eq!(bps("service", day(2026, 11, 27)), Some(1_900));
    assert_eq!(bps("food", day(2026, 11, 27)), Some(700));
    assert_eq!(bps("books", day(2026, 11, 27)), None);
    assert_eq!(tax::rate_on(&[tax::kr_vat()], "service", day(2026, 11, 27)), Some(&kr_rate()));
}
