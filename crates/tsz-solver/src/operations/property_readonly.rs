//! Readonly property checks for types.
//!
//! Contains standalone functions for checking if properties are readonly
//! across different type kinds (objects, index signatures, mapped types, etc.).

use crate::TypeDatabase;
use crate::evaluation::evaluate::evaluate_type;
use crate::types::{
    CallableShapeId, MappedModifier, ObjectShapeId, PropertyInfo, TypeData, TypeId,
};
use crate::utils;

pub(crate) fn property_is_readonly(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    prop_name: &str,
) -> bool {
    match interner.lookup(type_id) {
        Some(TypeData::Lazy(_)) => {
            // Resolve lazy types (interfaces, classes, type aliases) before checking readonly.
            // If evaluation returns the same type (e.g. namespace types that don't resolve
            // through the evaluator), return false to prevent infinite recursion.
            let resolved = evaluate_type(interner, type_id);
            if resolved == type_id {
                return false;
            }
            property_is_readonly(interner, resolved, prop_name)
        }
        Some(TypeData::ReadonlyType(inner)) => {
            if let Some(TypeData::Array(_) | TypeData::Tuple(_)) = interner.lookup(inner) {
                // Numeric indices are readonly on readonly arrays/tuples
                if is_numeric_index_name(prop_name) {
                    return true;
                }
                // The 'length' property is also readonly on readonly arrays/tuples
                if prop_name == "length" {
                    return true;
                }
            }
            property_is_readonly(interner, inner, prop_name)
        }
        Some(TypeData::Object(shape_id)) => {
            tracing::trace!(
                "property_is_readonly: Object shape {:?} for prop {}",
                shape_id,
                prop_name
            );
            let result = object_property_is_readonly(interner, shape_id, prop_name);
            tracing::trace!("property_is_readonly: Object result = {}", result);
            result
        }
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            indexed_object_property_is_readonly(interner, shape_id, prop_name)
        }
        Some(TypeData::Callable(shape_id)) => {
            callable_property_is_readonly(interner, shape_id, prop_name)
        }
        Some(TypeData::Union(types)) => {
            // For unions: property is readonly if it's readonly in ANY constituent type
            let types = interner.type_list(types);
            types
                .iter()
                .any(|t| property_is_readonly(interner, *t, prop_name))
        }
        Some(TypeData::Intersection(types)) => {
            // For intersections: property is readonly ONLY if it's readonly in ALL constituent types
            // This allows assignment to `{ readonly a: number } & { a: number }` (mixed readonly/mutable)
            let types = interner.type_list(types);
            types
                .iter()
                .all(|t| property_is_readonly(interner, *t, prop_name))
        }
        Some(TypeData::Mapped(mapped_id)) => {
            // Mapped types with explicit readonly modifier (e.g., Readonly<T>)
            // have ALL properties readonly.
            let mapped = interner.get_mapped(mapped_id);
            mapped.readonly_modifier == Some(MappedModifier::Add)
        }
        _ => false,
    }
}

/// Check if a property on a plain object type is readonly.
fn object_property_is_readonly(
    interner: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    prop_name: &str,
) -> bool {
    let shape = interner.object_shape(shape_id);
    let prop_atom = interner.intern_string(prop_name);
    shape
        .properties
        .iter()
        .find(|prop| prop.name == prop_atom)
        .is_some_and(|prop| prop.readonly)
}

/// Check if a property on an indexed object type is readonly.
/// Checks both named properties and index signatures.
fn indexed_object_property_is_readonly(
    interner: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    prop_name: &str,
) -> bool {
    let shape = interner.object_shape(shape_id);
    let prop_atom = interner.intern_string(prop_name);

    // Check named property first
    if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, prop_atom) {
        return prop.readonly;
    }

    // Check string index signature for ALL property names
    if shape.string_index.as_ref().is_some_and(|idx| idx.readonly) {
        return true;
    }

    // Check numeric index signature for numeric properties
    if is_numeric_index_name(prop_name)
        && shape.number_index.as_ref().is_some_and(|idx| idx.readonly)
    {
        return true;
    }

    false
}

/// Check if a property on a callable type is readonly.
/// Checks both named properties and index signatures (for static index signatures on classes).
fn callable_property_is_readonly(
    interner: &dyn TypeDatabase,
    shape_id: CallableShapeId,
    prop_name: &str,
) -> bool {
    let shape = interner.callable_shape(shape_id);
    let prop_atom = interner.intern_string(prop_name);

    // Check named property first
    if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, prop_atom) {
        return prop.readonly;
    }

    // Check string index signature for ALL property names
    if shape.string_index.as_ref().is_some_and(|idx| idx.readonly) {
        return true;
    }

    // Check numeric index signature for numeric properties
    if is_numeric_index_name(prop_name)
        && shape.number_index.as_ref().is_some_and(|idx| idx.readonly)
    {
        return true;
    }

    false
}

