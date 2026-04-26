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
//! - **`BigInt` literals** → `bigint`
//! - **Union types**: All members are widened recursively
//! - **Object types**: Property types are widened unless `readonly`
//! - **Type parameters**: Never widened
//! - **Unique symbols**: Never widened

use crate::types::{ObjectFlags, TypeData, TypeId};

/// Propagate `display_alias` from the original type to the widened type.
///
/// When a type produced by evaluating a generic Application (e.g., `Record<string, 1>`)
/// is widened (e.g., to `Record<string, number>`-shaped object), the new TypeId loses
/// its `display_alias` mapping. This function copies the mapping forward so the formatter
/// can still show the alias name instead of the expanded structural form.
#[inline]
fn propagate_display_alias(db: &dyn crate::TypeDatabase, original: TypeId, widened: TypeId) {
    if original != widened
        && let Some(alias) = db.get_display_alias(original)
    {
        db.store_display_alias(widened, alias);
    }
}

/// Public API to widen a literal type to its primitive.
///
/// This is the main entry point for type widening in the checker.
///
/// ## Example
///
/// ```text
/// use crate::operations::widening::widen_type;
///
/// // Widen a literal string to the string primitive
/// let widened = widen_type(db, string_literal_type);
/// assert_eq!(widened, TypeId::STRING);
/// ```
pub fn widen_type(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    // Fast path: most intrinsic types are already widened and never change.
    // Boolean literals (true/false) are intrinsic but DO need widening to boolean.
    if type_id.is_intrinsic()
        && type_id != crate::types::TypeId::BOOLEAN_TRUE
        && type_id != crate::types::TypeId::BOOLEAN_FALSE
    {
        return type_id;
    }
    // Fast path: non-literal, non-union, non-intersection types don't widen.
    // Object, Function, Callable, Array, Tuple, TypeParameter, etc. are stable.
    if matches!(
        db.lookup(type_id),
        Some(
            crate::types::TypeData::Function(_)
                | crate::types::TypeData::Callable(_)
                | crate::types::TypeData::TypeParameter(_)
                | crate::types::TypeData::Enum(_, _)
                | crate::types::TypeData::Mapped(_)
                | crate::types::TypeData::Conditional(_)
                | crate::types::TypeData::Application(_)
                | crate::types::TypeData::Lazy(_)
                | crate::types::TypeData::IndexAccess(_, _)
                | crate::types::TypeData::KeyOf(_)
                | crate::types::TypeData::TemplateLiteral(_)
                | crate::types::TypeData::ThisType
                | crate::types::TypeData::Error
        )
    ) {
        return type_id;
    }
    // Note: literals, unions, intersections, objects, arrays, and tuples may
    // still contain widenable data, so they must go through the full path.
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, true, true, false)
}

/// Widen for diagnostic display: like `widen_type` but preserves boolean
/// literal intrinsics (`true`/`false`) so that narrowed types like
/// `string | false` display correctly instead of `string | boolean`.
///
/// Does NOT recurse into function/callable parameter types, preserving
/// literal parameters so `(x: "bar") => number` displays as-is rather than
/// being widened to `(x: string) => number`.
pub fn widen_type_for_display(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, false, false, false)
}

/// Widen type for inference resolution: like `widen_type` but does NOT
/// recurse into function/callable parameter or return types.
///
/// tsc's `getInferredType` deep-widens fresh object literals but preserves
/// function types as-is. Widening function params in contravariant positions
/// (e.g., `(x: 1 | 2) => void` → `(x: number) => void`) creates a resolved T
/// that is structurally incompatible with the original arg type under strict
/// function type checking, causing false TS2322.
pub(crate) fn widen_type_for_inference(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, true, false, true)
}

/// Display-widen a type for TS2403 (subsequent variable declaration) messages.
///
/// Deep-widens fresh literal types nested inside compound shapes (function
/// return types, object property types) so the printer renders widened forms
/// like `{ x: number; y: number; }` rather than `{ x: 0; y: 0; }`. But
/// preserves top-level literal and union-of-literal types so explicit
/// annotations like `var x: 5; var x: 6;` keep their literal form (`'5'` /
/// `'6'`) instead of collapsing to `'number'` / `'number'` (which would also
/// self-suppress the diagnostic via the equal-display short-circuit in the
/// reporter).
pub fn display_widen_for_redeclaration(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    if matches!(
        db.lookup(type_id),
        Some(crate::types::TypeData::Literal(_) | crate::types::TypeData::Union(_))
    ) {
        return type_id;
    }
    widen_type_deep(db, type_id)
}

