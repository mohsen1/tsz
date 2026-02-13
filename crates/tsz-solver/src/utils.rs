//! Shared utility functions for the solver module.
//!
//! This module contains common utilities used across multiple solver components
//! to avoid code duplication.

use crate::db::TypeDatabase;
use crate::types::TypeId;
use tsz_common::interner::Atom;

/// Extension trait for iterators of TypeId to collect into unions or intersections.
///
/// This trait provides ergonomic methods to reduce boilerplate when collecting
/// types into type unions or intersections, automatically handling the common
/// cases of empty collections, single elements, and multiple elements.
///
/// # Examples
///
/// ```ignore
/// // Collect filtered types into a union
/// let result = members
///     .iter()
///     .filter(|&t| predicate(t))
///     .union_or_single(db);
///
/// // Collect mapped types into an intersection
/// let result = types
///     .iter()
///     .map(|&t| transform(t))
///     .filter(|&t| t != TypeId::NEVER)
///     .intersection_or_single(db);
/// ```
#[allow(dead_code)]
pub trait TypeIdIteratorExt: Iterator<Item = TypeId> + Sized {
    /// Collects types into a union, returning NEVER for empty, single type for one element,
    /// or a union type for multiple elements.
    ///
    /// This is equivalent to calling `.collect()` and then `union_or_single()`.
    fn union_or_single(self, db: &dyn TypeDatabase) -> TypeId;

    /// Collects types into an intersection, returning NEVER for empty, single type for one element,
    /// or an intersection type for multiple elements.
    ///
    /// This is equivalent to calling `.collect()` and then `intersection_or_single()`.
    fn intersection_or_single(self, db: &dyn TypeDatabase) -> TypeId;
}

impl<I> TypeIdIteratorExt for I
where
    I: Iterator<Item = TypeId>,
{
    #[inline]
    fn union_or_single(self, db: &dyn TypeDatabase) -> TypeId {
        union_or_single(db, self.collect())
    }

    #[inline]
    fn intersection_or_single(self, db: &dyn TypeDatabase) -> TypeId {
        intersection_or_single(db, self.collect())
    }
}

/// Extension trait for slices of TypeId to create unions or intersections without cloning.
///
/// This trait provides zero-allocation methods for creating unions or intersections
/// from slices of TypeId, avoiding the need to clone the slice into a Vec.
///
/// # Examples
///
/// ```ignore
/// // Create union from slice without cloning
/// let types = [TypeId::STRING, TypeId::NUMBER];
/// let result = types.as_slice().union_or_single(db);
///
/// // Create intersection from slice
/// let result = types.as_slice().intersection_or_single(db);
/// ```
#[allow(dead_code)]
pub trait TypeIdSliceExt {
    /// Creates a union from a slice, returning NEVER for empty, single type for one element,
    /// or a union type for multiple elements.
    fn union_or_single(&self, db: &dyn TypeDatabase) -> TypeId;

    /// Creates an intersection from a slice, returning NEVER for empty, single type for one element,
    /// or an intersection type for multiple elements.
    fn intersection_or_single(&self, db: &dyn TypeDatabase) -> TypeId;
}

impl TypeIdSliceExt for [TypeId] {
    #[inline]
    fn union_or_single(&self, db: &dyn TypeDatabase) -> TypeId {
        match self.len() {
            0 => TypeId::NEVER,
            1 => self[0],
            _ => db.union(self.to_vec()),
        }
    }

