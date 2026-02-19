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
            return parse_radix_digits(&text[2..], 16);
        } else if prefix.eq_ignore_ascii_case("0b") {
            return parse_radix_digits(&text[2..], 2);
        } else if prefix.eq_ignore_ascii_case("0o") {
            return parse_radix_digits(&text[2..], 8);
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

fn parse_radix_digits(text: &str, base: u32) -> Option<f64> {
    if text.is_empty() {
        // "0x" alone is invalid, but if caller stripped prefix and got empty, it might mean "0x"
        // which parser should have handled as error or incomplete.
        // But for value parsing, empty means no digits.
        return None;
    }

    let mut value = 0.0;
    let base_float = base as f64;

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

        value = value * base_float + (digit as f64);
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
}
