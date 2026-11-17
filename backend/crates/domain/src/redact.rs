// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Contact-number and e-mail masking shared by public review text and stored client error reports.

use std::ops::Range;

/// Spaces allowed on each side of a separator between digit groups: "010  1234", "010 - 1234".
const GAP_SPACES: usize = 2;

/// Digit span `start..end` of one group in a run.
type Group = (usize, usize);

/// Full-width ASCII (U+FF01..U+FF5E) reads as ASCII and other spaces as a space, as NFKC folds them, so
/// "０１０-１２３４" is a number and "kim＠mail．com" an address. Dashes, the minus sign and the middle dot read
/// as a dash. Only the scan folds: text that stays is kept as written.
fn fold(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
        '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{00B7}' => '-',
        _ => c,
    }
}

/// Replaces each whitespace-delimited word that `hit` accepts with `mask`. `hit` sees the word folded, so
/// "kim＠mail．com" reads as "kim@mail.com". Whitespace, line breaks included, is kept as written.
pub fn mask_words(text: &str, mask: &str, hit: impl Fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let ws = rest.find(|c: char| !c.is_whitespace()).unwrap_or(rest.len());
        out.push_str(&rest[..ws]);
        rest = &rest[ws..];
        let we = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..we];
        let folded: String = word.chars().map(fold).collect();
        out.push_str(if !word.is_empty() && hit(&folded) { mask } else { word });
        rest = &rest[we..];
    }
    out
}

fn spaces_at(f: &[char], at: usize) -> usize {
    f.iter().skip(at).take(GAP_SPACES).take_while(|&&c| c == ' ').count()
}

/// Length of the separator at `at` that joins two digit groups: up to two spaces, or a dash, dot or slash with
/// up to two spaces on each side, after an optional closing paren and before an optional opening paren
/// ("(010) 1234", "010.1234", "010 - 1234"). None when no digit follows.
fn gap_len(f: &[char], at: usize) -> Option<usize> {
    let mut k = at;
    if f.get(k) == Some(&')') {
        k += 1;
    }
    k += spaces_at(f, k);
    if matches!(f.get(k), Some('-' | '.' | '/')) {
        k += 1;
        k += spaces_at(f, k);
    }
    if f.get(k) == Some(&'(') {
        k += 1;
    }
    (k > at && f.get(k).is_some_and(char::is_ascii_digit)).then_some(k - at)
}

/// The digit groups of the run that starts at `i`, joined by `gap_len` separators.
fn run_groups(f: &[char], i: usize) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut start = i;
    loop {
        let end = start + f[start..].iter().take_while(|c| c.is_ascii_digit()).count();
        groups.push((start, end));
        match gap_len(f, end) {
            Some(len) => start = end + len,
            None => return groups,
        }
    }
}

fn value(f: &[char], (s, e): Group) -> u32 {
    f[s..e].iter().fold(0, |n, c| n.saturating_mul(10).saturating_add(c.to_digit(10).unwrap_or(0)))
}

/// A year-first date: "2026-11-17", "2026.11.17", "2026/11/17", "2026. 11. 17". A 19xx or 20xx year, a real
/// month and day, the same separator twice. Phone numbers start with 0, so none is read as a date.
fn is_date(f: &[char], g: &[Group]) -> bool {
    let [y, m, d] = g else { return false };
    let sep = |a: &Group, b: &Group| -> String { f[a.1..b.0].iter().filter(|&&c| c != ' ').collect() };
    let short = |&(s, e): &Group| (1..=2).contains(&(e - s));
    let one = sep(y, m);
    matches!(one.as_str(), "-" | "." | "/")
        && one == sep(m, d)
        && y.1 - y.0 == 4
        && short(m)
        && short(d)
        && (1900..=2099).contains(&value(f, *y))
        && (1..=12).contains(&value(f, *m))
        && (1..=31).contains(&value(f, *d))
}

/// The booking number format `BK` + YYMM + `-` + 5-digit sequence ("BK2611-00012").
fn is_booking_number(f: &[char], g: &[Group]) -> bool {
    let [a, b] = g else { return false };
    a.0 >= 2 && f[a.0 - 2..a.0] == ['B', 'K'] && a.1 - a.0 == 4 && f[a.1..b.0] == ['-'] && b.1 - b.0 == 5
}

/// One or two digits right before or after a colon with a digit on its other side: the "09" of "09:34".
fn is_time(f: &[char], (s, e): Group) -> bool {
    let colon_digit = |c: usize, d: usize| f.get(c) == Some(&':') && f.get(d).is_some_and(char::is_ascii_digit);
    e - s <= 2 && (colon_digit(e, e + 1) || (s >= 2 && colon_digit(s - 1, s - 2)))
}

