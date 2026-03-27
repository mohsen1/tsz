//! Shared utility functions for the solver module.
//!
//! This module contains common utilities used across multiple solver components
//! to avoid code duplication.

use std::borrow::Cow;

use crate::caches::db::TypeDatabase;
use crate::types::{ObjectShapeId, ParamInfo, PropertyInfo, PropertyLookup, TupleElement, TypeId};
use crate::visitor::{array_element_type, tuple_list_id};
use tsz_common::interner::Atom;

/// Count the number of required (non-optional, non-rest) parameters.
pub(crate) fn required_param_count(params: &[ParamInfo]) -> usize {
    params.iter().filter(|p| p.is_required()).count()
}

/// Count the number of required (non-optional, non-rest) tuple elements.
pub(crate) fn required_element_count(elements: &[TupleElement]) -> usize {
    elements.iter().filter(|e| e.is_required()).count()
}

/// Checks if a property name is numeric by resolving the atom and checking its string representation.
///
/// This function consolidates the previously duplicated `is_numeric_property_name` implementations
/// from operations.rs, evaluate.rs, subtype.rs, and infer.rs.
pub(crate) fn is_numeric_property_name(interner: &dyn TypeDatabase, name: Atom) -> bool {
    let prop_name = interner.resolve_atom_ref(name);
    is_numeric_literal_name(prop_name.as_ref())
}

/// Checks if a string represents a numeric literal name.
///
/// Returns `true` for:
/// - "`NaN`", "Infinity", "-Infinity"
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

/// Canonicalizes a numeric property name to its JavaScript canonical form.
///
/// If the input parses as a finite number, returns `Some(canonical_form)` where
/// `canonical_form` matches JavaScript's `Number.prototype.toString()`.
/// For example, `"1."`, `"1.0"`, and `"1"` all canonicalize to `"1"`.
/// Returns `None` if the name is not a numeric literal.
pub fn canonicalize_numeric_name(name: &str) -> Option<String> {
    let value: f64 = tsz_common::numeric::parse_numeric_literal_value(name)?;
    if !value.is_finite() && !value.is_nan() {
        return None;
    }
    Some(js_number_to_string(value).into_owned())
}

/// Converts a JavaScript number to its string representation.
///
/// This matches JavaScript's `Number.prototype.toString()` behavior for proper
/// numeric literal name checking.
///
/// Returns `Cow::Borrowed` for static special cases (NaN, 0, Infinity) and
/// `Cow::Owned` for dynamically formatted numbers.
fn js_number_to_string(value: f64) -> Cow<'static, str> {
    if value.is_nan() {
        return Cow::Borrowed("NaN");
    }
    if value == 0.0 {
        return Cow::Borrowed("0");
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            Cow::Borrowed("-Infinity")
        } else {
            Cow::Borrowed("Infinity")
        };
    }

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

    let formatted = value.to_string();
    if formatted == "-0" {
        Cow::Borrowed("0")
    } else {
        Cow::Owned(formatted)
    }
}

/// Reduces a vector of types to a union, single type, or NEVER.
///
/// This helper eliminates the common pattern:
/// ```text
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
/// ```text
/// let narrowed = union_or_single(db, filtered_members);
/// ```
pub fn union_or_single(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.union(types),
    }
}

/// Same as `union_or_single` but uses literal-only reduction (no subtype reduction).
/// Use this for union types from type annotations to preserve source structure.
pub fn union_or_single_literal_reduce(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.union_literal_reduce(types),
    }
}

/// Reduces a vector of types to an intersection, single type, or NEVER.
///
/// This helper eliminates the common pattern:
/// ```text
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
/// ```text
/// let narrowed = intersection_or_single(db, instance_types);
/// ```
pub fn intersection_or_single(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.intersection(types),
    }
}

/// Extension trait for `TypeId` with chainable methods for common operations.
///
/// This trait provides idiomatic Rust methods to reduce boilerplate when
/// working with `TypeId` values. Methods are designed to be chainable and
/// composable with iterator combinators.
///
/// # Examples
///
/// ```text
/// // Filter out NEVER types in a map operation
/// .filter_map(|&id| some_operation(id).non_never())
/// ```
pub trait TypeIdExt {
    /// Returns Some(self) if self is not NEVER, otherwise None.
    ///
    /// This is useful for `filter_map` chains where you want to skip NEVER results.
    fn non_never(self) -> Option<Self>
    where
        Self: Sized;
}

impl TypeIdExt for TypeId {
    #[inline]
    fn non_never(self) -> Option<Self> {
        (self != Self::NEVER).then_some(self)
    }
}

/// Look up a property by name, using the cached property index if available.
///
/// This consolidates the duplicated `lookup_property` implementations from
/// `subtype_rules/objects.rs` and `infer_bct.rs`.
#[inline]
pub(crate) fn lookup_property<'props>(
    db: &dyn TypeDatabase,
    props: &'props [PropertyInfo],
    shape_id: Option<ObjectShapeId>,
    name: Atom,
) -> Option<&'props PropertyInfo> {
    if let Some(shape_id) = shape_id {
        match db.object_property_index(shape_id, name) {
            PropertyLookup::Found(idx) => return props.get(idx),
            PropertyLookup::NotFound => return None,
            PropertyLookup::Uncached => {}
        }
    }
    props
        .binary_search_by_key(&name, |p| p.name)
        .ok()
        .map(|idx| &props[idx])
}

