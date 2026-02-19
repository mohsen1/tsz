//! Readonly property checks for types.
//!
//! Contains standalone functions for checking if properties are readonly
//! across different type kinds (objects, index signatures, mapped types, etc.).

use crate::TypeDatabase;
use crate::evaluate::evaluate_type;
use crate::types::{ObjectShapeId, TypeData, TypeId};
use crate::utils;

pub fn property_is_readonly(interner: &dyn TypeDatabase, type_id: TypeId, prop_name: &str) -> bool {
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
    if let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_atom) {
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
    use crate::index_signatures::{IndexKind, IndexSignatureResolver};

    // Handle Union types - index signature is readonly if ANY member has it readonly
    if let Some(TypeData::Union(types)) = interner.lookup(type_id) {
        let type_list = interner.type_list(types);
        let resolver = IndexSignatureResolver::new(interner);
        return type_list.iter().any(|&t| {
            (wants_string && resolver.is_readonly(t, IndexKind::String))
                || (wants_number && resolver.is_readonly(t, IndexKind::Number))
        });
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

// =============================================================================
// Binary Operations - Extracted to binary_ops.rs
// =============================================================================
//
// Binary operation evaluation has been extracted to `solver/binary_ops.rs`.
// The following are re-exported from that module:
// - BinaryOpEvaluator
// - BinaryOpResult
// - PrimitiveClass
//
// This extraction reduces operations.rs by ~330 lines and makes the code
// more maintainable by separating concerns.
