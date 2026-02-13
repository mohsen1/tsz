//! Shared utility functions for the solver module.
//!
//! This module contains common utilities used across multiple solver components
//! to avoid code duplication.

use crate::db::TypeDatabase;
use crate::types::TypeId;
use tsz_common::interner::Atom;

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

/// Reduces a vector of types to a union, single type, or NEVER.
///
/// This helper eliminates the common pattern:
/// ```ignore
/// if types.is_empty() {
///     TypeId::NEVER
/// } else if types.len() == 1 {
///     types[0]
/// } else {
///     db.union(types)
/// }
/// ```
///
/// # Examples
///
/// ```ignore
/// let narrowed = union_or_single(db, filtered_members);
/// ```
pub fn union_or_single(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.union(types),
    }
}

/// Reduces a vector of types to an intersection, single type, or NEVER.
///
/// This helper eliminates the common pattern:
/// ```ignore
/// if types.is_empty() {
///     TypeId::NEVER
/// } else if types.len() == 1 {
///     types[0]
/// } else {
///     db.intersection(types)
/// }
/// ```
///
/// # Examples
///
/// ```ignore
/// let narrowed = intersection_or_single(db, instance_types);
/// ```
pub fn intersection_or_single(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.intersection(types),
    }
}

/// Extension trait for TypeId with chainable methods for common operations.
///
/// This trait provides idiomatic Rust methods to reduce boilerplate when
/// working with TypeId values. Methods are designed to be chainable and
/// composable with iterator combinators.
///
/// # Examples
///
/// ```ignore
/// // Filter out NEVER types in a map operation
/// .filter_map(|&id| some_operation(id).non_never())
///
/// // Filter out UNKNOWN types
/// .filter_map(|&id| some_operation(id).non_unknown())
///
/// // Chain predicates
/// if type_id.is_never() || type_id.is_unknown() { ... }
///
/// // Map with NEVER as default for None
/// let result = maybe_type.map_or_never(|t| transform(t));
/// ```
#[allow(dead_code)]
pub trait TypeIdExt {
    /// Returns Some(self) if self is not NEVER, otherwise None.
    ///
    /// This is useful for filter_map chains where you want to skip NEVER results.
    fn non_never(self) -> Option<Self>
    where
        Self: Sized;

    /// Returns Some(self) if self is not UNKNOWN, otherwise None.
    ///
    /// Useful for filtering out unknown types during type inference.
    fn non_unknown(self) -> Option<Self>
    where
        Self: Sized;

    /// Returns Some(self) if self is not ANY, otherwise None.
    ///
    /// Useful for filtering out any types in strict type checking contexts.
    fn non_any(self) -> Option<Self>
    where
        Self: Sized;

    /// Returns true if this is the NEVER type.
    fn is_never(&self) -> bool;

    /// Returns true if this is the UNKNOWN type.
    fn is_unknown(&self) -> bool;

    /// Returns true if this is the ANY type.
    fn is_any(&self) -> bool;

    /// Returns true if this is the VOID type.
    fn is_void(&self) -> bool;

    /// Maps an Option<TypeId> to TypeId, using NEVER as the default.
    ///
    /// This is equivalent to `option.unwrap_or(TypeId::NEVER)` but more expressive.
    fn unwrap_or_never(option: Option<Self>) -> Self
    where
        Self: Sized;
}

impl TypeIdExt for TypeId {
    #[inline]
    fn non_never(self) -> Option<Self> {
        if self != TypeId::NEVER {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn non_unknown(self) -> Option<Self> {
        if self != TypeId::UNKNOWN {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn non_any(self) -> Option<Self> {
        if self != TypeId::ANY {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn is_never(&self) -> bool {
        *self == TypeId::NEVER
    }

    #[inline]
    fn is_unknown(&self) -> bool {
        *self == TypeId::UNKNOWN
    }

    #[inline]
    fn is_any(&self) -> bool {
        *self == TypeId::ANY
    }

    #[inline]
    fn is_void(&self) -> bool {
        *self == TypeId::VOID
    }

    #[inline]
    fn unwrap_or_never(option: Option<Self>) -> Self {
        option.unwrap_or(TypeId::NEVER)
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

    #[test]
    fn test_type_id_ext_non_methods() {
        // Test non_never
        assert_eq!(TypeId::UNKNOWN.non_never(), Some(TypeId::UNKNOWN));
        assert_eq!(TypeId::NEVER.non_never(), None);

        // Test non_unknown
        assert_eq!(TypeId::NEVER.non_unknown(), Some(TypeId::NEVER));
        assert_eq!(TypeId::UNKNOWN.non_unknown(), None);

        // Test non_any
        assert_eq!(TypeId::NEVER.non_any(), Some(TypeId::NEVER));
        assert_eq!(TypeId::ANY.non_any(), None);
    }

    #[test]
    fn test_type_id_ext_predicates() {
        // Test is_never
        assert!(TypeId::NEVER.is_never());
        assert!(!TypeId::UNKNOWN.is_never());

        // Test is_unknown
        assert!(TypeId::UNKNOWN.is_unknown());
        assert!(!TypeId::NEVER.is_unknown());

        // Test is_any
        assert!(TypeId::ANY.is_any());
        assert!(!TypeId::NEVER.is_any());

        // Test is_void
        assert!(TypeId::VOID.is_void());
        assert!(!TypeId::NEVER.is_void());
    }

    #[test]
    fn test_type_id_ext_unwrap_or_never() {
        assert_eq!(
            TypeId::unwrap_or_never(Some(TypeId::UNKNOWN)),
            TypeId::UNKNOWN
        );
        assert_eq!(TypeId::unwrap_or_never(None), TypeId::NEVER);
    }
}
