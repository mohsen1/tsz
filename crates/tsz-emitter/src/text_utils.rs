//! Canonical JavaScript text-formatting routines for emit.
//!
//! These helpers are the single correct implementation for escaping strings,
//! formatting numbers, and testing identifier emittability. All emit-side code
//! must route through these rather than inlining divergent copies.

/// Format an f64 value the way JavaScript's `Number.toString()` would.
///
/// Handles `NaN`, `+/-Infinity`, and the scientific-notation threshold
/// (magnitudes >= 1e21 or < 1e-6). Uses Rust's shortest-roundtrip formatter
/// which matches JS `Number.prototype.toString()` for the normal range.
pub(crate) fn format_js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let abs = value.abs();
    if abs != 0.0 && abs < 1e-6 {
        return format_js_scientific(value);
    }
    let s = value.to_string();
    let abs_s = s.strip_prefix('-').unwrap_or(&s);
    let needs_scientific = if let Some(dot_pos) = abs_s.find('.') {
        dot_pos >= 21
    } else {
        abs_s.len() >= 21
    };
    if needs_scientific {
        format_js_scientific(value)
    } else {
        s
    }
}

/// Format a number in JavaScript-style scientific notation (e.g., `1.2345678912345678e+53`).
fn format_js_scientific(n: f64) -> String {
    let neg = n < 0.0;
    let abs_n = n.abs();
    let s = format!("{abs_n:e}");
    let result = if let Some(pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(pos);
        let exp_str = &exp_part[1..];
        if exp_str.starts_with('-') {
            format!("{mantissa}e{exp_str}")
        } else {
            format!("{mantissa}e+{exp_str}")
        }
    } else {
        s
    };
    if neg { format!("-{result}") } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infinity() {
        assert_eq!(format_js_number(f64::INFINITY), "Infinity");
        assert_eq!(format_js_number(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn nan() {
        assert_eq!(format_js_number(f64::NAN), "NaN");
    }

    #[test]
    fn integers() {
        assert_eq!(format_js_number(0.0), "0");
        assert_eq!(format_js_number(42.0), "42");
        assert_eq!(format_js_number(-1.0), "-1");
        assert_eq!(format_js_number(1_000_000.0), "1000000");
    }

    #[test]
    fn floats() {
        assert_eq!(format_js_number(3.15), "3.15");
        assert_eq!(format_js_number(-0.5), "-0.5");
    }

    #[test]
    fn scientific_large() {
        assert_eq!(format_js_number(1e21), "1e+21");
        assert_eq!(
            format_js_number(1.2345678912345678e53),
            "1.2345678912345678e+53"
        );
    }

    #[test]
    fn scientific_small() {
        assert_eq!(format_js_number(1e-7), "1e-7");
    }

    #[test]
    fn negative_scientific() {
        assert_eq!(format_js_number(-1e21), "-1e+21");
    }
}
