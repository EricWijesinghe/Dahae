// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Currencies and the amount formats providers expect on the wire. Amounts stay [`Money`] (minor units) inside;
//! only an adapter turns one into its provider's format, through [`wire_amount`].

use crate::{DomainError, Money};

/// One ISO 4217 currency's money rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Currency {
    pub code: &'static str,
    /// Decimal places of the minor unit: KRW 0, EUR 2, KWD 3.
    pub minor_units: u8,
    /// Prices are rounded to this many minor units for display (KRW 100).
    pub display_unit: i64,
    /// Smallest amount cash can settle, in minor units (CHF 5 rappen).
    pub cash_unit: i64,
}

pub const KRW: Currency = Currency {
    code: "KRW",
    minor_units: 0,
    display_unit: 100,
    cash_unit: 1,
};
pub const JPY: Currency = Currency {
    code: "JPY",
    minor_units: 0,
    display_unit: 1,
    cash_unit: 1,
};
pub const EUR: Currency = Currency {
    code: "EUR",
    minor_units: 2,
    display_unit: 1,
    cash_unit: 1,
};
pub const CHF: Currency = Currency {
    code: "CHF",
    minor_units: 2,
    display_unit: 1,
    cash_unit: 5,
};
pub const KWD: Currency = Currency {
    code: "KWD",
    minor_units: 3,
    display_unit: 1,
    cash_unit: 1,
};

/// How a provider writes an amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireUnit {
    /// Integer minor units: EUR 10.50 is `1050` (Stripe).
    Minor,
    /// Major units as a decimal: EUR 10.50 is `"10.50"`, KRW 1000 is `"1000"` (Toss Payments, PortOne).
    MajorDecimal,
    /// Integer major units; an amount with a fraction is refused (M-Pesa).
    WholeMajor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireAmount {
    Int(i64),
    Decimal(String),
}

/// `m` in `c`'s minor units, written the way `u` says. Fails on a fraction under [`WireUnit::WholeMajor`] and on a
/// currency with more decimals than an `i64` can scale.
pub fn wire_amount(m: Money, c: &Currency, u: WireUnit) -> Result<WireAmount, DomainError> {
    let scale = 10_i64
        .checked_pow(u32::from(c.minor_units))
        .ok_or(DomainError::OutOfRange("currency minor units"))?;
    match u {
        WireUnit::Minor => Ok(WireAmount::Int(m.0)),
        WireUnit::WholeMajor if m.0 % scale != 0 => Err(DomainError::OutOfRange("amount has a fraction of the major unit")),
        WireUnit::WholeMajor => Ok(WireAmount::Int(m.0 / scale)),
        WireUnit::MajorDecimal => {
            let sign = if m.0 < 0 { "-" } else { "" };
            let abs = m.0.unsigned_abs();
            let scale = scale.unsigned_abs();
            Ok(WireAmount::Decimal(match c.minor_units {
                0 => format!("{sign}{abs}"),
                d => format!("{sign}{}.{:0w$}", abs / scale, abs % scale, w = usize::from(d)),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(m: i64, c: &Currency) -> WireAmount {
        wire_amount(Money(m), c, WireUnit::MajorDecimal).unwrap()
    }

    fn d(s: &str) -> WireAmount {
        WireAmount::Decimal(s.to_string())
    }

    #[test]
    fn zero_decimal_currency() {
        assert_eq!(wire_amount(Money(55_000), &KRW, WireUnit::Minor), Ok(WireAmount::Int(55_000)));
        assert_eq!(dec(55_000, &KRW), d("55000"));
        assert_eq!(dec(0, &KRW), d("0"));
        assert_eq!(dec(-1_000, &JPY), d("-1000"));
        assert_eq!(wire_amount(Money(55_000), &KRW, WireUnit::WholeMajor), Ok(WireAmount::Int(55_000)));
    }

    #[test]
    fn two_decimal_currency() {
        assert_eq!(wire_amount(Money(1_050), &EUR, WireUnit::Minor), Ok(WireAmount::Int(1_050)));
        assert_eq!(dec(1_050, &EUR), d("10.50"));
        assert_eq!(dec(5, &CHF), d("0.05"));
        assert_eq!(dec(-5, &EUR), d("-0.05"));
        assert_eq!(dec(0, &EUR), d("0.00"));
        assert_eq!(wire_amount(Money(1_000), &EUR, WireUnit::WholeMajor), Ok(WireAmount::Int(10)));
        assert!(wire_amount(Money(1_050), &EUR, WireUnit::WholeMajor).is_err());
    }

    #[test]
    fn three_decimal_currency() {
        assert_eq!(wire_amount(Money(1_234), &KWD, WireUnit::Minor), Ok(WireAmount::Int(1_234)));
        assert_eq!(dec(1_234, &KWD), d("1.234"));
        assert_eq!(dec(7, &KWD), d("0.007"));
        assert_eq!(dec(-10_500, &KWD), d("-10.500"));
        assert_eq!(wire_amount(Money(3_000), &KWD, WireUnit::WholeMajor), Ok(WireAmount::Int(3)));
        assert!(wire_amount(Money(3_001), &KWD, WireUnit::WholeMajor).is_err());
    }

    #[test]
    fn extremes_do_not_overflow() {
        assert_eq!(dec(i64::MIN, &EUR), d("-92233720368547758.08"));
        assert_eq!(dec(i64::MAX, &KRW), d("9223372036854775807"));
        let wide = Currency { minor_units: 19, ..EUR };
        assert!(wire_amount(Money(1), &wide, WireUnit::Minor).is_err());
    }
}
