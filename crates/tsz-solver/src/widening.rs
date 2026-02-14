//! Type widening operations for literal types.
//!
//! This module implements TypeScript's type widening rules, where literal types
//! are widened to their primitive types in certain contexts for usability.
//!
//! ## Widening Rules
//!
//! - **String literals** → `string`
//! - **Number literals** → `number`
//! - **Boolean literals** → `boolean`
//! - **BigInt literals** → `bigint`
//! - **Union types**: All members are widened recursively
//! - **Object types**: Property types are widened unless `readonly`
//! - **Type parameters**: Never widened
//! - **Unique symbols**: Never widened

use crate::types::{LiteralValue, TypeId, TypeKey};

/// Public API to widen a literal type to its primitive.
///
/// This is the main entry point for type widening in the checker.
///
/// ## Example
///
/// ```rust,ignore
/// use crate::widening::widen_type;
///
/// // Widen a literal string to the string primitive
/// let widened = widen_type(db, string_literal_type);
/// assert_eq!(widened, TypeId::STRING);
/// ```
pub fn widen_type(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        // String/Number/Boolean/BigInt literals widen to their primitives
        Some(TypeKey::Literal(ref value)) => match value {
            LiteralValue::String(_) => TypeId::STRING,
            LiteralValue::Number(_) => TypeId::NUMBER,
            LiteralValue::Boolean(_) => TypeId::BOOLEAN,
            LiteralValue::BigInt(_) => TypeId::BIGINT,
        },

        // Unique Symbol widens to Symbol
        Some(TypeKey::UniqueSymbol(_)) => TypeId::SYMBOL,

        // Unions: recursively widen all members
        Some(TypeKey::Union(list_id)) => {
            let members = db.type_list(list_id);
            let widened_members: Vec<TypeId> = members.iter().map(|&m| widen_type(db, m)).collect();
            db.union(widened_members)
        }

        // Objects: recursively widen properties (critical for mutable variables)
        Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let mut new_props = Vec::with_capacity(shape.properties.len());
            let mut changed = false;

            for prop in &shape.properties {
                // Rule: Readonly properties are NOT widened
                let widened_type = if prop.readonly {
                    prop.type_id
                } else {
                    widen_type(db, prop.type_id)
                };

                // Write type follows read type logic
                let widened_write_type = if prop.readonly {
                    prop.write_type
                } else {
                    widen_type(db, prop.write_type)
                };

                if widened_type != prop.type_id || widened_write_type != prop.write_type {
                    changed = true;
                }
                let mut new_prop = prop.clone();
                new_prop.type_id = widened_type;
                new_prop.write_type = widened_write_type;
                new_props.push(new_prop);
            }

            if changed {
                // If we have index signatures, we must preserve them using object_with_index
                if shape.string_index.is_some() || shape.number_index.is_some() {
                    let mut new_shape = (*shape).clone();
                    new_shape.properties = new_props;
                    db.object_with_index(new_shape)
                } else {
                    db.object(new_props)
                }
            } else {
                type_id
            }
        }

        // All other types are not widened:
        // - Primitives (already widened)
        // - Type parameters (preserve identity)
        // - Refs/Lazy (preserve what they resolve to)
        // - Intrinsics (already widened)
        // - Enums (nominal identity)
        // - Applications (Array<T>, Promise<T>, etc.)
        _ => type_id,
    }
}

/// Apply `as const` assertion to a type.
///
/// This function transforms a type to its const-asserted form:
/// - Literals: Preserved as-is
/// - Arrays: Converted to readonly tuples
/// - Tuples: Marked readonly, elements recursively const-asserted
/// - Objects: All properties marked readonly, recursively const-asserted
/// - Other types: Preserved as-is (any, unknown, primitives, etc.)
///
/// # Example
///
/// ```rust,ignore
/// use crate::widening::apply_const_assertion;
///
/// // [1, 2] as const becomes readonly [1, 2] (tuple)
/// let array_type = interner.array(interner.literal_number(1));
/// let const_array = apply_const_assertion(&interner, array_type);
/// ```
pub fn apply_const_assertion(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    use crate::visitor::ConstAssertionVisitor;
    let mut visitor = ConstAssertionVisitor::new(db);
    visitor.apply_const_assertion(type_id)
}

#[cfg(test)]
#[path = "../tests/widening_tests.rs"]
mod tests;
