//! Money amount detection in OCR text of receipts and bank transfer screenshots (VND conventions).

use regex::Regex;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::OnceLock;

use crate::services::statement::ocr::OcrPage;

/// Amounts at or above this are treated as account numbers, phone numbers, etc.
const MAX_AMOUNT: i64 = 100_000_000_000;

/// Grouped number (`1.250.000`, `1 250 000`, `1,250,000.00`) or a plain run of 4+ digits.
fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\d{1,3}(?:[.,\s]\d{3})+(?:[.,]\d{1,2})?|\d+").expect("valid amount regex")
    })
}

/// Parse a matched candidate into its value. Returns None when it is not a grouped number.
fn parse_grouped(raw: &str) -> Option<Decimal> {
    let seps: Vec<(usize, char)> = raw
        .char_indices()
        .filter(|(_, c)| matches!(c, '.' | ',') || c.is_whitespace())
        .collect();
    if seps.is_empty() {
        return Decimal::from_str(raw).ok();
    }
    let (last_idx, last_ch) = *seps.last()?;
    let tail = &raw[last_idx + last_ch.len_utf8()..];
    // The last group is a decimal part when it has 1-2 digits (a 3-digit group is a thousands group).
    let has_decimal = tail.len() <= 2;
    let (int_part, frac) = if has_decimal {
        (&raw[..last_idx], Some(tail))
    } else {
        (raw, None)
    };
    let mut digits: String = int_part.chars().filter(|c| c.is_ascii_digit()).collect();
    if let Some(f) = frac {
        digits.push('.');
        digits.push_str(f);
    }
    Decimal::from_str(&digits).ok()
}

fn is_marker_char(c: char) -> bool {
    matches!(c, 'đ' | 'Đ' | '₫')
}

/// True when a currency marker sits right after (`50000đ`, `50000 VND`) or right before the digits.
fn has_currency_marker(before: &str, after: &str) -> bool {
    let after_trim = after.trim_start_matches(' ');
    let lower = after_trim.to_lowercase();
    if lower.starts_with("vnd") || lower.starts_with("vnđ") {
        return true;
    }
    if let Some(c) = after_trim.chars().next() {
        if is_marker_char(c) {
            return true;
        }
        // Standalone "d" suffix: not followed by another letter
        if (c == 'd' || c == 'D') && !after_trim.chars().nth(1).is_some_and(|n| n.is_alphabetic()) {
            return true;
        }
    }
    let before_trim = before.trim_end_matches(' ');
    let lower_b = before_trim.to_lowercase();
    lower_b.ends_with("vnd") || lower_b.ends_with("vnđ") || before_trim.chars().next_back().is_some_and(is_marker_char)
}

/// Money amounts found in one line of OCR text.
pub fn amounts_in(text: &str) -> Vec<Decimal> {
    let mut out = Vec::new();
    for m in number_re().find_iter(text) {
        let (start, end) = (m.start(), m.end());
        let before = &text[..start];
        let after = &text[end..];
        let prev = before.chars().next_back();
        let next = after.chars().next();

        // Part of a code (FT26306123, MB123) or a date/time/percentage
        if prev.is_some_and(|c| c.is_alphabetic() || c == '/' || c == ':') {
            continue;
        }
        if next.is_some_and(|c| c == '/' || c == ':' || c == '%') {
            continue;
        }
        let after_trim = after.trim_start_matches(' ');
        if after_trim.starts_with('%') {
            continue;
        }
        // A letter directly after the number is a code, unless it is a currency marker
        let letter_next = next.is_some_and(|c| c.is_alphabetic());
        let raw = m.as_str();
        let grouped = raw.chars().any(|c| matches!(c, '.' | ',') || c.is_whitespace());
        let marker = has_currency_marker(before, after);
        if letter_next && !marker {
            continue;
        }
        // Digits glued to a separator on the left/right belong to a longer, malformed number
        if prev.is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        if !grouped {
            if raw.len() < 4 || !marker {
                continue;
            }
        } else if letter_next && !marker {
            continue;
        }
        let Some(value) = parse_grouped(raw) else { continue };
        if value.is_zero() || value >= Decimal::from(MAX_AMOUNT) {
            continue;
        }
        out.push(value.normalize());
    }
    out
}

