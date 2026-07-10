//! Utilities for parsing and stringifying numeric literals.

use std::borrow::Cow;

/// Converts a JavaScript number to its string representation, matching the
/// ECMAScript `Number::toString(10)` abstract operation.
///
/// This is the single owner for JS number→string conversion. Every semantic
/// or emit decision that turns a numeric value into JS text (template literal
/// type evaluation, property-key canonicalization, indexed-access key
/// derivation, `infer`-pattern round-trip checks, JS/DTS numeric emit) must
/// route through it: raw Rust `Display` diverges from JS exactly where the
/// spec switches notation (`1e21` → JS `"1e+21"` vs Rust
/// `"1000000000000000000000"`, `1e-7` → JS `"1e-7"` vs Rust `"0.0000001"`,
/// `-0` → JS `"0"` vs Rust `"-0"`).
///
/// Returns `Cow::Borrowed` for static special cases (`NaN`, `0`, infinities)
/// and `Cow::Owned` for dynamically formatted numbers.
pub fn js_number_to_string(value: f64) -> Cow<'static, str> {
    if value.is_nan() {
        return Cow::Borrowed("NaN");
    }
    if value == 0.0 {
        // Covers -0.0 as well: IEEE 754 comparison treats -0 == 0, and JS
        // Number::toString(-0) is "0".
        return Cow::Borrowed("0");
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            Cow::Borrowed("-Infinity")
        } else {
            Cow::Borrowed("Infinity")
        };
    }

    // ECMAScript Number::toString uses scientific notation when the decimal
    // exponent is >= 21 or <= -7, i.e. |value| >= 1e21 or |value| < 1e-6.
    let abs = value.abs();
    if !(1e-6..1e21).contains(&abs) {
        let mut formatted = format!("{value:e}");
        if let Some(split) = formatted.find('e') {
            let (mantissa, exp) = formatted.split_at(split);
            let exp_digits = exp.strip_prefix('e').unwrap_or("");
            let (sign, digits) = if let Some(digits) = exp_digits.strip_prefix('-') {
                ('-', digits)
            } else {
                ('+', exp_digits)
            };
            let trimmed = digits.trim_start_matches('0');
            let digits = if trimmed.is_empty() { "0" } else { trimmed };
            formatted = format!("{mantissa}e{sign}{digits}");
        }
        return Cow::Owned(formatted);
    }

    Cow::Owned(value.to_string())
}

/// tsc's `isValidNumberString(text, roundTripOnly = true)`: parse `text` as a
/// JS number and return the value only when [`js_number_to_string`]
/// reproduces the text exactly.
///
/// This is the gate template-literal `infer` captures and inference-capture
/// coercions use to decide whether a captured segment keeps a numeric literal
/// type: `"42"` round-trips (→ `42`), while `"042"`, `"1.0"`, `"-0"`,
/// `"0x2A"`, and `"Infinity"` do not.
pub fn round_trip_js_number(text: &str) -> Option<f64> {
    let value = parse_numeric_literal_value(text)?;
    (value.is_finite() && js_number_to_string(value) == text).then_some(value)
}

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

/// Number of distinct 32-bit integer values (`2^32`), the modulus used by the
/// ECMAScript `ToInt32` / `ToUint32` abstract operations.
const TWO_POW_32: f64 = 4_294_967_296.0;

/// ECMAScript `ToUint32` (<https://tc39.es/ecma262/#sec-touint32>).
///
/// Truncates `value` toward zero and reduces it modulo `2^32` into the
/// unsigned 32-bit range `[0, 2^32)`. `NaN`, `±0`, and `±∞` map to `0`.
///
/// This is the conversion JavaScript/TypeScript applies to the operands of the
/// bitwise (`&`, `|`, `^`, `~`) and shift (`<<`, `>>`, `>>>`) operators, so any
/// constant-expression evaluator that mirrors those operators on `f64` values
/// must route through it. A plain `value as u32` cast is **not** equivalent:
/// Rust's float-to-int cast *saturates* (`3e9_f64 as u32 == u32::MAX`,
/// `-1.0_f64 as u32 == 0`), whereas ECMAScript *wraps* (`ToUint32(3e9) ==
/// 3000000000`, `ToUint32(-1) == 4294967295`).
#[inline]
#[must_use]
pub fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    // `trunc()` is the spec's truncate-toward-zero; `rem_euclid` yields a
    // non-negative remainder in `[0, 2^32)`, so the subsequent cast never
    // saturates. For integer-valued inputs within `2^53` the arithmetic is
    // exact, matching the double-precision result JS engines produce.
    let modulo = value.trunc().rem_euclid(TWO_POW_32);
    modulo as u32
}

