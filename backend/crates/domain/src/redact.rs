//! Contact-number masking shared by public review text and stored client error reports.

/// Full-width ASCII (U+FF01..U+FF5E) and the ideographic space read as their ASCII forms, as NFKC folds
/// them, so "０１０-１２３４" is a number too. Only the scan folds: text that stays is kept as written.
fn fold(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{3000}' => ' ',
        _ => c,
    }
}

/// Length of the separator at `at` that joins two digit groups: one of space, dash or dot, with an optional
/// closing paren before it and opening paren after it ("(010) 1234", "010.1234"). None when no digit follows.
fn gap_len(f: &[char], at: usize) -> Option<usize> {
    let mut k = at;
    if f.get(k) == Some(&')') {
        k += 1;
    }
    if matches!(f.get(k), Some(' ' | '-' | '.')) {
        k += 1;
    }
    if f.get(k) == Some(&'(') {
        k += 1;
    }
    (k > at && f.get(k).is_some_and(char::is_ascii_digit)).then_some(k - at)
}

/// True when the unseparated digit run `f[start..end]` sits in a word of hex digits with a letter in it
/// (a stack trace address such as `00000000001b2c3d`).
fn in_hex_word(f: &[char], start: usize, end: usize) -> bool {
    let ws = (0..start).rev().take_while(|&i| f[i].is_ascii_alphanumeric()).last().unwrap_or(start);
    let we = (end..f.len()).take_while(|&i| f[i].is_ascii_alphanumeric()).last().map_or(end, |i| i + 1);
    let word = &f[ws..we];
    word.iter().all(char::is_ascii_hexdigit) && word.iter().any(char::is_ascii_alphabetic)
}

/// Replaces each run of `min_digits`+ digits with `mask`. Digit groups joined by one separator (see
/// `gap_len`) are one run: "010 1234 5678", "(010) 1234-5678", "010 1234.5678". With `keep_hex_words`, an
/// unseparated run inside a hex word is kept. Everything outside a masked run is kept as written.
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
        let (start, mut end, mut digits, mut separated) = (i, i, 0, false);
        loop {
            while end < f.len() && f[end].is_ascii_digit() {
                digits += 1;
                end += 1;
            }
            match gap_len(&f, end) {
                Some(len) => {
                    separated = true;
                    end += len;
                }
                None => break,
            }
        }
        let keep = digits < min_digits || (keep_hex_words && !separated && in_hex_word(&f, start, end));
        if keep {
            out.extend(&orig[start..end]);
        } else {
            out.push_str(mask);
        }
        i = end;
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
        assert_eq!(mask_digit_runs("010  1234 5678", 9, "#", false), "010  1234 5678");
        assert_eq!(mask_digit_runs("step 2 of 3", 2, "#", false), "step 2 of 3");
    }

    #[test]
    fn hex_words_are_kept_only_when_asked_and_unseparated() {
        assert_eq!(mask_digit_runs("virt 00000000001b2c3d", 9, "#", true), "virt 00000000001b2c3d");
        assert_eq!(mask_digit_runs("virt 00000000001b2c3d", 9, "#", false), "virt #b2c3d");
        assert_eq!(mask_digit_runs("a010-1234-5678", 9, "#", true), "a#");
    }
}
