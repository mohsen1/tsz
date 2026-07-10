//! Canonical JavaScript text-formatting routines for emit.
//!
//! These helpers are the single correct implementation for escaping strings,
//! formatting numbers, and testing identifier emittability. All emit-side code
//! must route through these rather than inlining divergent copies.

/// Format an f64 value the way JavaScript's `Number.toString()` would.
///
/// Delegates to the cross-crate `js_number_to_string` owner shared with the
/// solver and checker, so emit and semantic decisions can never disagree on a
/// number's JS text: `-0` → `"0"`, and scientific notation exactly at
/// magnitudes at or above 1e21 or below 1e-6 (the previous local copy
/// switched a digit early, at 21 integer digits, mis-formatting values in
/// `[1e20, 1e21)`).
pub(crate) fn format_js_number(value: f64) -> String {
    tsz_common::numeric::js_number_to_string(value).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behavior tests live beside the owner in `tsz_common::numeric`; this
    /// smoke test only pins that the emitter delegate wires through.
    #[test]
    fn delegates_to_shared_owner() {
        assert_eq!(format_js_number(1e21), "1e+21");
        assert_eq!(format_js_number(-0.0), "0");
    }
}