/// Deep-widen a type including inside function/callable signatures.
///
/// Unlike `widen_type` which skips Function/Callable types for performance
/// and correctness in the general case, this variant recurses into function
/// return types and parameter types. Used for TS2403 redeclaration checking
/// where `var fn = (s: string) => 3` should compare as `(s: string) => number`
/// against `var fn: (s: string) => number`.
pub fn widen_type_deep(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    // Fast path: intrinsics (except boolean literals)
    if type_id.is_intrinsic()
        && type_id != crate::types::TypeId::BOOLEAN_TRUE
        && type_id != crate::types::TypeId::BOOLEAN_FALSE
    {
        return type_id;
    }
    // Skip types that never contain widenable data, but NOT Function/Callable
    if matches!(
        db.lookup(type_id),
        Some(
            crate::types::TypeData::TypeParameter(_)
                | crate::types::TypeData::Enum(_, _)
                | crate::types::TypeData::Mapped(_)
                | crate::types::TypeData::Conditional(_)
                | crate::types::TypeData::Application(_)
                | crate::types::TypeData::Lazy(_)
                | crate::types::TypeData::IndexAccess(_, _)
                | crate::types::TypeData::KeyOf(_)
                | crate::types::TypeData::TemplateLiteral(_)
                | crate::types::TypeData::ThisType
                | crate::types::TypeData::Error
        )
    ) {
        return type_id;
    }
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, true, true, false)
}