/// Splits a run's groups into pieces masked or kept on their own, as (group range, always kept). A date or
/// booking number starting a piece is always kept, and a time group never joins its neighbours, so
/// "2026-11-17 09:34:51" stays readable while "2026-11-17 010 1234 5678" still masks the phone.
fn pieces(f: &[char], groups: &[Group]) -> Vec<(Range<usize>, bool)> {
    let mut out = Vec::new();
    let (mut s, mut k) = (0, 0);
    while k < groups.len() {
        let fixed = if k == s && groups.len() >= k + 3 && is_date(f, &groups[k..k + 3]) {
            3
        } else if k == 0 && groups.len() >= 2 && is_booking_number(f, &groups[..2]) {
            2
        } else if is_time(f, groups[k]) {
            1
        } else {
            k += 1;
            continue;
        };
        if k > s {
            out.push((s..k, false));
        }
        out.push((k..k + fixed, fixed > 1));
        k += fixed;
        s = k;
    }
    if s < groups.len() {
        out.push((s..groups.len(), false));
    }
    out
}

/// "2026년" and "60000원": a 19xx or 20xx year right before 년, or an amount under 9 digits that does not start
/// with 0 right before 원. Phone numbers have 9+ digits and start with 0, so none passes.
fn is_year_or_price(f: &[char], s: usize, e: usize) -> bool {
    match f.get(e) {
        Some('년') => e - s == 4 && matches!(f[s..s + 2], ['1', '9'] | ['2', '0']),
        Some('원') => e - s < 9 && f[s] != '0',
        _ => false,
    }
}

/// True when the whole whitespace-delimited token around `s..e` is hex digits with a letter in it (a stack
/// trace address such as `00000000001b2c3d`). "tel:a01012345678" is not: the token holds "tel:".
fn in_hex_token(f: &[char], s: usize, e: usize) -> bool {
    let ws = f[..s].iter().rposition(|c| c.is_whitespace()).map_or(0, |i| i + 1);
    let we = f[e..].iter().position(|c| c.is_whitespace()).map_or(f.len(), |i| e + i);
    let token = &f[ws..we];
    token.iter().all(char::is_ascii_hexdigit) && token.iter().any(char::is_ascii_alphabetic)
}

/// Replaces each run of `min_digits`+ digits with `mask`. Digit groups joined by a separator (see `gap_len`)
/// are one run: "010 1234 5678", "(010) 1234-5678", "010/1234/5678". Kept whatever their length: dates,
/// times, booking numbers, a year before 년 or a price before 원 (see `pieces`, `is_year_or_price`), and with
/// `keep_hex_words` an unseparated run in a hex token. Everything outside a masked run is kept as written.
pub fn mask_digit_runs(text: &str, min_digits: usize, mask: &str, keep_hex_words: bool) -> String {
    let orig: Vec<char> = text.chars().collect();
    let f: Vec<char> = orig.iter().map(|&c| fold(c)).collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < f.len() {
        if !f[i].is_ascii_digit() {
            out.push(orig[i]);
            i += 1;
            continue;
        }
        let groups = run_groups(&f, i);
        for (r, always) in pieces(&f, &groups) {
            let (s, e) = (groups[r.start].0, groups[r.end - 1].1);
            let digits: usize = groups[r.clone()].iter().map(|(gs, ge)| ge - gs).sum();
            let single = r.len() == 1;
            let keep = always || digits < min_digits || (single && (is_year_or_price(&f, s, e) || (keep_hex_words && in_hex_token(&f, s, e))));
            out.extend(&orig[i..s]);
            if keep {
                out.extend(&orig[s..e]);
            } else {
                out.push_str(mask);
            }
            i = e;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_groups_are_one_run_and_the_rest_is_kept() {
        assert_eq!(mask_digit_runs("연락처: 010 1234 5678.", 9, "#", false), "연락처: #.");
        assert_eq!(mask_digit_runs("(010) 1234-5678", 9, "#", false), "(#");
        assert_eq!(mask_digit_runs("０１０-１２３４-５６７８！", 9, "#", false), "#！");
        assert_eq!(mask_digit_runs("010  1234 5678", 9, "#", false), "#");
        assert_eq!(mask_digit_runs("step 2 of 3", 2, "#", false), "step 2 of 3");
    }

    #[test]
    fn hex_words_are_kept_only_when_asked_and_unseparated() {
        assert_eq!(mask_digit_runs("virt 00000000001b2c3d", 9, "#", true), "virt 00000000001b2c3d");
        assert_eq!(mask_digit_runs("virt 00000000001b2c3d", 9, "#", false), "virt #b2c3d");
        assert_eq!(mask_digit_runs("a010-1234-5678", 9, "#", true), "a#");
    }

    #[test]
    fn words_keep_every_kind_of_whitespace() {
        let hit = |w: &str| w.contains('@');
        assert_eq!(mask_words(" a@b \n\tc\u{a0}d@e ", "#", hit), " # \n\tc\u{a0}# ");
        assert_eq!(mask_words("", "#", hit), "");
    }
}
