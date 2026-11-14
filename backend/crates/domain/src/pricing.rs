use crate::{
    DomainError, Won,
    catalog::{Pricing, Service},
    coupon::Coupon,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceBreakdown {
    /// Catalogue sum before any discount.
    pub list_price: Won,
    /// Multi-item discount for counts services (zero otherwise).
    pub multi_discount: Won,
    /// Coupon or welcome discount, taken after the multi-item discount.
    pub discount: Won,
    /// `list_price - multi_discount - discount`.
    pub total: Won,
    pub minutes: u32,
}

impl PriceBreakdown {
    /// Every discount taken off the list price, as stored in `bookings.discount`.
    pub fn total_discount(&self) -> Won {
        Won(self.multi_discount.0 + self.discount.0)
    }
}

/// Server-authoritative price for a fixed / unit booking (paid in full).
pub fn price_booking(service: &Service, unit_index: Option<usize>, coupon: Option<&Coupon>, now: DateTime<Utc>) -> Result<PriceBreakdown, DomainError> {
    price_booking_with(service, unit_index, &[], coupon, now)
}

/// Like `price_booking`, with one count per option for counts services (`counts` wins over `unit_index`).
pub fn price_booking_with(
    service: &Service,
    unit_index: Option<usize>,
    counts: &[u8],
    coupon: Option<&Coupon>,
    now: DateTime<Utc>,
) -> Result<PriceBreakdown, DomainError> {
    let (list_price, multi_discount, minutes) = match &service.pricing {
        Pricing::Counts { .. } => {
            let c = service.count_price(&service.counts_for(unit_index, counts)?)?;
            (c.list, c.multi_discount, c.minutes)
        }
        _ if counts.is_empty() => {
            let (list, minutes) = service.list_price(unit_index)?;
            (list, Won::ZERO, minutes)
        }
        _ => return Err(DomainError::InvalidUnit),
    };
    let after_multi = list_price.saturating_sub(multi_discount);
    let discount = match coupon {
        Some(c) => c.discount_for(after_multi, &service.code, now)?,
        None => Won::ZERO,
    };
    Ok(PriceBreakdown {
        list_price,
        multi_discount,
        discount,
        total: after_multi.saturating_sub(discount),
        minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::default_catalog;
    use chrono::TimeZone;

    #[test]
    fn bathroom_with_welcome_coupon() {
        let now = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
        let bath = default_catalog().into_iter().find(|s| s.code == "bathroom").unwrap();
        let c = Coupon::issue(Won(5_000), None, now);
        let p = price_booking(&bath, Some(1), Some(&c), now).unwrap();
        assert_eq!(
            (p.list_price, p.multi_discount, p.discount, p.total, p.minutes),
            (Won(52_000), Won(0), Won(5_000), Won(47_000), 120)
        );
    }

    fn by_code(code: &str) -> Service {
        default_catalog().into_iter().find(|s| s.code == code).unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap()
    }

    #[test]
    fn counts_example_aircon_total_and_minutes() {
        let p = price_booking_with(&by_code("aircon"), None, &[0, 2, 1, 0], None, now()).unwrap();
        // 2 stand + 1 two_in_one is 417,000 before the discount; 10% off = 41,700, total 375,300.
        assert_eq!(
            (p.list_price, p.multi_discount, p.total, p.minutes),
            (Won(417_000), Won(41_700), Won(375_300), 420)
        );
    }

    #[test]
    fn counts_example_stand_two_wall_one() {
        let p = price_booking_with(&by_code("aircon"), None, &[1, 2, 0, 0], None, now()).unwrap();
        assert_eq!(
            (p.list_price, p.multi_discount, p.total, p.minutes),
            (Won(317_000), Won(31_700), Won(285_300), 330)
        );
    }

    #[test]
    fn counts_example_window_three_small_two_large() {
        let p = price_booking_with(&by_code("window"), None, &[3, 2], None, now()).unwrap();
        assert_eq!((p.list_price, p.minutes), (Won(57_000), 95));
    }

    #[test]
    fn counts_example_disinfection_pest_and_sanitize() {
        let p = price_booking_with(&by_code("disinfection"), None, &[1, 0, 1, 0], None, now()).unwrap();
        assert_eq!((p.list_price, p.multi_discount, p.total), (Won(180_000), Won(18_000), Won(162_000)));
        assert_eq!(p.total_discount(), Won(18_000));
    }

    #[test]
    fn legacy_mattress_unit_index_prices_one_mattress() {
        let p = price_booking(&by_code("mattress"), Some(0), None, now()).unwrap();
        assert_eq!((p.list_price, p.minutes), (Won(59_000), 60));
        assert_eq!(price_booking(&by_code("mattress"), Some(1), None, now()), Err(DomainError::InvalidUnit));
    }

    #[test]
    fn counts_mattress_two_pieces_take_fifteen_percent_off() {
        // 2 x 59,000 = 118,000, -15% = 100,300 (rounded to 100 won).
        let p = price_booking_with(&by_code("mattress"), None, &[2], None, now()).unwrap();
        assert_eq!((p.list_price, p.multi_discount, p.total), (Won(118_000), Won(17_700), Won(100_300)));
    }

    #[test]
    fn counts_rejected_on_a_unit_service() {
        assert_eq!(
            price_booking_with(&by_code("bathroom"), Some(0), &[1, 0, 0, 0], None, now()),
            Err(DomainError::InvalidUnit)
        );
    }
}
