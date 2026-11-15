//! Conversion and retention rules: welcome offer, referral rewards, honest social-proof numbers.
//! Pure functions; the API decides the facts (booking counts, payment state) and passes them in.

use crate::{DomainError, Won, pricing::PriceBreakdown};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Social-proof counts stay hidden below this, so a tiny number never persuades.
pub const MIN_PROOF_COUNT: i64 = 10;
/// Seats are shown as scarce only when this many or fewer remain.
pub const SCARCE_SEATS: u32 = 2;
const MAX_PERCENT: i64 = 50;
const MAX_AMOUNT: i64 = 100_000;
const PERCENT_BASE: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscountKind {
    Amount,
    Percent,
}

/// Owner-configured first-booking discount. Off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WelcomeOffer {
    pub enabled: bool,
    pub kind: DiscountKind,
    pub value: i64,
}

impl Default for WelcomeOffer {
    fn default() -> Self {
        Self { enabled: false, kind: DiscountKind::Amount, value: 5_000 }
    }
}

impl WelcomeOffer {
    pub fn validate(&self) -> Result<(), DomainError> {
        let (min, max) = match self.kind {
            DiscountKind::Amount => (1, MAX_AMOUNT),
            DiscountKind::Percent => (1, MAX_PERCENT),
        };
        if (min..=max).contains(&self.value) { Ok(()) } else { Err(DomainError::OutOfRange("welcome_value")) }
    }

    /// Discount for a list price, never more than the price itself.
    pub fn discount_for(&self, list: Won) -> Won {
        if !self.enabled {
            return Won::ZERO;
        }
        match self.kind {
            DiscountKind::Amount => Won(self.value).min(list),
            DiscountKind::Percent => Won(list.0 * self.value / PERCENT_BASE).round100().min(list),
        }
    }
}

/// The welcome offer applies to the customer's first booking of any status (a cancelled
/// first booking still counts, so cancel-and-rebook can't repeat it) and never on top of a coupon.
pub fn welcome_applies(offer: &WelcomeOffer, prior_bookings: i64, has_coupon: bool) -> bool {
    offer.enabled && prior_bookings == 0 && !has_coupon
}

