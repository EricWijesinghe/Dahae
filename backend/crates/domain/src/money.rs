// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};

/// An amount in minor units of the install's currency (whole won on KR), tax included. Integer only: money never
/// touches floating point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(pub i64);

/// The old name, kept while callers move to [`Money`]. A `use` alias rather than `type Won = Money`, because a type
/// alias cannot construct or match a tuple struct (`Won(5)`), which about 300 call sites do.
pub use self::Money as Won;

pub const ROUNDING_UNIT: i64 = 100;
const BASIS_POINTS: i64 = 10_000;

/// `v` clamped into `i64`.
fn saturate(v: i128) -> i64 {
    i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX })
}

impl Money {
    pub const ZERO: Money = Money(0);

    /// Round to the nearest `unit` (a currency's display or cash unit), halves away from zero, so a negative amount
    /// (a refund) mirrors its sale: 150 -> 200 and -150 -> -200, like `tax::Rounding::HalfUp`. A unit of 1 or less
    /// is a no-op. A result beyond `i64` saturates at the bound instead of wrapping.
    pub fn round_to(self, unit: i64) -> Money {
        if unit <= 1 {
            return self;
        }
        let (n, u) = (i128::from(self.0), i128::from(unit));
        let magnitude = (n.abs() + u / 2) / u * u;
        Money(saturate(if n < 0 { -magnitude } else { magnitude }))
    }

    /// Round half-up to the nearest 100 won (spec rule 8). KR's rule, kept as it always was: halves round toward
    /// +infinity, so -150 -> -100. New currencies use [`Money::round_to`]. Saturates instead of wrapping.
    pub fn round100(self) -> Money {
        let u = i128::from(ROUNDING_UNIT);
        Money(saturate((i128::from(self.0) + u / 2).div_euclid(u) * u))
    }

    /// Multiply by a rate in basis points (10_000 = 100%), rounding half-up (toward +infinity) to 1 won. Worked in
    /// `i128`; a result beyond `i64` saturates at the bound instead of wrapping.
    pub fn times_bps(self, bps: i64) -> Money {
        let b = i128::from(BASIS_POINTS);
        Money(saturate((i128::from(self.0) * i128::from(bps) + b / 2).div_euclid(b)))
    }

    pub fn min(self, other: Money) -> Money {
        Money(self.0.min(other.0))
    }

    pub fn saturating_sub(self, other: Money) -> Money {
        Money((self.0 - other.0).max(0))
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_half_up_to_100() {
        assert_eq!(Won(580_849).round100(), Won(580_800));
        assert_eq!(Won(580_850).round100(), Won(580_900));
        assert_eq!(Won(0).round100(), Won(0));
    }

    #[test]
    fn round_to_any_unit() {
        assert_eq!(Money(1_234).round_to(5), Money(1_235));
        assert_eq!(Money(1_232).round_to(5), Money(1_230));
        assert_eq!(Money(150).round_to(100), Money(200));
        assert_eq!(Money(-150).round_to(100), Money(-200), "a refund mirrors its sale");
        assert_eq!(Money(-149).round_to(100), Money(-100));
        assert_eq!(Money(1_234).round_to(1), Money(1_234));
        assert_eq!(Money(1_234).round_to(0), Money(1_234));
        for v in -1_000..=1_000 {
            assert_eq!(Money(v).round_to(ROUNDING_UNIT), Money(-Money(-v).round_to(ROUNDING_UNIT).0), "{v}");
            if v >= 0 {
                assert_eq!(Money(v).round_to(ROUNDING_UNIT), Money(v).round100(), "{v}");
            }
        }
        assert_eq!(Money(-150).round100(), Money(-100), "KR's round100 keeps halves toward +infinity");
    }

    #[test]
    fn extremes_saturate_instead_of_wrapping() {
        // Wrapped to -9223372036854775800 when worked in i64.
        assert_eq!(Money(i64::MAX - 10).round_to(100), Money(9_223_372_036_854_775_800));
        assert_eq!(Money(i64::MAX - 10).round100(), Money(9_223_372_036_854_775_800));
        assert_eq!(Money(i64::MAX).round_to(1_000), Money(i64::MAX));
        assert_eq!(Money(i64::MIN).round_to(1_000), Money(i64::MIN));
        assert_eq!(Money(i64::MAX / 1000).times_bps(10_000), Money(i64::MAX / 1000));
        assert_eq!(Money(i64::MAX).times_bps(20_000), Money(i64::MAX));
        assert_eq!(Money(i64::MIN).times_bps(20_000), Money(i64::MIN));
        assert_eq!(Money(-5).times_bps(10_000), Money(-5));
    }

    #[test]
    fn won_alias_constructs_and_matches() {
        let Won(v) = Won(7);
        assert_eq!(Money(v), Won(7));
    }

    #[test]
    fn basis_points() {
        assert_eq!(Won(560_000).times_bps(1_000), Won(56_000));
        assert_eq!(Won(52_000).times_bps(2_000), Won(10_400));
    }
}
