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

use crate::solver::types::{LiteralValue, TypeId, TypeKey};

/// Public API to widen a literal type to its primitive.
///
/// This is the main entry point for type widening in the checker.
///
/// ## Example
///
/// ```rust
/// use crate::solver::widening::widen_type;
///
/// // Widen a literal string to the string primitive
/// let widened = widen_type(db, string_literal_type);
/// assert_eq!(widened, TypeId::STRING);
/// ```
pub fn widen_type(db: &impl crate::solver::TypeDatabase, type_id: TypeId) -> TypeId {
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
/// ```rust
/// use crate::solver::widening::apply_const_assertion;
///
/// // [1, 2] as const becomes readonly [1, 2] (tuple)
/// let array_type = interner.array(interner.literal_number(1));
/// let const_array = apply_const_assertion(&interner, array_type);
/// ```
pub fn apply_const_assertion(db: &dyn crate::solver::TypeDatabase, type_id: TypeId) -> TypeId {
    use crate::solver::visitor::ConstAssertionVisitor;
    let mut visitor = ConstAssertionVisitor::new(db);
    visitor.apply_const_assertion(type_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interner::Atom;
    use crate::solver::TypeInterner;
    use crate::solver::types::{
        LiteralValue, OrderedFloat, PropertyInfo, SymbolRef, TypeKey, TypeParamInfo, Visibility,
    };

    #[test]
    fn test_widen_string_literal() {
        let interner = TypeInterner::new();
        let string_lit = interner.intern(TypeKey::Literal(LiteralValue::String(
            interner.intern_string("hello"),
        )));
        let widened = widen_type(&interner, string_lit);
        assert_eq!(widened, TypeId::STRING);
    }

    #[test]
    fn test_widen_number_literal() {
        let interner = TypeInterner::new();
        let number_lit =
            interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(42.0))));
        let widened = widen_type(&interner, number_lit);
        assert_eq!(widened, TypeId::NUMBER);
    }

    #[test]
    fn test_widen_boolean_literal() {
        let interner = TypeInterner::new();
        let bool_lit = interner.intern(TypeKey::Literal(LiteralValue::Boolean(true)));
        let widened = widen_type(&interner, bool_lit);
        assert_eq!(widened, TypeId::BOOLEAN);
    }

    #[test]
    fn test_widen_union() {
        let interner = TypeInterner::new();
        let lit1 = interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0))));
        let lit2 = interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(2.0))));
        let union = interner.union(vec![lit1, lit2]);

        let widened = widen_type(&interner, union);
        // After widening, we get number | number which dedups to number
        assert_eq!(widened, TypeId::NUMBER);
    }

    #[test]
    fn test_widen_primitive_preserved() {
        let interner = TypeInterner::new();
        // Primitives should be preserved (already widened)
        let widened = widen_type(&interner, TypeId::STRING);
        assert_eq!(widened, TypeId::STRING);
    }

    #[test]
    fn test_type_param_not_widened() {
        let interner = TypeInterner::new();
        // Type parameters are NOT widened
        let name = interner.intern_string("T");
        let info = TypeParamInfo {
            name,
            constraint: Some(
                interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
            ),
            default: None,
            is_const: false,
        };
        let type_param = interner.intern(TypeKey::TypeParameter(info));

        let widened = widen_type(&interner, type_param);
        // Should preserve the original type_param type
        assert_eq!(widened, type_param);
    }

    #[test]
    fn test_widen_unique_symbol() {
        let interner = TypeInterner::new();
        let unique_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(42)));
        let widened = widen_type(&interner, unique_sym);
        assert_eq!(widened, TypeId::SYMBOL);
    }

    #[test]
    fn test_widen_object_properties() {
        let interner = TypeInterner::new();
        // Create object { x: 1 } where x is a literal number
        let props = vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
            write_type: interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];
        let obj_type = interner.object(props);

        let widened = widen_type(&interner, obj_type);

        // Check that the widened type has number, not the literal 1
        let widened_key = interner.lookup(widened);
        match widened_key {
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = interner.object_shape(shape_id);
                assert_eq!(shape.properties.len(), 1);
                assert_eq!(shape.properties[0].type_id, TypeId::NUMBER);
                assert_eq!(shape.properties[0].write_type, TypeId::NUMBER);
            }
            _ => panic!("Expected widened object type"),
        }
    }

    #[test]
    fn test_widen_nested_object_properties() {
        let interner = TypeInterner::new();
        // Create nested object { a: { b: "hello" } }
        let inner_props = vec![PropertyInfo {
            name: interner.intern_string("b"),
            type_id: interner.intern(TypeKey::Literal(LiteralValue::String(
                interner.intern_string("hello"),
            ))),
            write_type: interner.intern(TypeKey::Literal(LiteralValue::String(
                interner.intern_string("hello"),
            ))),
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];
        let inner_obj = interner.object(inner_props);

        let outer_props = vec![PropertyInfo {
            name: interner.intern_string("a"),
            type_id: inner_obj,
            write_type: inner_obj,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];
        let outer_obj = interner.object(outer_props);

        let widened = widen_type(&interner, outer_obj);

        // Check that both inner and outer properties are widened
        let widened_key = interner.lookup(widened);
        match widened_key {
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = interner.object_shape(shape_id);
                assert_eq!(shape.properties.len(), 1);

                // Outer property 'a' should be an object
                let inner_type = shape.properties[0].type_id;
                let inner_key = interner.lookup(inner_type);
                match inner_key {
                    Some(TypeKey::Object(inner_shape_id))
                    | Some(TypeKey::ObjectWithIndex(inner_shape_id)) => {
                        let inner_shape = interner.object_shape(inner_shape_id);
                        assert_eq!(inner_shape.properties.len(), 1);
                        // Inner property 'b' should be widened to string
                        assert_eq!(inner_shape.properties[0].type_id, TypeId::STRING);
                    }
                    _ => panic!("Expected inner object type"),
                }
            }
            _ => panic!("Expected widened object type"),
        }
    }

    #[test]
    fn test_widen_readonly_property_preserved() {
        let interner = TypeInterner::new();
        // { a: 1, readonly b: 2 }
        let props = vec![
            PropertyInfo {
                name: interner.intern_string("a"),
                type_id: interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
                write_type: interner
                    .intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
                optional: false,
                readonly: false, // Mutable -> Widens
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
            PropertyInfo {
                name: interner.intern_string("b"),
                type_id: interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(2.0)))),
                write_type: interner
                    .intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(2.0)))),
                optional: false,
                readonly: true, // Readonly -> Preserved
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
        ];
        let obj_type = interner.object(props);
        let widened = widen_type(&interner, obj_type);

        // Verify 'a' is number, 'b' is literal 2
        let shape = match interner.lookup(widened).unwrap() {
            TypeKey::Object(id) => interner.object_shape(id),
            _ => panic!("Expected object"),
        };

        let a = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "a")
            .unwrap();
        let b = shape
            .properties
            .iter()
            .find(|p| interner.resolve_atom(p.name) == "b")
            .unwrap();

        assert_eq!(a.type_id, TypeId::NUMBER);
        assert!(matches!(
            interner.lookup(b.type_id),
            Some(TypeKey::Literal(_))
        ));
    }
}
