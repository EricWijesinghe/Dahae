use crate::{DomainError, Won, catalog::Service, coupon::Coupon};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FULL_BPS: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceBreakdown {
    pub list_price: Won,
    pub discount: Won,
    pub total: Won,
    pub minutes: u32,
}

/// Server-authoritative price for a fixed / unit booking (paid in full).
/// `peak_bps` is the slot's surcharge rate (0 when peak does not apply). The surcharge is folded into
/// `list_price` so that `list_price - discount == total` holds for every stored booking.
pub fn price_booking(
    service: &Service,
    unit_index: Option<usize>,
    coupon: Option<&Coupon>,
    peak_bps: i64,
    now: DateTime<Utc>,
) -> Result<PriceBreakdown, DomainError> {
    if !(0..=FULL_BPS).contains(&peak_bps) {
        return Err(DomainError::OutOfRange("peak_bps"));
    }
    let (base, minutes) = service.list_price(unit_index)?;
    let list_price = base + base.times_bps(peak_bps).round100();
    let discount = match coupon {
        Some(c) => c.discount_for(list_price, &service.code, now)?,
        None => Won::ZERO,
    };
    Ok(PriceBreakdown {
        list_price,
        discount,
        total: list_price.saturating_sub(discount),
        minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::default_catalog;
    use chrono::TimeZone;

    fn bathroom() -> Service {
        default_catalog().into_iter().find(|s| s.code == "bathroom").unwrap()
    }

    #[test]
    fn bathroom_with_welcome_coupon() {
        let now = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
        let c = Coupon::issue(Won(5_000), None, now);
        let p = price_booking(&bathroom(), Some(1), Some(&c), 0, now).unwrap();
        assert_eq!((p.list_price, p.discount, p.total, p.minutes), (Won(52_000), Won(5_000), Won(47_000), 120));
    }

    #[test]
    fn peak_surcharge_is_added_to_the_list_price_before_the_coupon() {
        // 20% of 52,000 = 10,400 (already a multiple of 100); the 5,000 coupon applies to the surcharged price.
        let now = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
        let c = Coupon::issue(Won(5_000), None, now);
        let p = price_booking(&bathroom(), Some(1), Some(&c), 2_000, now).unwrap();
        assert_eq!((p.list_price, p.discount, p.total), (Won(62_400), Won(5_000), Won(57_400)));
        assert_eq!(p.list_price - p.discount, p.total);
    }

    #[test]
    fn peak_surcharge_rounds_to_the_hundred_won_like_quotes() {
        // 11% of 52,000 = 5,720, rounded half-up to the hundred: 5,700.
        let now = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
        let p = price_booking(&bathroom(), Some(1), None, 1_100, now).unwrap();
        assert_eq!(p.list_price, Won(57_700));
    }

    #[test]
    fn out_of_range_peak_is_rejected() {
        let now = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
        assert_eq!(price_booking(&bathroom(), Some(1), None, 10_001, now), Err(DomainError::OutOfRange("peak_bps")));
        assert_eq!(price_booking(&bathroom(), Some(1), None, -1, now), Err(DomainError::OutOfRange("peak_bps")));
    }
}