/// Check if an index signature is readonly for the given type.
///
/// # Parameters
/// - `wants_string`: Check if string index signature should be readonly
/// - `wants_number`: Check if numeric index signature should be readonly
///
/// # Returns
/// `true` if the requested index signature is readonly, `false` otherwise.
///
/// # Examples
/// - `{ readonly [x: string]: string }` → `is_readonly_index_signature(t, true, false)` = `true`
/// - `{ [x: string]: string }` → `is_readonly_index_signature(t, true, false)` = `false`
pub fn is_readonly_index_signature(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    wants_string: bool,
    wants_number: bool,
) -> bool {
    use crate::objects::index_signatures::{IndexKind, IndexSignatureResolver};

    // PERF: Single lookup for Mapped and Union checks
    match interner.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = interner.get_mapped(mapped_id);
            if mapped.readonly_modifier == Some(MappedModifier::Add) {
                return true;
            }
        }
        Some(TypeData::Union(types)) => {
            let type_list = interner.type_list(types);
            let resolver = IndexSignatureResolver::new(interner);
            return type_list.iter().any(|&t| {
                (wants_string && resolver.is_readonly(t, IndexKind::String))
                    || (wants_number && resolver.is_readonly(t, IndexKind::Number))
            });
        }
        _ => {}
    }

    let resolver = IndexSignatureResolver::new(interner);

    if wants_string && resolver.is_readonly(type_id, IndexKind::String) {
        return true;
    }

    if wants_number && resolver.is_readonly(type_id, IndexKind::Number) {
        return true;
    }

    false
}

/// Check if a type is a mapped type with an explicit readonly modifier.
///
/// Returns `true` for types like `Readonly<T>` (`{ readonly [P in keyof T]: T[P] }`),
/// where the mapped type has `+readonly`. Resolves Lazy/Application wrappers.
///
/// For `Application(base, args)` (e.g., `Readonly<T>` where T is generic),
/// evaluates the base type alias to check if the underlying mapped type
/// has a readonly modifier.
pub fn is_mapped_type_with_readonly_modifier(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match interner.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = interner.get_mapped(mapped_id);
            mapped.readonly_modifier == Some(MappedModifier::Add)
        }
        Some(TypeData::Application(app_id)) => {
            let resolved = evaluate_type(interner, type_id);
            if resolved != type_id {
                return is_mapped_type_with_readonly_modifier(interner, resolved);
            }
            let app = interner.type_application(app_id);
            is_mapped_type_with_readonly_modifier(interner, app.base)
        }
        Some(TypeData::Lazy(_)) => {
            let resolved = evaluate_type(interner, type_id);
            if resolved == type_id {
                return false;
            }
            is_mapped_type_with_readonly_modifier(interner, resolved)
        }
        _ => false,
    }
}

/// Check if a string represents a valid numeric property name.
///
/// Returns `true` for numeric literals that TypeScript treats as valid numeric
/// property names, using the shared `utils::is_numeric_literal_name` helper.
///
/// This is used for determining if a property access can use numeric index signatures.
///
/// # Examples
/// - `is_numeric_index_name("0")` → `true`
/// - `is_numeric_index_name("42")` → `true`
/// - `is_numeric_index_name("1.5")` → `false` (fractional part)
/// - `is_numeric_index_name("-1")` → `false` (negative)
/// - `is_numeric_index_name("NaN")` → `false` (special value)
fn is_numeric_index_name(name: &str) -> bool {
    utils::is_numeric_literal_name(name)
}

/// Check if a numeric property name refers to a fixed (non-rest) tuple element
/// on a `ReadonlyType(Tuple(...))`.
///
/// Returns `true` when:
/// - `type_id` is `ReadonlyType(Tuple(elements))`, AND
/// - `prop_name` is a numeric index within the tuple's fixed (non-rest) element count.
///
/// This is used to distinguish TS2540 (readonly named property) from TS2542
/// (readonly index signature) on readonly tuples. For example, given
/// `readonly [number, number, ...number[]]`:
/// - `v[0]` → fixed element 0 → TS2540 ("Cannot assign to '0'...")
/// - `v[2]` → rest element range → TS2542 (index signature only permits reading)
pub fn is_readonly_tuple_fixed_element(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    prop_name: &str,
) -> bool {
    let Some(TypeData::ReadonlyType(inner)) = interner.lookup(type_id) else {
        return false;
    };
    let Some(TypeData::Tuple(list_id)) = interner.lookup(inner) else {
        return false;
    };
    let index: usize = match prop_name.parse() {
        Ok(i) => i,
        Err(_) => return false,
    };
    let elements = interner.tuple_list(list_id);
    // Count fixed (non-rest) elements before any rest element
    let fixed_count = elements.iter().take_while(|e| !e.rest).count();
    index < fixed_count
}

// =============================================================================
// Binary Operations - Extracted to binary_ops.rs
// =============================================================================
//
// Binary operation evaluation has been extracted to `operations/binary_ops.rs`.
// The following are re-exported from that module:
// - BinaryOpEvaluator
// - BinaryOpResult
// - PrimitiveClass
//
// This extraction reduces operations.rs by ~330 lines and makes the code
// more maintainable by separating concerns.