/// ECMAScript `ToInt32` (<https://tc39.es/ecma262/#sec-toint32>).
///
/// Like [`to_uint32`] but reinterprets the wrapped value into the signed 32-bit
/// range `[-2^31, 2^31)`. `ToInt32(2^31) == -2^31`, `ToInt32(-1) == -1`.
///
/// Use this — never `value as i32` — when folding the operands of JavaScript
/// bitwise/shift operators: the saturating `as i32` cast turns
/// `0x80000000 | 0` into `i32::MAX` instead of the correct `-2147483648`.
#[inline]
#[must_use]
pub fn to_int32(value: f64) -> i32 {
    // `u32 as i32` is a bit-pattern reinterpretation (wrapping), which is
    // exactly the signed view of the `ToUint32` result.
    to_uint32(value) as i32
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
        assert_eq!(parse_numeric_literal_value("1e3"), Some(1000.0));
        assert_eq!(parse_numeric_literal_value("1E-3"), Some(0.001));
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
    fn to_uint32_wraps_modulo_two_pow_32() {
        // Small values are unchanged.
        assert_eq!(to_uint32(0.0), 0);
        assert_eq!(to_uint32(255.0), 255);
        // `2^31` stays in unsigned range; `2^32` wraps to 0.
        assert_eq!(to_uint32(2_147_483_648.0), 2_147_483_648);
        assert_eq!(to_uint32(4_294_967_296.0), 0);
        // Negative and out-of-range values wrap rather than saturate
        // (a plain `as u32` cast would yield 0 and u32::MAX respectively).
        assert_eq!(to_uint32(-1.0), 4_294_967_295);
        assert_eq!(to_uint32(3_000_000_000.0), 3_000_000_000);
        assert_eq!(to_uint32(4_294_967_297.0), 1);
        // Truncation is toward zero before the modulo.
        assert_eq!(to_uint32(5.9), 5);
        assert_eq!(to_uint32(-5.9), 4_294_967_291);
    }

    #[test]
    fn to_uint32_maps_non_finite_to_zero() {
        assert_eq!(to_uint32(f64::NAN), 0);
        assert_eq!(to_uint32(f64::INFINITY), 0);
        assert_eq!(to_uint32(f64::NEG_INFINITY), 0);
        assert_eq!(to_uint32(-0.0), 0);
    }

    #[test]
    fn to_int32_wraps_into_signed_range() {
        assert_eq!(to_int32(0.0), 0);
        assert_eq!(to_int32(255.0), 255);
        // `0x80000000` is the canonical witness: saturating `as i32` would give
        // i32::MAX (2147483647); ECMAScript ToInt32 wraps to -2147483648.
        assert_eq!(to_int32(2_147_483_648.0), -2_147_483_648);
        assert_ne!(to_int32(2_147_483_648.0), i32::MAX);
        assert_eq!(to_int32(4_294_967_295.0), -1);
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_int32(3_000_000_000.0), -1_294_967_296);
        assert_eq!(to_int32(4_294_967_296.0), 0);
        assert_eq!(to_int32(f64::NAN), 0);
    }

    #[test]
    fn test_parse_numeric_literal_value_mixes_rejections_and_separator_normalization() {
        assert_eq!(parse_numeric_literal_value("1e"), None);
        assert_eq!(parse_numeric_literal_value("0x1p2"), None);
        assert_eq!(parse_numeric_literal_value("abc"), None);
        assert_eq!(parse_numeric_literal_value("1__2"), Some(12.0));
    }

    #[test]
    fn js_number_to_string_specials() {
        assert_eq!(js_number_to_string(f64::NAN), "NaN");
        assert_eq!(js_number_to_string(f64::INFINITY), "Infinity");
        assert_eq!(js_number_to_string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(js_number_to_string(0.0), "0");
        assert_eq!(js_number_to_string(-0.0), "0");
    }

    #[test]
    fn js_number_to_string_fixed_point_range() {
        assert_eq!(js_number_to_string(42.0), "42");
        assert_eq!(js_number_to_string(-1.0), "-1");
        assert_eq!(js_number_to_string(3.15), "3.15");
        assert_eq!(js_number_to_string(-0.5), "-0.5");
        assert_eq!(js_number_to_string(1e-6), "0.000001");
        // 21-digit integers below 1e21 stay fixed-point, as in JS.
        assert_eq!(js_number_to_string(1e20), "100000000000000000000");
        assert_eq!(js_number_to_string(9.99e20), "999000000000000000000");
    }

    #[test]
    fn js_number_to_string_scientific_range() {
        assert_eq!(js_number_to_string(1e21), "1e+21");
        assert_eq!(js_number_to_string(-1e21), "-1e+21");
        assert_eq!(js_number_to_string(1e-7), "1e-7");
        assert_eq!(
            js_number_to_string(1.2345678912345678e53),
            "1.2345678912345678e+53"
        );
    }

    #[test]
    fn round_trip_js_number_gate() {
        assert_eq!(round_trip_js_number("42"), Some(42.0));
        assert_eq!(round_trip_js_number("-1"), Some(-1.0));
        assert_eq!(round_trip_js_number("1e+21"), Some(1e21));
        assert_eq!(round_trip_js_number("042"), None);
        assert_eq!(round_trip_js_number("1.0"), None);
        assert_eq!(round_trip_js_number("-0"), None);
        assert_eq!(round_trip_js_number("0x2A"), None);
        assert_eq!(round_trip_js_number("Infinity"), None);
        assert_eq!(round_trip_js_number(""), None);
    }
}