/// Bare whole numbers (`10000`) that `amounts_in` skips for lack of a currency marker: 4-9 digits,
/// no leading 0 (phone numbers), not glued to letters, other digits or date/time/percent signs.
fn plain_numbers_in(text: &str) -> Vec<Decimal> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\d+").expect("valid plain number regex"));
    let glued = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || matches!(c, '/' | ':' | '%' | '.' | ',' | '-'));
    re.find_iter(text)
        .filter(|m| (4..=9).contains(&m.len()) && !m.as_str().starts_with('0'))
        .filter(|m| !glued(text[..m.start()].chars().next_back()) && !glued(text[m.end()..].chars().next()))
        .filter_map(|m| Decimal::from_str(m.as_str()).ok())
        .collect()
}

/// Biggest money amount across all lines of a recognized photo; when none has a currency marker or
/// grouping, the biggest bare number instead.
pub fn biggest_amount(page: &OcrPage) -> Option<Decimal> {
    page.lines.iter().flat_map(|l| amounts_in(&l.text)).max()
        .or_else(|| page.lines.iter().flat_map(|l| plain_numbers_in(&l.text)).max())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::statement::ocr::OcrLine;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn grouped_numbers() {
        assert_eq!(amounts_in("Tổng cộng: 1.250.000 đ"), vec![d("1250000")]);
        assert_eq!(amounts_in("TOTAL 1,250,000"), vec![d("1250000")]);
        assert_eq!(amounts_in("1 250 000 VND"), vec![d("1250000")]);
        assert_eq!(amounts_in("Số tiền: -500.000 VND"), vec![d("500000")]);
        assert_eq!(amounts_in("1,250,000.00"), vec![d("1250000")]);
        assert_eq!(amounts_in("12.500,50"), vec![d("12500.50")]);
        assert_eq!(amounts_in("45.000"), vec![d("45000")]);
    }

    #[test]
    fn ungrouped_needs_marker() {
        assert_eq!(amounts_in("50000đ"), vec![d("50000")]);
        assert_eq!(amounts_in("50000d"), vec![d("50000")]);
        assert_eq!(amounts_in("50000"), Vec::<Decimal>::new());
    }

    #[test]
    fn rejects_codes_dates_percent() {
        assert!(amounts_in("FT26306123456").is_empty());
        assert!(amounts_in("02/11/2026 18:03").is_empty());
        assert!(amounts_in("Tài khoản 19036812345678").is_empty());
        assert!(amounts_in("VAT 10%").is_empty());
        assert!(amounts_in("MB123").is_empty());
    }

    #[test]
    fn biggest_over_lines() {
        let line = |t: &str| OcrLine {
            text: t.to_string(),
            confidence: 1.0,
            top_left: (0.0, 0.0),
            top_right: (0.0, 0.0),
            bottom_left: (0.0, 0.0),
            bottom_right: (0.0, 0.0),
        };
        let page = OcrPage {
            width: 0,
            height: 0,
            lines: vec![
                line("Tiền hàng 1.100.000"),
                line("VAT 110.000"),
                line("Tổng 1.210.000"),
                line("SĐT 0912345678"),
            ],
        };
        assert_eq!(biggest_amount(&page), Some(d("1210000")));

        // No marked/grouped amount: biggest bare number, skipping phone numbers, dates and codes
        let bare = |lines: &[&str]| OcrPage { width: 0, height: 0, lines: lines.iter().map(|t| line(t)).collect() };
        assert_eq!(biggest_amount(&bare(&["10000"])), Some(d("10000")));
        assert_eq!(biggest_amount(&bare(&["Ban 12", "5000", "25000", "SĐT 0912345678"])), Some(d("25000")));
        assert_eq!(biggest_amount(&bare(&["2026-11-02", "02/11/2026", "FT26306123", "1234567890"])), None);
        // A marked amount still wins over a bigger bare number
        assert_eq!(biggest_amount(&bare(&["99999", "50.000đ"])), Some(d("50000")));
    }
}
