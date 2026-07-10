//! Canonical JavaScript text-formatting routines for emit.
//!
//! These helpers are the single correct implementation for escaping strings,
//! formatting numbers, and testing identifier emittability. All emit-side code
//! must route through these rather than inlining divergent copies.

/// Format an f64 value the way JavaScript's `Number.toString()` would.
///
/// Delegates to the workspace-wide ECMAScript `Number::toString(10)` owner in
/// `tsz_common`; see [`tsz_common::numeric::js_number_to_string`]. (A previous
/// local implementation switched to scientific notation one decimal digit too
/// early, emitting `1e+20` where JavaScript prints
/// `100000000000000000000`.)
pub(crate) fn format_js_number(value: f64) -> String {
    tsz_common::numeric::js_number_to_string(value).into_owned()
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
    fn positional_up_to_21_digits() {
        // JavaScript stays positional below 1e21; the old local formatter
        // switched to scientific one digit early.
        assert_eq!(format_js_number(1e20), "100000000000000000000");
        assert_eq!(format_js_number(-1e20), "-100000000000000000000");
    }

    #[test]
    fn negative_zero_prints_as_zero() {
        assert_eq!(format_js_number(-0.0), "0");
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
