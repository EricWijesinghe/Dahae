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
        Self {
            enabled: false,
            kind: DiscountKind::Amount,
            value: 5_000,
        }
    }
}

impl WelcomeOffer {
    pub fn validate(&self) -> Result<(), DomainError> {
        let (min, max) = match self.kind {
            DiscountKind::Amount => (1, MAX_AMOUNT),
            DiscountKind::Percent => (1, MAX_PERCENT),
        };
        if (min..=max).contains(&self.value) {
            Ok(())
        } else {
            Err(DomainError::OutOfRange("welcome_value"))
        }
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
    let after_multi = price.list_price.saturating_sub(price.multi_discount);
    let discount = offer.discount_for(after_multi);
    PriceBreakdown {
        discount,
        total: after_multi.saturating_sub(discount),
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
        if (1..=MAX_AMOUNT).contains(&self.amount) {
            Ok(())
        } else {
            Err(DomainError::OutOfRange("referral_amount"))
        }
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

/// What a booking owes: the quoted final price, else the list price less every discount taken.
pub fn owed_amount(final_price: Option<i64>, list_price: Option<i64>, discount: i64) -> Option<i64> {
    final_price.or_else(|| list_price.map(|list| list - discount))
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
            if chars.len() == 2 {
                format!("{first}*")
            } else {
                format!("{first}{middle}{last}")
            }
        }
    }
}

/// Public review text without contact details: runs of 4+ digits (phone numbers, unit numbers, also written
/// with spaces, dots, dashes or full-width digits) and anything shaped like an e-mail address become `***`.
pub fn redact_contacts(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').map(|w| if w.contains('@') && w.contains('.') { "***" } else { w }).collect();
    crate::redact::mask_digit_runs(&words.join(" "), 4, "***", false)
}

/// Refund, discount and compensation words. A reply may use one only when the facts use it too.
const CLAIM_TERMS: [&str; 8] = ["환불", "할인", "쿠폰", "보상", "refund", "discount", "coupon", "compensat"];

/// Korean digit words 일..구 (1..9), the value a single word stands for.
fn korean_digit(c: char) -> Option<u64> {
    ['일', '이', '삼', '사', '오', '육', '칠', '팔', '구']
        .iter()
        .position(|&k| k == c)
        .map(|i| i as u64 + 1)
}

/// Small units 십, 백, 천 and big units 만, 억 that spell amounts such as `3천` or `6만 3천`.
fn small_unit(c: char) -> Option<u64> {
    match c {
        '십' => Some(10),
        '백' => Some(100),
        '천' => Some(1_000),
        _ => None,
    }
}

fn big_unit(c: char) -> Option<u64> {
    match c {
        '만' => Some(10_000),
        '억' => Some(100_000_000),
        _ => None,
    }
}

fn is_amount_char(c: char) -> bool {
    c.is_ascii_digit() || c == ',' || korean_digit(c).is_some() || small_unit(c).is_some() || big_unit(c).is_some()
}

/// A Korean digit word counts only when a unit follows it (`구천`, `삼만`) or it ends the amount, so
/// the particle or noun before a number (`이 3천`, `2일 3만`) is not read as a digit.
fn word_is_digit(chars: &[char], i: usize) -> bool {
    match chars[i + 1..].iter().find(|&&c| c != ' ') {
        None => true,
        Some(&next) => small_unit(next).is_some() || big_unit(next).is_some(),
    }
}

/// Value of a spelled or digit amount: `9,900` -> 9900, `3천` -> 3000, `6만 3천` -> 63000. None on overflow.
fn spelled_value(run: &str) -> Option<u64> {
    let chars: Vec<char> = run.chars().collect();
    let (mut total, mut section, mut num) = (0u64, 0u64, 0u64);
    for (i, &c) in chars.iter().enumerate() {
        if let Some(d) = c.to_digit(10) {
            num = num.checked_mul(10)?.checked_add(u64::from(d))?;
        } else if let Some(d) = korean_digit(c).filter(|_| word_is_digit(&chars, i)) {
            num = d;
        } else if let Some(unit) = small_unit(c) {
            section = section.checked_add(num.max(1).checked_mul(unit)?)?;
            num = 0;
        } else if let Some(unit) = big_unit(c) {
            section = section.checked_add(num)?;
            total = total.checked_add(section.max(1).checked_mul(unit)?)?;
            section = 0;
            num = 0;
        }
    }
    total.checked_add(section)?.checked_add(num)
}

/// Amount text before `end`, spaces allowed inside (`6만 3천원`), so `2명 9900원` yields ` 9900`.
fn run_before(chars: &[char], end: usize) -> String {
    let mut start = end;
    while start > 0 && (is_amount_char(chars[start - 1]) || chars[start - 1] == ' ') {
        start -= 1;
    }
    chars[start..end].iter().collect()
}

/// Digits from `start` on, leading spaces skipped (`₩60,000`, `KRW 60000`). Spelled words stop it,
/// since a particle such as `이` often follows the figure (`₩60,000이에요`).
fn run_after(chars: &[char], start: usize) -> String {
    chars
        .iter()
        .skip(start)
        .skip_while(|&&c| c == ' ')
        .take_while(|&&c| c.is_ascii_digit() || c == ',')
        .collect()
}

/// True when `word` (lowercase ASCII) starts at `i` as a whole word, so `won` does not match `wonder`.
fn word_at(lower: &[char], i: usize, word: &str) -> bool {
    let len = word.chars().count();
    word.chars().enumerate().all(|(k, w)| lower.get(i + k) == Some(&w)) && !lower.get(i + len).is_some_and(|c| c.is_ascii_alphabetic())
}

/// Won amounts a reply states, as numbers: `9,900원`, `6만원`, `3천원`, `₩60,000`, `60000 KRW`, `60,000 won`.
/// Zero is not an amount to check.
fn won_amounts(text: &str) -> Vec<u64> {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let mut runs = Vec::new();
    for i in 0..chars.len() {
        if chars[i] == '원' || word_at(&lower, i, "krw") || word_at(&lower, i, "won") {
            runs.push(run_before(&chars, i));
        }
        if chars[i] == '₩' {
            runs.push(run_after(&chars, i + 1));
        }
        if word_at(&lower, i, "krw") {
            runs.push(run_after(&chars, i + 3));
        }
    }
    runs.iter().filter_map(|r| spelled_value(r)).filter(|&v| v > 0).collect()
}

/// Every number the facts state: plain digit runs (`60000`, `60,000`) and won amounts spelled out.
fn known_amounts(facts: &str) -> HashSet<u64> {
    let mut known: HashSet<u64> = facts.split(|c: char| !c.is_ascii_digit()).filter_map(|s| s.parse().ok()).collect();
    known.extend(won_amounts(facts));
    known
}

/// True when a model reply states a won amount, or a refund or discount term, that the grounding
/// facts (shop data and the customer's booking, as JSON text) do not contain. Such replies are not shown.
pub fn ungrounded_claim(reply: &str, facts: &str) -> bool {
    let known = known_amounts(facts);
    let facts_lower = facts.to_lowercase();
    let reply_lower = reply.to_lowercase();
    won_amounts(reply).iter().any(|amount| !known.contains(amount)) || CLAIM_TERMS.iter().any(|term| reply_lower.contains(term) && !facts_lower.contains(term))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(list: i64) -> PriceBreakdown {
        PriceBreakdown {
            list_price: Won(list),
            multi_discount: Won::ZERO,
            discount: Won::ZERO,
            total: Won(list),
            minutes: 60,
        }
    }

    #[test]
    fn welcome_is_first_booking_only_and_never_stacks_on_coupons() {
        let on = WelcomeOffer {
            enabled: true,
            kind: DiscountKind::Amount,
            value: 5_000,
        };
        assert!(welcome_applies(&on, 0, false));
        assert!(!welcome_applies(&on, 1, false), "second booking");
        assert!(!welcome_applies(&on, 0, true), "coupon wins, no stacking");
        assert!(!welcome_applies(&WelcomeOffer { enabled: false, ..on }, 0, false), "switched off");
    }

    #[test]
    fn welcome_discount_is_capped_and_rounded_server_side() {
        let amount = WelcomeOffer {
            enabled: true,
            kind: DiscountKind::Amount,
            value: 10_000,
        };
        assert_eq!(amount.discount_for(Won(7_000)), Won(7_000), "never below zero");
        let pct = WelcomeOffer {
            enabled: true,
            kind: DiscountKind::Percent,
            value: 10,
        };
        assert_eq!(pct.discount_for(Won(52_000)), Won(5_200).round100());
        assert_eq!(WelcomeOffer { enabled: false, ..pct }.discount_for(Won(52_000)), Won::ZERO);
        let p = apply_welcome(price(52_000), &amount);
        assert_eq!((p.discount, p.total), (Won(10_000), Won(42_000)));
    }

    #[test]
    fn welcome_config_bounds() {
        assert!(
            WelcomeOffer {
                enabled: true,
                kind: DiscountKind::Percent,
                value: 51
            }
            .validate()
            .is_err()
        );
        assert!(
            WelcomeOffer {
                enabled: true,
                kind: DiscountKind::Percent,
                value: 50
            }
            .validate()
            .is_ok()
        );
        assert!(
            WelcomeOffer {
                enabled: true,
                kind: DiscountKind::Amount,
                value: 0
            }
            .validate()
            .is_err()
        );
        assert!(
            ReferralOffer {
                enabled: true,
                amount: 100_001
            }
            .validate()
            .is_err()
        );
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
    fn owed_is_the_price_after_discounts() {
        assert_eq!(owed_amount(None, Some(52_000), 15_000), Some(37_000), "fixed price after coupon");
        assert_eq!(owed_amount(Some(40_000), Some(52_000), 15_000), Some(40_000), "quoted price wins");
        assert_eq!(owed_amount(None, None, 0), None, "no price known");
        assert!(
            paid_in_full(37_000, owed_amount(None, Some(52_000), 15_000)),
            "paid what it cost after discount"
        );
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
    fn public_reviews_hide_phone_numbers_written_with_spaces() {
        assert_eq!(redact_contacts("연락처 010 12 34 56 78"), "연락처 ***");
        assert_eq!(redact_contacts("010 1234 5678"), "***");
        assert_eq!(redact_contacts("2 3 번 와주셨어요"), "2 3 번 와주셨어요");
    }

    #[test]
    fn public_reviews_hide_full_width_and_punctuated_phone_numbers() {
        assert_eq!(redact_contacts("연락처 ０１０-１２３４-５６７８ 주세요"), "연락처 *** 주세요");
        assert_eq!(redact_contacts("(010) 1234-5678로 연락"), "(***로 연락");
        assert_eq!(redact_contacts("101동　１２０３호"), "101동　***호");
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

    #[test]
    fn spelled_and_symbol_amounts_are_checked_too() {
        let facts = r#"{"pricing":{"kind":"fixed","price":60000}}"#;
        assert!(ungrounded_claim("3천원에 해드려요", facts));
        assert!(ungrounded_claim("6만 3천원이에요", facts));
        assert!(ungrounded_claim("구천원이면 돼요", facts));
        assert!(ungrounded_claim("₩63,000 입니다", facts));
        assert!(ungrounded_claim("KRW 63000 입니다", facts));
        assert!(!ungrounded_claim("₩60,000이에요", facts));
        assert!(!ungrounded_claim("60000 KRW입니다", facts));
        assert!(!ungrounded_claim("60,000 won이에요", facts));
        assert!(!ungrounded_claim("6만원이에요", facts));
        assert!(!ungrounded_claim("2명 6만원이에요", facts));
        assert!(!ungrounded_claim("0원이에요", facts), "zero is not an amount");
        assert!(!ungrounded_claim("3 wonderful days", facts), "won must be a whole word");
        assert!(!ungrounded_claim("3천원이에요", r#"{"fee":"3천원"}"#), "spelled amounts in facts count too");
        assert!(!ungrounded_claim("이 6만원이면 돼요", facts), "a particle before the amount is not a digit");
    }

    #[test]
    fn refund_terms_pass_when_shop_data_states_them() {
        let facts = r#"{"policy":"방문 5일 전 환불 가능"}"#;
        assert!(!ungrounded_claim("환불은 방문 5일 전까지 가능해요", facts));
    }
}