/// Applies the welcome discount to a server-computed price (the list price stays untouched).
pub fn apply_welcome(price: PriceBreakdown, offer: &WelcomeOffer) -> PriceBreakdown {
    let discount = offer.discount_for(price.list_price);
    PriceBreakdown {
        discount,
        total: price.list_price.saturating_sub(discount),
        ..price
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferralOffer {
    pub enabled: bool,
    /// Won given to both the friend and the referrer.
    pub amount: i64,
}

impl Default for ReferralOffer {
    fn default() -> Self {
        Self { enabled: false, amount: 5_000 }
    }
}

impl ReferralOffer {
    pub fn validate(&self) -> Result<(), DomainError> {
        if (1..=MAX_AMOUNT).contains(&self.amount) { Ok(()) } else { Err(DomainError::OutOfRange("referral_amount")) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferralError {
    SelfReferral,
    AlreadyBooked,
    AlreadyRewarded,
    NotPaid,
}

impl ReferralError {
    pub fn code(self) -> &'static str {
        match self {
            Self::SelfReferral => "self_referral",
            Self::AlreadyBooked => "referee_already_booked",
            Self::AlreadyRewarded => "already_rewarded",
            Self::NotPaid => "not_paid",
        }
    }
}

/// A friend may claim a code only with their own phone and before any booking exists.
pub fn check_claim(referrer_phone: &str, friend_phone: &str, friend_bookings: i64) -> Result<(), ReferralError> {
    if referrer_phone == friend_phone {
        return Err(ReferralError::SelfReferral);
    }
    if friend_bookings > 0 {
        return Err(ReferralError::AlreadyBooked);
    }
    Ok(())
}

/// Rewards fire once, when a friend's booking is completed and paid in full.
pub fn check_reward(already_rewarded: bool, paid_in_full: bool) -> Result<(), ReferralError> {
    if already_rewarded {
        return Err(ReferralError::AlreadyRewarded);
    }
    if !paid_in_full {
        return Err(ReferralError::NotPaid);
    }
    Ok(())
}

/// Paid in full: some money came in, and a quoted job's final price is covered.
pub fn paid_in_full(paid: i64, final_price: Option<i64>) -> bool {
    // No known price means we cannot tell it was paid in full: no reward.
    final_price.is_some_and(|p| paid > 0 && paid >= p)
}

/// Shows a social-proof count only once it is persuasive.
pub fn proof_count(n: i64) -> Option<i64> {
    (n >= MIN_PROOF_COUNT).then_some(n)
}

/// "n자리 남음" is shown only when a slot has a real capacity and 1..=2 seats are left.
pub fn seats_left_to_show(capacity: Option<u32>, used: u32) -> Option<u32> {
    let left = capacity?.saturating_sub(used);
    (1..=SCARCE_SEATS).contains(&left).then_some(left)
}

/// Korean-style masking: 김민수 -> 김*수, 김민 -> 김*, 김 -> 김*. Empty -> generic label.
pub fn mask_name(name: &str) -> String {
    let chars: Vec<char> = name.trim().chars().collect();
    match chars.as_slice() {
        [] => "고객".into(),
        [only] => format!("{only}*"),
        [first, .., last] => {
            let middle = "*".repeat(chars.len() - 2);
            if chars.len() == 2 { format!("{first}*") } else { format!("{first}{middle}{last}") }
        }
    }
}

/// Public review text without contact details: runs of 4+ digits (phone numbers, unit numbers)
/// and anything shaped like an e-mail address become `***`.
pub fn redact_contacts(text: &str) -> String {
    let words: Vec<String> = text
        .split(' ')
        .map(|w| if w.contains('@') && w.contains('.') { "***".to_string() } else { w.to_string() })
        .collect();
    let joined = words.join(" ");
    let mut out = String::with_capacity(joined.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.chars().filter(char::is_ascii_digit).count() >= 4 { out.push_str("***") } else { out.push_str(run) }
        run.clear();
    };
    for c in joined.chars() {
        if c.is_ascii_digit() || (!run.is_empty() && (c == '-' || c == '.')) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Refund, discount and compensation words. A reply may use one only when the facts use it too.
const CLAIM_TERMS: [&str; 8] = ["환불", "할인", "쿠폰", "보상", "refund", "discount", "coupon", "compensat"];

/// Maximal ASCII digit runs, commas and signs removed, so `60000` and `60,000` compare equal.
fn digit_runs(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(|s| s.trim_start_matches('0').to_string()).collect()
}

/// Won amounts a reply states, as plain digit strings: `9,900원` -> `9900`, `6만원` -> `60000`.
fn won_amounts(reply: &str) -> Vec<String> {
    let chars: Vec<char> = reply.chars().collect();
    let mut amounts = Vec::new();
    for i in (0..chars.len()).filter(|&i| chars[i] == '원') {
        let mut end = i;
        while end > 0 && chars[end - 1] == ' ' {
            end -= 1;
        }
        let man = end > 0 && chars[end - 1] == '만';
        if man {
            end -= 1;
            while end > 0 && chars[end - 1] == ' ' {
                end -= 1;
            }
        }
        let mut start = end;
        while start > 0 && (chars[start - 1].is_ascii_digit() || chars[start - 1] == ',') {
            start -= 1;
        }
        let digits: String = chars[start..end].iter().filter(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let zeros = if man { "0000" } else { "" };
            amounts.push(format!("{}{zeros}", digits.trim_start_matches('0')));
        }
    }
    amounts
}

/// True when a model reply states a won amount, or a refund or discount term, that the grounding
/// facts (shop data and the customer's booking, as JSON text) do not contain. Such replies are not shown.
pub fn ungrounded_claim(reply: &str, facts: &str) -> bool {
    let known = digit_runs(facts);
    let facts_lower = facts.to_lowercase();
    let reply_lower = reply.to_lowercase();
    won_amounts(reply).iter().any(|amount| !known.contains(amount))
        || CLAIM_TERMS.iter().any(|term| reply_lower.contains(term) && !facts_lower.contains(term))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(list: i64) -> PriceBreakdown {
        PriceBreakdown { list_price: Won(list), discount: Won::ZERO, total: Won(list), minutes: 60 }
    }

    #[test]
    fn welcome_is_first_booking_only_and_never_stacks_on_coupons() {
        let on = WelcomeOffer { enabled: true, kind: DiscountKind::Amount, value: 5_000 };
        assert!(welcome_applies(&on, 0, false));
        assert!(!welcome_applies(&on, 1, false), "second booking");
        assert!(!welcome_applies(&on, 0, true), "coupon wins, no stacking");
        assert!(!welcome_applies(&WelcomeOffer { enabled: false, ..on }, 0, false), "switched off");
    }

    #[test]
    fn welcome_discount_is_capped_and_rounded_server_side() {
        let amount = WelcomeOffer { enabled: true, kind: DiscountKind::Amount, value: 10_000 };
        assert_eq!(amount.discount_for(Won(7_000)), Won(7_000), "never below zero");
        let pct = WelcomeOffer { enabled: true, kind: DiscountKind::Percent, value: 10 };
        assert_eq!(pct.discount_for(Won(52_000)), Won(5_200).round100());
        assert_eq!(WelcomeOffer { enabled: false, ..pct }.discount_for(Won(52_000)), Won::ZERO);
        let p = apply_welcome(price(52_000), &amount);
        assert_eq!((p.discount, p.total), (Won(10_000), Won(42_000)));
    }

    #[test]
    fn welcome_config_bounds() {
        assert!(WelcomeOffer { enabled: true, kind: DiscountKind::Percent, value: 51 }.validate().is_err());
        assert!(WelcomeOffer { enabled: true, kind: DiscountKind::Percent, value: 50 }.validate().is_ok());
        assert!(WelcomeOffer { enabled: true, kind: DiscountKind::Amount, value: 0 }.validate().is_err());
        assert!(ReferralOffer { enabled: true, amount: 100_001 }.validate().is_err());
    }

    #[test]
    fn referral_claim_rules() {
        assert_eq!(check_claim("01011112222", "01011112222", 0), Err(ReferralError::SelfReferral));
        assert_eq!(check_claim("01011112222", "01033334444", 1), Err(ReferralError::AlreadyBooked));
        assert_eq!(check_claim("01011112222", "01033334444", 0), Ok(()));
    }

    #[test]
    fn referral_reward_once_and_only_when_paid() {
        assert_eq!(check_reward(false, false), Err(ReferralError::NotPaid));
        assert_eq!(check_reward(true, true), Err(ReferralError::AlreadyRewarded));
        assert_eq!(check_reward(false, true), Ok(()));
        assert!(paid_in_full(47_000, Some(47_000)), "fixed service paid up front");
        assert!(!paid_in_full(47_000, None), "unknown price never counts as paid in full");
        assert!(!paid_in_full(0, None), "nothing paid");
        assert!(!paid_in_full(10_000, Some(50_000)), "quote deposit only");
        assert!(paid_in_full(50_000, Some(50_000)), "quote fully paid");
    }

    #[test]
    fn social_proof_is_hidden_when_small() {
        assert_eq!(proof_count(9), None);
        assert_eq!(proof_count(10), Some(10));
    }

    #[test]
    fn scarcity_only_when_truly_low() {
        assert_eq!(seats_left_to_show(Some(4), 2), Some(2));
        assert_eq!(seats_left_to_show(Some(4), 3), Some(1));
        assert_eq!(seats_left_to_show(Some(4), 0), None, "plenty left");
        assert_eq!(seats_left_to_show(Some(4), 4), None, "full: handled as SlotFull, not scarcity");
        assert_eq!(seats_left_to_show(None, 99), None, "unlimited slot");
    }

    #[test]
    fn names_are_masked() {
        assert_eq!(mask_name("김민수"), "김*수");
        assert_eq!(mask_name("김민"), "김*");
        assert_eq!(mask_name("김"), "김*");
        assert_eq!(mask_name("  "), "고객");
        assert_eq!(mask_name("Anna Lee"), "A******e");
    }

    #[test]
    fn public_reviews_hide_phone_numbers_and_emails() {
        assert_eq!(redact_contacts("친절해요 010-1234-5678 연락주세요"), "친절해요 *** 연락주세요");
        assert_eq!(redact_contacts("문의 kim@mail.com 으로"), "문의 *** 으로");
        assert_eq!(redact_contacts("3시간 걸렸고 2번 와주셨어요"), "3시간 걸렸고 2번 와주셨어요");
        assert_eq!(redact_contacts("101동 1203호"), "101동 ***호");
    }

    #[test]
    fn replies_stating_unlisted_amounts_or_refund_terms_are_ungrounded() {
        let facts = r#"{"pricing":{"kind":"fixed","price":60000},"final_price":null}"#;
        assert!(!ungrounded_claim("스튜디오 청소는 60,000원이에요.", facts));
        assert!(!ungrounded_claim("60000원입니다", facts));
        assert!(ungrounded_claim("환불 금액은 99,000원입니다.", facts));
        assert!(ungrounded_claim("6000원 할인 가능해요", facts));
        assert!(!ungrounded_claim("6만원까지 가능해요", facts), "6만원 is 60000, which is listed");
        assert!(ungrounded_claim("7만원까지 가능해요", facts));
        assert!(ungrounded_claim("refund will be issued", facts));
        assert!(!ungrounded_claim("2시간 걸리고 3명이 가요", facts));
    }

    #[test]
    fn terms_pass_only_when_the_facts_name_them() {
        let facts = r#"{"refund":"5 days before visit"}"#;
        assert!(!ungrounded_claim("환불 정책은 방문 5일 전까지예요", "환불 정책 5일"));
        assert!(!ungrounded_claim("refund rules apply", facts));
        assert!(ungrounded_claim("쿠폰을 드릴게요", facts));
    }
}
