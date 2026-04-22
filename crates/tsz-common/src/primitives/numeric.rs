//! Utilities for parsing numeric literals.

/// Parse a numeric literal text representation into a f64 value.
/// Supports standard floating point literals as well as 0x, 0b, and 0o prefixes.
/// Also handles numeric separators (`_`).
pub fn parse_numeric_literal_value(text: &str) -> Option<f64> {
    if text.is_empty() {
        return None;
    }

    if text.len() > 2 {
        let prefix = &text[0..2];
        if prefix.eq_ignore_ascii_case("0x") {
            return parse_radix_digits_as_f64(&text[2..], 16);
        } else if prefix.eq_ignore_ascii_case("0b") {
            return parse_radix_digits_as_f64(&text[2..], 2);
        } else if prefix.eq_ignore_ascii_case("0o") {
            return parse_radix_digits_as_f64(&text[2..], 8);
        }
    }

    if text.contains('_') {
        let mut cleaned = String::with_capacity(text.len());
        for c in text.chars() {
            if c != '_' {
                cleaned.push(c);
            }
        }
        return cleaned.parse::<f64>().ok();
    }

    text.parse::<f64>().ok()
}

/// Parse a digit sequence in the given base (2/8/10/16) as `f64`.
///
/// Hex digits are case-insensitive. Underscores (numeric separators) are
/// skipped. Returns `None` for empty input, separator-only input, or any
/// digit invalid for the chosen base. Accumulates directly as `f64`, so
/// inputs larger than `u128::MAX` still produce the closest representable
/// float — no two-path overflow fallback needed at the call site.
pub fn parse_radix_digits_as_f64(text: &str, base: u32) -> Option<f64> {
    if text.is_empty() {
        // "0x" alone is invalid, but if caller stripped prefix and got empty, it might mean "0x"
        // which parser should have handled as error or incomplete.
        // But for value parsing, empty means no digits.
        return None;
    }

    let mut value = 0.0;
    let base_float = base as f64;
    let mut saw_digit = false;

    for byte in text.bytes() {
        if byte == b'_' {
            continue;
        }

        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u32,
            b'a'..=b'f' => (byte - b'a' + 10) as u32,
            b'A'..=b'F' => (byte - b'A' + 10) as u32,
            _ => return None, // Invalid digit for any supported base
        };

        if digit >= base {
            return None; // Digit too large for base
        }

        saw_digit = true;
        value = value * base_float + (digit as f64);
    }

    if !saw_digit {
        // Stripped body contained only separators (e.g. "0x_") — no digits, invalid.
        return None;
    }

    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numeric_literal_value() {
        assert_eq!(parse_numeric_literal_value("123"), Some(123.0));
        assert_eq!(parse_numeric_literal_value("123.456"), Some(123.456));
        assert_eq!(parse_numeric_literal_value("1_000"), Some(1000.0));
        assert_eq!(parse_numeric_literal_value("0b11"), Some(3.0));
        assert_eq!(parse_numeric_literal_value("0B111"), Some(7.0));
        assert_eq!(parse_numeric_literal_value("0o10"), Some(8.0));
        assert_eq!(parse_numeric_literal_value("0O123"), Some(83.0));
        assert_eq!(parse_numeric_literal_value("0xFF"), Some(255.0));
        assert_eq!(parse_numeric_literal_value("0Xabc"), Some(2748.0));
        assert_eq!(parse_numeric_literal_value("0b1_0"), Some(2.0));

        // Invalid
        assert_eq!(parse_numeric_literal_value("0b2"), None);
        assert_eq!(parse_numeric_literal_value("0o8"), None);
        assert_eq!(parse_numeric_literal_value("0xg"), None);
    }

    #[test]
    fn test_parse_numeric_literal_value_rejects_missing_digits_and_empty_input() {
        assert_eq!(parse_numeric_literal_value(""), None);
        assert_eq!(parse_numeric_literal_value("0x"), None);
        assert_eq!(parse_numeric_literal_value("0b"), None);
        assert_eq!(parse_numeric_literal_value("0o"), None);
    }

    #[test]
    fn test_parse_numeric_literal_value_rejects_separator_only_radix_body() {
        // A radix body consisting only of separators has zero digits, which is
        // invalid per spec. Regression for the previous behavior where
        // `0x_` / `0b_` / `0o_` silently returned `Some(0.0)`.
        assert_eq!(parse_numeric_literal_value("0x_"), None);
        assert_eq!(parse_numeric_literal_value("0X__"), None);
        assert_eq!(parse_numeric_literal_value("0b_"), None);
        assert_eq!(parse_numeric_literal_value("0B_"), None);
        assert_eq!(parse_numeric_literal_value("0o_"), None);
        assert_eq!(parse_numeric_literal_value("0O___"), None);
    }

    #[test]
    fn test_parse_numeric_literal_value_handles_signs_and_separators() {
        assert_eq!(parse_numeric_literal_value("+42"), Some(42.0));
        assert_eq!(parse_numeric_literal_value("-3.5"), Some(-3.5));
        assert_eq!(parse_numeric_literal_value("1_2_3_4"), Some(1234.0));
        assert_eq!(parse_numeric_literal_value("0xDE_AD"), Some(57005.0));
        assert_eq!(parse_numeric_literal_value("0b1010_1111"), Some(175.0));
        assert_eq!(parse_numeric_literal_value("0o7_7"), Some(63.0));
    }

    #[test]
    fn test_parse_numeric_literal_value_mixes_rejections_and_separator_normalization() {
        assert_eq!(parse_numeric_literal_value("1e"), None);
        assert_eq!(parse_numeric_literal_value("0x1p2"), None);
        assert_eq!(parse_numeric_literal_value("abc"), None);
        assert_eq!(parse_numeric_literal_value("1__2"), Some(12.0));
    }
}