fn widen_type_cached(
    db: &dyn crate::TypeDatabase,
    type_id: TypeId,
    cache: &mut rustc_hash::FxHashMap<TypeId, TypeId>,
    widen_boolean_intrinsics: bool,
    widen_functions: bool,
    widen_object_union_members: bool,
) -> TypeId {
    // Fast path: most intrinsic types are never widened, but boolean
    // literal intrinsics (BOOLEAN_TRUE / BOOLEAN_FALSE) must widen to BOOLEAN.
    if (type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE)
        && widen_boolean_intrinsics
    {
        return TypeId::BOOLEAN;
    }
    if type_id.is_intrinsic() {
        return type_id;
    }

    if let Some(&cached) = cache.get(&type_id) {
        return cached;
    }

    // Insert a sentinel before recursing to break cycles on recursive types
    // like `D<T> { recurse: D<T>; wrapped: D<D<T>>; }`. If we encounter
    // this type_id again during recursive widening, we return the original
    // type_id (no widening) instead of diverging.
    cache.insert(type_id, type_id);

    let result = match db.lookup(type_id) {
        // String/Number/Boolean/BigInt literals widen to their primitives
        Some(TypeData::Literal(ref value)) => value.primitive_type_id(),

        // Unique Symbol widens to Symbol
        Some(TypeData::UniqueSymbol(_)) => TypeId::SYMBOL,

        // Unions: only widen if the union itself requires widening.
        // tsc's getWidenedType only widens types with the RequiresWidening flag.
        // A union like `1 | 2` from fresh expressions requires widening (→ number).
        // A union like `"fr" | "en" | "es"` from a type alias does NOT.
        //
        // We approximate tsc's RequiresWidening heuristic: a union requires widening
        // when it has ≤ 3 members, at least one of which is a literal type, and
        // all members are either literals or non-widenable intrinsics (undefined,
        // null, void). Larger unions are more likely from type aliases.
        // This handles: `1 | 2`, `true | false`, `"a" | 1`, `true | undefined`.
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let is_fresh_member = |m: TypeId| -> bool {
                matches!(
                    db.lookup(m),
                    Some(TypeData::Literal(_) | TypeData::UniqueSymbol(_))
                ) || m == TypeId::BOOLEAN_TRUE
                    || m == TypeId::BOOLEAN_FALSE
            };
            // Allow undefined/null/void as union members — they don't need
            // widening themselves but shouldn't prevent literal siblings from
            // being widened. E.g., `true | undefined` → `boolean | undefined`.
            let is_passthrough_intrinsic = |m: TypeId| -> bool {
                m == TypeId::UNDEFINED || m == TypeId::NULL || m == TypeId::VOID
            };
            let is_fresh_object_member = |m: TypeId| -> bool {
                match db.lookup(m) {
                    Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => db
                        .object_shape(shape_id)
                        .flags
                        .contains(ObjectFlags::FRESH_LITERAL),
                    _ => false,
                }
            };
            let is_fresh_object_or_array_member = |m: TypeId| -> bool {
                match db.lookup(m) {
                    Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_)) => {
                        is_fresh_object_member(m)
                    }
                    Some(TypeData::Array(_) | TypeData::Tuple(_)) => true,
                    _ => false,
                }
            };
            let has_literal = members.iter().any(|&m| is_fresh_member(m));
            let small_fresh_union = has_literal
                && members.len() <= 3
                && members
                    .iter()
                    .all(|&m| is_fresh_member(m) || is_passthrough_intrinsic(m));
            let has_fresh_object_or_array_member =
                members.iter().any(|&m| is_fresh_object_or_array_member(m));
            if small_fresh_union || (widen_object_union_members && has_fresh_object_or_array_member)
            {
                let mut members_to_widen = members.to_vec();
                if widen_object_union_members {
                    let fresh_object_members: Vec<TypeId> = members
                        .iter()
                        .copied()
                        .filter(|&member| is_fresh_object_member(member))
                        .collect();
                    if let Some(normalized_objects) =
                        super::expression_ops::normalize_fresh_object_literal_union_members(
                            db,
                            &fresh_object_members,
                        )
                    {
                        let mut normalized = normalized_objects.into_iter();
                        for member in &mut members_to_widen {
                            if is_fresh_object_member(*member)
                                && let Some(normalized_member) = normalized.next()
                            {
                                *member = normalized_member;
                            }
                        }
                    }
                }

                let widened_members: Vec<TypeId> = members_to_widen
                    .iter()
                    .map(|&m| {
                        widen_type_cached(
                            db,
                            m,
                            cache,
                            widen_boolean_intrinsics,
                            widen_functions,
                            widen_object_union_members,
                        )
                    })
                    .collect();
                let widened = db.union(widened_members);
                propagate_display_alias(db, type_id, widened);
                widened
            } else {
                type_id
            }
        }

        // Objects: recursively widen properties (critical for mutable variables)
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let mut new_props = Vec::with_capacity(shape.properties.len());
            let mut changed = false;
            let strip_fresh_display =
                widen_object_union_members && shape.flags.contains(ObjectFlags::FRESH_LITERAL);

            for prop in &shape.properties {
                // Rule: Readonly properties are NOT widened
                let widened_type = if prop.readonly {
                    prop.type_id
                } else {
                    widen_type_cached(
                        db,
                        prop.type_id,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                    )
                };

                // Write type follows read type logic
                let widened_write_type = if prop.readonly {
                    prop.write_type
                } else {
                    widen_type_cached(
                        db,
                        prop.write_type,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                    )
                };

                if widened_type != prop.type_id || widened_write_type != prop.write_type {
                    changed = true;
                }
                let mut new_prop = prop.clone();
                new_prop.type_id = widened_type;
                new_prop.write_type = widened_write_type;
                new_props.push(new_prop);
            }

            if changed || strip_fresh_display {
                let mut flags = shape.flags;
                if strip_fresh_display {
                    flags.remove(ObjectFlags::FRESH_LITERAL);
                }

                let widened_type_id =
                    if shape.string_index.is_some() || shape.number_index.is_some() {
                        let mut new_shape = (*shape).clone();
                        new_shape.properties = new_props;
                        new_shape.flags = flags;
                        db.object_with_index(new_shape)
                    } else {
                        // Preserve symbol and flags so named types (interfaces,
                        // classes) retain their identity through widening.
                        db.object_with_flags_and_symbol(new_props, flags, shape.symbol)
                    };

                // Carry forward display properties from the original TypeId.
                if let Some(display_props) = db.get_display_properties(type_id) {
                    db.store_display_properties(widened_type_id, display_props.as_ref().clone());
                }
                propagate_display_alias(db, type_id, widened_type_id);

                widened_type_id
            } else {
                type_id
            }
        }

        // Arrays: recursively widen element type
        Some(TypeData::Array(element_type)) => {
            let widened = widen_type_cached(
                db,
                element_type,
                cache,
                widen_boolean_intrinsics,
                widen_functions,
                widen_object_union_members,
            );
            if widened != element_type {
                let widened_arr = db.array(widened);
                propagate_display_alias(db, type_id, widened_arr);
                widened_arr
            } else {
                type_id
            }
        }

        // Tuples: recursively widen element types
        Some(TypeData::Tuple(tuple_list_id)) => {
            let elements = db.tuple_list(tuple_list_id);
            let mut new_elements = Vec::with_capacity(elements.len());
            let mut changed = false;
            for elem in elements.iter() {
                let widened = widen_type_cached(
                    db,
                    elem.type_id,
                    cache,
                    widen_boolean_intrinsics,
                    widen_functions,
                    widen_object_union_members,
                );
                if widened != elem.type_id {
                    changed = true;
                }
                let mut new_elem = *elem;
                new_elem.type_id = widened;
                new_elements.push(new_elem);
            }
            if changed {
                let widened_tuple = db.tuple(new_elements);
                propagate_display_alias(db, type_id, widened_tuple);
                widened_tuple
            } else {
                type_id
            }
        }

        // Functions: recursively widen parameter and return types for display contexts.
        Some(TypeData::Function(shape_id)) if widen_functions => {
            let shape = db.function_shape(shape_id);
            let mut widened_shape = shape.as_ref().clone();
            let mut changed = false;
            widened_shape.params = widened_shape
                .params
                .iter()
                .map(|param| {
                    let mut widened = *param;
                    widened.type_id = widen_type_cached(
                        db,
                        param.type_id,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                    );
                    if widened.type_id != param.type_id {
                        changed = true;
                    }
                    widened
                })
                .collect();
            widened_shape.this_type = widened_shape.this_type.map(|this_ty| {
                let widened = widen_type_cached(
                    db,
                    this_ty,
                    cache,
                    widen_boolean_intrinsics,
                    widen_functions,
                    widen_object_union_members,
                );
                if widened != this_ty {
                    changed = true;
                }
                widened
            });
            let widened_return = widen_type_cached(
                db,
                widened_shape.return_type,
                cache,
                widen_boolean_intrinsics,
                widen_functions,
                widen_object_union_members,
            );
            if widened_return != widened_shape.return_type {
                changed = true;
            }
            widened_shape.return_type = widened_return;

            if changed {
                let widened_fn = db.function(widened_shape);
                propagate_display_alias(db, type_id, widened_fn);
                widened_fn
            } else {
                type_id
            }
        }

        // Callable objects: recursively widen all signature parameter/return types.
        Some(TypeData::Callable(shape_id)) if widen_functions => {
            let shape = db.callable_shape(shape_id);
            let mut widened_shape = shape.as_ref().clone();
            let mut changed = false;
            widened_shape.call_signatures = widened_shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let mut widened_sig = sig.clone();
                    widened_sig.params = widened_sig
                        .params
                        .iter()
                        .map(|param| {
                            let mut widened = *param;
                            widened.type_id = widen_type_cached(
                                db,
                                param.type_id,
                                cache,
                                widen_boolean_intrinsics,
                                widen_functions,
                                widen_object_union_members,
                            );
                            if widened.type_id != param.type_id {
                                changed = true;
                            }
                            widened
                        })
                        .collect();
                    widened_sig.this_type = widened_sig.this_type.map(|this_ty| {
                        let widened = widen_type_cached(
                            db,
                            this_ty,
                            cache,
                            widen_boolean_intrinsics,
                            widen_functions,
                            widen_object_union_members,
                        );
                        if widened != this_ty {
                            changed = true;
                        }
                        widened
                    });
                    let widened_return = widen_type_cached(
                        db,
                        widened_sig.return_type,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                    );
                    if widened_return != widened_sig.return_type {
                        changed = true;
                    }
                    widened_sig.return_type = widened_return;
                    widened_sig
                })
                .collect();

            if changed {
                let widened_callable = db.callable(widened_shape);
                propagate_display_alias(db, type_id, widened_callable);
                widened_callable
            } else {
                type_id
            }
        }

        // All other types (including Function/Callable when widen_functions is false)
        // are returned as-is. When widen_functions is false, tsc's getInferredType
        // does NOT recurse into function parameter types during deep-widening.
        // Widening function params changes contravariant positions
        // (e.g., `(x: 1 | 2) => void` → `(x: number) => void`), causing false TS2322.
        _ => type_id,
    };

    cache.insert(type_id, result);
    result
}