/// Find a common base type for a set of types using the provided `get_base` function.
///
/// Returns `Some(base)` if all types share the same base, `None` otherwise.
/// Used by both expression operations (literal widening) and BCT inference (nominal hierarchy).
pub(crate) fn find_common_base_type(
    types: &[TypeId],
    get_base: impl Fn(TypeId) -> Option<TypeId>,
) -> Option<TypeId> {
    let first_base = get_base(*types.first()?)?;
    for &ty in types.iter().skip(1) {
        if get_base(ty)? != first_base {
            return None;
        }
    }
    Some(first_base)
}

/// Get the effective read type of a property, adding `undefined` if the property is optional.
///
/// When a property is marked as optional (`prop.optional == true`), its read type
/// should include `undefined` to match TypeScript's behavior. This consolidates the
/// previously duplicated `optional_property_type` methods from `PropertyAccessEvaluator`,
/// `TypeEvaluator`, `CallEvaluator`, and `InferenceContext`.
///
/// Note: `SubtypeChecker` has its own version that respects `exactOptionalPropertyTypes`.
pub(crate) fn optional_property_type(db: &dyn TypeDatabase, prop: &PropertyInfo) -> TypeId {
    if prop.optional {
        db.union2(prop.type_id, TypeId::UNDEFINED)
    } else {
        prop.type_id
    }
}

/// Get the effective write type of a property, adding `undefined` if the property is optional.
///
/// Similar to [`optional_property_type`] but uses the property's `write_type` field instead.
pub(crate) fn optional_property_write_type(db: &dyn TypeDatabase, prop: &PropertyInfo) -> TypeId {
    if prop.optional {
        db.union2(prop.write_type, TypeId::UNDEFINED)
    } else {
        prop.write_type
    }
}

/// Check if two sorted property lists share at least one property name.
/// Used by both compat (top-level weak type detection) and subtype (nested weak type checks).
pub(crate) fn has_common_property_name(
    source_props: &[PropertyInfo],
    target_props: &[PropertyInfo],
) -> bool {
    let mut s_idx = 0;
    let mut t_idx = 0;
    while s_idx < source_props.len() && t_idx < target_props.len() {
        let s_name = source_props[s_idx].name;
        let t_name = target_props[t_idx].name;
        if s_name == t_name {
            return true;
        }
        if s_name < t_name {
            s_idx += 1;
        } else {
            t_idx += 1;
        }
    }
    false
}

/// Expansion of a tuple rest element into its constituent parts.
///
/// Used to normalize variadic tuples for subtype checking, call argument
/// matching, and best-common-type inference.
pub(crate) struct TupleRestExpansion {
    /// Fixed elements before the variadic portion (prefix)
    pub fixed: Vec<TupleElement>,
    /// The variadic element type (e.g., T for ...T[])
    pub variadic: Option<TypeId>,
    /// Fixed elements after the variadic portion (suffix/tail)
    pub tail: Vec<TupleElement>,
}

/// Expand a type into its tuple rest structure.
///
/// Handles three cases:
/// - Array types → empty fixed, variadic = element type, empty tail
/// - Tuple types → splits at the first rest element, recursing into nested rests
/// - Other types → treated as a variadic of the type itself
///
/// ## Examples:
/// - `number[]` → fixed: [], variadic: Some(number), tail: []
/// - `[string, number]` → fixed: [string, number], variadic: None, tail: []
/// - `[string, ...number[]]` → fixed: [string], variadic: Some(number), tail: []
/// - `[...T[], number]` → fixed: [], variadic: Some(T), tail: [number]
///
/// ## Recursive Expansion:
/// Nested rest elements are recursively expanded, so:
/// - `[A, ...[...B[], C]]` → fixed: [A], variadic: Some(B), tail: [C]
pub(crate) fn expand_tuple_rest(db: &dyn TypeDatabase, type_id: TypeId) -> TupleRestExpansion {
    if let Some(elem) = array_element_type(db, type_id) {
        return TupleRestExpansion {
            fixed: Vec::new(),
            variadic: Some(elem),
            tail: Vec::new(),
        };
    }

    if let Some(elements) = tuple_list_id(db, type_id) {
        let elements = db.tuple_list(elements);
        let mut fixed = Vec::new();
        for (i, elem) in elements.iter().enumerate() {
            if elem.rest {
                let inner = expand_tuple_rest(db, elem.type_id);
                fixed.extend(inner.fixed);
                // Capture tail elements: inner.tail + elements after the rest
                let mut tail = inner.tail;
                tail.extend(elements[i + 1..].iter().copied());
                return TupleRestExpansion {
                    fixed,
                    variadic: inner.variadic,
                    tail,
                };
            }
            fixed.push(*elem);
        }
        return TupleRestExpansion {
            fixed,
            variadic: None,
            tail: Vec::new(),
        };
    }

    TupleRestExpansion {
        fixed: Vec::new(),
        variadic: Some(type_id),
        tail: Vec::new(),
    }
}

#[cfg(test)]
#[path = "../tests/utils_tests.rs"]
mod tests;