    #[inline]
    fn intersection_or_single(&self, db: &dyn TypeDatabase) -> TypeId {
        match self.len() {
            0 => TypeId::NEVER,
            1 => self[0],
            _ => db.intersection(self.to_vec()),
        }
    }
}

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

    /// Maps an Option<TypeId> to TypeId, using UNKNOWN as the default.
    fn unwrap_or_unknown(option: Option<Self>) -> Self
    where
        Self: Sized;

    /// Maps an Option<TypeId> to TypeId, using ANY as the default.
    fn unwrap_or_any(option: Option<Self>) -> Self
    where
        Self: Sized;

    /// Maps an Option<TypeId> to TypeId, using VOID as the default.
    fn unwrap_or_void(option: Option<Self>) -> Self
    where
        Self: Sized;

    /// Maps an Option<TypeId> to TypeId, using UNDEFINED as the default.
    fn unwrap_or_undefined(option: Option<Self>) -> Self
    where
        Self: Sized;

    /// Returns true if this is either NEVER, UNKNOWN, or ANY.
    ///
    /// Useful for checking if a type represents an error or placeholder state.
    fn is_error_like(&self) -> bool;

    /// Returns true if this is a definite value type (not NEVER, UNKNOWN, ANY, or VOID).
    ///
    /// Useful for checking if a type represents a concrete value.
    fn is_concrete(&self) -> bool;

    /// Returns true if this is NULL, UNDEFINED, or a union containing them.
    ///
    /// Note: This is a simple check for the primitive types only, not union members.
    fn is_nullish(&self) -> bool;
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

    #[inline]
    fn unwrap_or_unknown(option: Option<Self>) -> Self {
        option.unwrap_or(TypeId::UNKNOWN)
    }

    #[inline]
    fn unwrap_or_any(option: Option<Self>) -> Self {
        option.unwrap_or(TypeId::ANY)
    }

    #[inline]
    fn unwrap_or_void(option: Option<Self>) -> Self {
        option.unwrap_or(TypeId::VOID)
    }

    #[inline]
    fn unwrap_or_undefined(option: Option<Self>) -> Self {
        option.unwrap_or(TypeId::UNDEFINED)
    }

    #[inline]
    fn is_error_like(&self) -> bool {
        *self == TypeId::NEVER || *self == TypeId::UNKNOWN || *self == TypeId::ANY
    }

    #[inline]
    fn is_concrete(&self) -> bool {
        *self != TypeId::NEVER
            && *self != TypeId::UNKNOWN
            && *self != TypeId::ANY
            && *self != TypeId::VOID
    }

    #[inline]
    fn is_nullish(&self) -> bool {
        *self == TypeId::NULL || *self == TypeId::UNDEFINED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;

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

    #[test]
    fn test_type_id_ext_unwrap_or_defaults() {
        // Test unwrap_or_unknown
        assert_eq!(
            TypeId::unwrap_or_unknown(Some(TypeId::NEVER)),
            TypeId::NEVER
        );
        assert_eq!(TypeId::unwrap_or_unknown(None), TypeId::UNKNOWN);

        // Test unwrap_or_any
        assert_eq!(TypeId::unwrap_or_any(Some(TypeId::NEVER)), TypeId::NEVER);
        assert_eq!(TypeId::unwrap_or_any(None), TypeId::ANY);

        // Test unwrap_or_void
        assert_eq!(TypeId::unwrap_or_void(Some(TypeId::NEVER)), TypeId::NEVER);
        assert_eq!(TypeId::unwrap_or_void(None), TypeId::VOID);

        // Test unwrap_or_undefined
        assert_eq!(
            TypeId::unwrap_or_undefined(Some(TypeId::NEVER)),
            TypeId::NEVER
        );
        assert_eq!(TypeId::unwrap_or_undefined(None), TypeId::UNDEFINED);
    }

    #[test]
    fn test_iterator_ext_union_or_single() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // Empty iterator -> NEVER
        let result = std::iter::empty().union_or_single(db);
        assert_eq!(result, TypeId::NEVER);

        // Single element -> that element
        let result = std::iter::once(TypeId::STRING).union_or_single(db);
        assert_eq!(result, TypeId::STRING);

        // Multiple elements -> union
        let types = vec![TypeId::STRING, TypeId::NUMBER];
        let result = types.into_iter().union_or_single(db);
        // Verify it's a union (not one of the inputs)
        assert_ne!(result, TypeId::STRING);
        assert_ne!(result, TypeId::NUMBER);
    }

    #[test]
    fn test_iterator_ext_intersection_or_single() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // Empty iterator -> NEVER
        let result = std::iter::empty().intersection_or_single(db);
        assert_eq!(result, TypeId::NEVER);

        // Single element -> that element
        let result = std::iter::once(TypeId::STRING).intersection_or_single(db);
        assert_eq!(result, TypeId::STRING);

        // Multiple elements -> intersection
        let types = vec![TypeId::STRING, TypeId::NUMBER];
        let result = types.into_iter().intersection_or_single(db);
        // Verify it's an intersection (not one of the inputs)
        assert_ne!(result, TypeId::STRING);
        assert_ne!(result, TypeId::NUMBER);
    }

    #[test]
    fn test_type_id_ext_is_error_like() {
        // Error-like types
        assert!(TypeId::NEVER.is_error_like());
        assert!(TypeId::UNKNOWN.is_error_like());
        assert!(TypeId::ANY.is_error_like());

        // Not error-like
        assert!(!TypeId::STRING.is_error_like());
        assert!(!TypeId::NUMBER.is_error_like());
        assert!(!TypeId::VOID.is_error_like());
        assert!(!TypeId::NULL.is_error_like());
    }

    #[test]
    fn test_type_id_ext_is_concrete() {
        // Concrete types
        assert!(TypeId::STRING.is_concrete());
        assert!(TypeId::NUMBER.is_concrete());
        assert!(TypeId::BOOLEAN.is_concrete());
        assert!(TypeId::NULL.is_concrete());
        assert!(TypeId::UNDEFINED.is_concrete());

        // Not concrete
        assert!(!TypeId::NEVER.is_concrete());
        assert!(!TypeId::UNKNOWN.is_concrete());
        assert!(!TypeId::ANY.is_concrete());
        assert!(!TypeId::VOID.is_concrete());
    }

    #[test]
    fn test_type_id_ext_is_nullish() {
        // Nullish types
        assert!(TypeId::NULL.is_nullish());
        assert!(TypeId::UNDEFINED.is_nullish());

        // Not nullish
        assert!(!TypeId::STRING.is_nullish());
        assert!(!TypeId::NUMBER.is_nullish());
        assert!(!TypeId::NEVER.is_nullish());
        assert!(!TypeId::UNKNOWN.is_nullish());
        assert!(!TypeId::ANY.is_nullish());
    }

    #[test]
    fn test_slice_ext_union_or_single() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // Empty slice -> NEVER
        let types: &[TypeId] = &[];
        let result = types.union_or_single(db);
        assert_eq!(result, TypeId::NEVER);

        // Single element -> that element
        let types = &[TypeId::STRING];
        let result = types.union_or_single(db);
        assert_eq!(result, TypeId::STRING);

        // Multiple elements -> union
        let types = &[TypeId::STRING, TypeId::NUMBER];
        let result = types.union_or_single(db);
        assert_ne!(result, TypeId::STRING);
        assert_ne!(result, TypeId::NUMBER);
    }

    #[test]
    fn test_slice_ext_intersection_or_single() {
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;

        // Empty slice -> NEVER
        let types: &[TypeId] = &[];
        let result = types.intersection_or_single(db);
        assert_eq!(result, TypeId::NEVER);

        // Single element -> that element
        let types = &[TypeId::STRING];
        let result = types.intersection_or_single(db);
        assert_eq!(result, TypeId::STRING);

        // Multiple elements -> intersection
        let types = &[TypeId::STRING, TypeId::NUMBER];
        let result = types.intersection_or_single(db);
        assert_ne!(result, TypeId::STRING);
        assert_ne!(result, TypeId::NUMBER);
    }
}