/// Widen only object literal property types (not top-level types or union members).
///
/// This is used during inference resolution to match TypeScript's behavior:
/// when an object literal like `{ c: false }` is inferred against a bare type
/// parameter `T`, the property literal types are widened (`{ c: boolean }`).
/// However, top-level union types like `"foo" | "bar"` must NOT be widened
/// (they should stay as literal unions for type parameter inference).
///
/// This differs from `widen_type` which recursively widens everything including
/// union members and direct literals. This function only enters objects/arrays/tuples.
pub(crate) fn widen_object_literal_properties(
    db: &dyn crate::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    match db.lookup(type_id) {
        // Objects: recursively widen mutable property types
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let mut new_props = Vec::with_capacity(shape.properties.len());
            let mut changed = false;

            for prop in &shape.properties {
                let widened_type = if prop.readonly {
                    prop.type_id
                } else {
                    widen_type(db, prop.type_id)
                };
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
                if shape.string_index.is_some() || shape.number_index.is_some() {
                    let mut new_shape = (*shape).clone();
                    new_shape.properties = new_props;
                    db.object_with_index(new_shape)
                } else {
                    // Preserve symbol and flags so named types retain identity.
                    db.object_with_flags_and_symbol(new_props, shape.flags, shape.symbol)
                }
            } else {
                type_id
            }
        }

        // All other types pass through unchanged — particularly unions of
        // string/number literals must NOT be widened here.
        _ => type_id,
    }
}

