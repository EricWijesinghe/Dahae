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

impl Money {
    pub const ZERO: Money = Money(0);

    /// Round half-up to the nearest `unit` (a currency's display or cash unit). A unit of 1 or less is a no-op.
    pub fn round_to(self, unit: i64) -> Money {
        if unit <= 1 {
            return self;
        }
        Money((self.0 + unit / 2).div_euclid(unit) * unit)
    }

    /// Round half-up to the nearest 100 won (spec rule 8).
    pub fn round100(self) -> Money {
        self.round_to(ROUNDING_UNIT)
    }

    /// Multiply by a rate in basis points (10_000 = 100%), rounding half-up to 1 won.
    pub fn times_bps(self, bps: i64) -> Money {
        Money((self.0 * bps + BASIS_POINTS / 2).div_euclid(BASIS_POINTS))
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
        assert_eq!(Money(-150).round_to(100), Money(-100), "half-up on negatives rounds toward +infinity");
        assert_eq!(Money(1_234).round_to(1), Money(1_234));
        assert_eq!(Money(1_234).round_to(0), Money(1_234));
        for v in -1_000..=1_000 {
            assert_eq!(Money(v).round_to(ROUNDING_UNIT), Money(v).round100());
        }
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
