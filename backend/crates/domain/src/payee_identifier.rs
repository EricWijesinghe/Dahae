//! Payee identifier for the 지급명세서: 주민등록번호 (individual) or 사업자등록번호 (business). Pure checks and masking.

use crate::{DomainError, cash_receipt::normalize_business_number};

const RRN_DIGITS: usize = 13;
const BUSINESS_DIGITS: usize = 10;
const VISIBLE_DIGITS: usize = 4;
/// 주민등록번호 check-digit weights for the first twelve digits.
const RRN_WEIGHTS: [u32; 12] = [2, 3, 4, 5, 6, 7, 8, 9, 2, 3, 4, 5];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayeeKind {
    Rrn,
    Business,
}

impl PayeeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rrn => "rrn",
            Self::Business => "business",
        }
    }

    fn digit_count(self) -> usize {
        match self {
            Self::Rrn => RRN_DIGITS,
            Self::Business => BUSINESS_DIGITS,
        }
    }
}

/// A checked identifier: its kind and its digits only (hyphens and spaces removed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayeeIdentifier {
    pub kind: PayeeKind,
    pub digits: String,
}

/// 주민등록번호: 13 digits; the last digit checks the first twelve (weights 2..5 repeating).
fn rrn_check_digit_ok(d: &[u8]) -> bool {
    let sum: u32 = d[..12].iter().zip(RRN_WEIGHTS).map(|(b, w)| u32::from(b - b'0') * w).sum();
    (11 - sum % 11) % 10 == u32::from(d[12] - b'0')
}

/// Accepts 13 digits with a valid 주민등록번호 check digit, or a valid 10-digit 사업자등록번호.
/// Hyphens and spaces are allowed as separators; any other character is refused.
pub fn normalize(raw: &str) -> Result<PayeeIdentifier, DomainError> {
    if !raw.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ') {
        return Err(DomainError::OutOfRange("payee_identifier"));
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == RRN_DIGITS && rrn_check_digit_ok(digits.as_bytes()) {
        return Ok(PayeeIdentifier { kind: PayeeKind::Rrn, digits });
    }
    if digits.len() == BUSINESS_DIGITS {
        normalize_business_number(&digits)?;
        return Ok(PayeeIdentifier {
            kind: PayeeKind::Business,
            digits,
        });
    }
    Err(DomainError::OutOfRange("payee_identifier"))
}

/// The last four digits, kept in clear beside the ciphertext so the console can show a masked value.
pub fn last4(id: &PayeeIdentifier) -> &str {
    &id.digits[id.digits.len() - VISIBLE_DIGITS..]
}

/// Masked display form for a stored kind and last four digits, e.g. `*********4568`.
pub fn masked(kind: &str, last4: &str) -> Option<String> {
    let kind = match kind {
        "rrn" => PayeeKind::Rrn,
        "business" => PayeeKind::Business,
        _ => return None,
    };
    let hidden = kind.digit_count() - VISIBLE_DIGITS;
    Some(format!("{}{last4}", "*".repeat(hidden)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_rrn_with_a_valid_check_digit_with_or_without_hyphen() {
        let plain = normalize("9001011234568").expect("valid rrn");
        assert_eq!((plain.kind, plain.digits.as_str()), (PayeeKind::Rrn, "9001011234568"));
        assert_eq!(normalize("900101-1234568").expect("hyphenated").digits, "9001011234568");
    }

    #[test]
    fn refuses_rrn_with_a_wrong_check_digit() {
        assert!(normalize("9001011234567").is_err());
    }

    #[test]
    fn accepts_business_number_with_a_valid_check_digit() {
        let id = normalize("220-81-62517").expect("valid business number");
        assert_eq!((id.kind, id.digits.as_str()), (PayeeKind::Business, "2208162517"));
        assert!(normalize("2208162518").is_err(), "wrong check digit");
    }

    #[test]
    fn refuses_other_lengths_and_letters() {
        assert!(normalize("").is_err());
        assert!(normalize("12345").is_err());
        assert!(normalize("9001011234568x").is_err(), "letters are never stripped silently");
        assert!(normalize("900101123456789").is_err(), "15 digits");
    }

    #[test]
    fn masks_all_but_the_last_four_digits() {
        let id = normalize("9001011234568").expect("valid");
        assert_eq!(last4(&id), "4568");
        assert_eq!(masked("rrn", "4568").as_deref(), Some("*********4568"));
        assert_eq!(masked("business", "2517").as_deref(), Some("******2517"));
        assert_eq!(masked("other", "2517"), None);
    }
}
