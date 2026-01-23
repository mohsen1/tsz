//! Shared utility functions for the solver module.
//!
//! This module contains common utilities used across multiple solver components
//! to avoid code duplication.

use crate::interner::Atom;
use crate::solver::db::TypeDatabase;

/// Checks if a property name is numeric by resolving the atom and checking its string representation.
///
/// This function consolidates the previously duplicated `is_numeric_property_name` implementations
/// from operations.rs, evaluate.rs, subtype.rs, and infer.rs.
pub fn is_numeric_property_name(interner: &dyn TypeDatabase, name: Atom) -> bool {
    let prop_name = interner.resolve_atom_ref(name);
    is_numeric_literal_name(prop_name.as_ref())
}

/// Checks if a string represents a numeric literal name.
///
/// Returns `true` for:
/// - "NaN", "Infinity", "-Infinity"
/// - Numeric strings that round-trip correctly through JavaScript's number-to-string conversion
pub fn is_numeric_literal_name(name: &str) -> bool {
    if name == "NaN" || name == "Infinity" || name == "-Infinity" {
        return true;
    }

    let value: f64 = match name.parse() {
        Ok(value) => value,
        Err(_) => return false,
    };
    if !value.is_finite() {
        return false;
    }

    js_number_to_string(value) == name
}

/// Converts a JavaScript number to its string representation.
///
/// This matches JavaScript's `Number.prototype.toString()` behavior for proper
/// numeric literal name checking.
fn js_number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }

    let abs = value.abs();
    if !(1e-6..1e21).contains(&abs) {
        let mut formatted = format!("{:e}", value);
        if let Some(split) = formatted.find('e') {
            let (mantissa, exp) = formatted.split_at(split);
            let exp_digits = &exp[1..];
            let (sign, digits) = if exp_digits.starts_with('-') {
                ('-', &exp_digits[1..])
            } else {
                ('+', exp_digits)
            };
            let trimmed = digits.trim_start_matches('0');
            let digits = if trimmed.is_empty() { "0" } else { trimmed };
            formatted = format!("{mantissa}e{sign}{digits}");
        }
        return formatted;
    }

    let formatted = value.to_string();
    if formatted == "-0" {
        "0".to_string()
    } else {
        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_numeric_literal_name() {
        // Special values
        assert!(is_numeric_literal_name("NaN"));
        assert!(is_numeric_literal_name("Infinity"));
        assert!(is_numeric_literal_name("-Infinity"));

        // Regular numbers
        assert!(is_numeric_literal_name("0"));
        assert!(is_numeric_literal_name("1"));
        assert!(is_numeric_literal_name("42"));
        assert!(is_numeric_literal_name("-1"));
        assert!(is_numeric_literal_name("3.14"));

        // Non-numeric strings
        assert!(!is_numeric_literal_name("foo"));
        assert!(!is_numeric_literal_name(""));
        assert!(!is_numeric_literal_name("abc123"));
    }
}