/// Get the base type of a literal type for comparison operators.
///
/// Matches TypeScript's `getBaseTypeOfLiteralTypeForComparison`:
/// - String literals, template literals, string intrinsics → `string`
/// - Number literals → `number`
/// - `BigInt` literals → `bigint`
/// - Boolean literals → `boolean`
/// - Enum types → recursively widen their member union
/// - Union types → recursively map each member
/// - Everything else → unchanged
///
/// Used by relational operators (`<`, `>`, `<=`, `>=`) to normalize types
/// before comparability checks. This is distinct from general widening because
/// it also handles enum types and template literals.
pub fn get_base_type_for_comparison(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        // String/Number/Boolean/BigInt literals widen to their primitives
        Some(TypeData::Literal(ref value)) => value.primitive_type_id(),

        // Enum types: recursively widen their member union
        // (numeric enums → number, string enums → string)
        Some(TypeData::Enum(_, member_type_id)) => get_base_type_for_comparison(db, member_type_id),

        // Template literals and string intrinsics (Uppercase<T>, etc.) → string
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. }) => TypeId::STRING,

        // Type parameters: resolve through constraint to determine comparison family.
        // This ensures that e.g. `T extends "a" | "b"` has comparison base `string`,
        // matching the base of literal `"x"`, so the TS2367 display preserves
        // literal detail for same-family comparisons (tsc shows `T` and `"x"`,
        // not `T` and `string`).
        Some(TypeData::TypeParameter(ref info)) => {
            if let Some(constraint) = info.constraint {
                get_base_type_for_comparison(db, constraint)
            } else {
                type_id
            }
        }

        // Unions: recursively map all members
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members
                .iter()
                .map(|&m| get_base_type_for_comparison(db, m))
                .collect();
            db.union(mapped)
        }

        // Everything else unchanged
        _ => type_id,
    }
}

/// Widen only literal types to their base primitive types.
///
/// This is more targeted than `get_base_type_for_comparison`:
/// - String/Number/Boolean/BigInt literals → their primitive types
/// - Unions → recursively map members
/// - Everything else (including enums, template literals) → unchanged
///
/// Used for binary operator error messages where tsc shows widened types
/// for literal operands but preserves enum type names.
pub fn widen_literal_type(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return TypeId::BOOLEAN;
    }

    match db.lookup(type_id) {
        Some(TypeData::Literal(ref value)) => value.primitive_type_id(),

        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| widen_literal_type(db, m)).collect();
            db.union(mapped)
        }

        _ => type_id,
    }
}

/// Widen number and boolean literal types but preserve string and bigint literals.
///
/// tsc's TS2367 diagnostic uses widened types for number/boolean operands
/// (e.g., `true` → `boolean`, `0` → `number`) but preserves string/bigint
/// literal types in the message text.
#[allow(dead_code)] // Reserved for TS2367 diagnostic message formatting
pub(crate) fn widen_non_string_bigint_literal(
    db: &dyn crate::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::Literal(ref value)) => match value {
            crate::LiteralValue::Number(_) => TypeId::NUMBER,
            crate::LiteralValue::Boolean(_) => TypeId::BOOLEAN,
            crate::LiteralValue::String(_) | crate::LiteralValue::BigInt(_) => type_id,
        },
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
/// ```text
/// use crate::operations::widening::apply_const_assertion;
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
#[path = "../../tests/widening_tests.rs"]
mod tests;
