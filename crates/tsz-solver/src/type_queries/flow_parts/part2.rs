/// Whether `type_id` is a template-literal type or a string-intrinsic mapping
/// type (`Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize`). Both are
/// subtypes of `string` whose membership is pattern-defined.
fn is_template_or_string_intrinsic(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. })
    )
}

/// Whether `type_id` belongs to the `string` domain for assertion overlap:
/// the `string` primitive, a string-literal type, or a pattern-defined string
/// subtype (template-literal / string-intrinsic). A literal source widens to
/// its base primitive at the assertion site, so each of these overlaps a
/// template-literal target.
fn is_string_domain_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::STRING
        || matches!(
            db.lookup(type_id),
            Some(
                TypeData::Literal(crate::types::LiteralValue::String(_))
                    | TypeData::TemplateLiteral(_)
                    | TypeData::StringIntrinsic { .. }
            )
        )
}

/// Check if a base primitive type is comparable to a literal or other form of that primitive.
fn is_primitive_comparable(db: &dyn TypeDatabase, base: TypeId, other: TypeId) -> bool {
    // Decompose union types: a union is primitive-comparable if any member is.
    // This is needed for enum structural types which are stored as unions of
    // member literals (e.g., `"" | "time" | "system" | "location"`).
    if let Some(TypeData::Union(list_id)) = db.lookup(base) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| is_primitive_comparable(db, m, other));
    }
    if let Some(TypeData::Union(list_id)) = db.lookup(other) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| is_primitive_comparable(db, base, m));
    }
    // string is comparable to string literals
    if base == TypeId::STRING {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::String(_));
        }
        return other == TypeId::STRING;
    }
    // number is comparable to numeric literals
    if base == TypeId::NUMBER {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::Number(_));
        }
        return other == TypeId::NUMBER;
    }
    // boolean is comparable to true/false
    if base == TypeId::BOOLEAN {
        return other == TypeId::BOOLEAN_TRUE
            || other == TypeId::BOOLEAN_FALSE
            || other == TypeId::BOOLEAN;
    }
    // bigint is comparable to bigint literals
    if base == TypeId::BIGINT {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::BigInt(_));
        }
        return other == TypeId::BIGINT;
    }
    // symbol is comparable to unique symbol (unique symbol is a subtype of symbol)
    if base == TypeId::SYMBOL {
        return matches!(db.lookup(other), Some(TypeData::UniqueSymbol(_)))
            || other == TypeId::SYMBOL;
    }
    // unique symbol is comparable to symbol and to other unique symbols
    if let Some(TypeData::UniqueSymbol(_)) = db.lookup(base) {
        return other == TypeId::SYMBOL
            || matches!(db.lookup(other), Some(TypeData::UniqueSymbol(_)));
    }
    // Two literals of the same primitive kind are broadly comparable. Assertion
    // property overlap applies an additional value-level guard for shared
    // discriminant/phantom properties before reaching this helper.
    if let Some(TypeData::Literal(lit_a)) = db.lookup(base) {
        if let Some(TypeData::Literal(lit_b)) = db.lookup(other) {
            return std::mem::discriminant(&lit_a) == std::mem::discriminant(&lit_b);
        }
        // literal vs its base primitive: "foo" ~ string, 1 ~ number
        return match lit_a {
            crate::types::LiteralValue::String(_) => other == TypeId::STRING,
            crate::types::LiteralValue::Number(_) => other == TypeId::NUMBER,
            crate::types::LiteralValue::BigInt(_) => other == TypeId::BIGINT,
            crate::types::LiteralValue::Boolean(_) => {
                other == TypeId::BOOLEAN
                    || other == TypeId::BOOLEAN_TRUE
                    || other == TypeId::BOOLEAN_FALSE
            }
        };
    }
    // true/false are comparable to each other
    if (base == TypeId::BOOLEAN_TRUE || base == TypeId::BOOLEAN_FALSE)
        && (other == TypeId::BOOLEAN_TRUE || other == TypeId::BOOLEAN_FALSE)
    {
        return true;
    }
    // Enum members are comparable via their underlying structural (literal) type.
    // E.g., `AutomationMode.NONE` (Enum(_, "")) is comparable to `""` and to `string`.
    if let Some(TypeData::Enum(_, structural)) = db.lookup(base) {
        return is_primitive_comparable(db, structural, other)
            || is_primitive_comparable(db, other, structural);
    }
    if let Some(TypeData::Enum(_, structural)) = db.lookup(other) {
        return is_primitive_comparable(db, base, structural)
            || is_primitive_comparable(db, structural, base);
    }
    false
}

/// Check if two types have common properties with ALL of them having comparable types.
///
/// Returns true when the types share at least one property name AND every shared
/// property has comparable types. This matches tsc's behavior for the comparable
/// relation on object types — a single incompatible shared property means the
/// types are NOT comparable, even if other properties match.
fn types_have_common_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    // Helper to get properties from an object/callable type.
    // Returns (name, type_id, optional) — the optional flag is needed because
    // optional properties implicitly include `undefined` for comparability.
    fn get_properties(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<(Atom, TypeId, bool)> {
        if type_id.is_intrinsic() {
            return Vec::new();
        }
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
                    .collect()
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = db.callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
                    .collect()
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = db.type_list(list_id);
                let mut props = Vec::new();
                for &member in members.iter() {
                    props.extend(get_properties(db, member));
                }
                props
            }
            // Arrays have no named properties for overlap checking - element types
            // are compared separately in types_are_comparable_for_assertion_inner.
            // Returning empty ensures we don't short-circuit array↔object comparisons.
            _ => Vec::new(),
        }
    }

    // Handle array↔array comparability: check element types directly
    if let (Some(TypeData::Array(src_elem)), Some(TypeData::Array(tgt_elem))) =
        (db.lookup(source), db.lookup(target))
    {
        return types_are_comparable_for_assertion_inner(db, src_elem, tgt_elem, depth + 1, false);
    }

    // Handle array↔tuple comparability: array element vs any tuple element
    if let (Some(TypeData::Array(arr_elem)), Some(TypeData::Tuple(tuple_id))) =
        (db.lookup(source), db.lookup(target))
    {
        let tuple_elements = db.tuple_list(tuple_id);
        return tuple_elements.iter().any(|elem| {
            types_are_comparable_for_assertion_inner(db, arr_elem, elem.type_id, depth + 1, false)
        });
    }
    if let (Some(TypeData::Tuple(tuple_id)), Some(TypeData::Array(arr_elem))) =
        (db.lookup(source), db.lookup(target))
    {
        let tuple_elements = db.tuple_list(tuple_id);
        return tuple_elements.iter().any(|elem| {
            types_are_comparable_for_assertion_inner(db, elem.type_id, arr_elem, depth + 1, false)
        });
    }

    // Handle tuple↔tuple comparability: check element types pairwise.
    // tsc's isTypeComparableTo checks tuples structurally: each element at
    // position i must be comparable to the element at position i in the other
    // tuple. Different-length tuples are not comparable (neither is assignable
    // to the other), so TS2352 should fire.
    if let (Some(TypeData::Tuple(src_tuple)), Some(TypeData::Tuple(tgt_tuple))) =
        (db.lookup(source), db.lookup(target))
    {
        let src_elements = db.tuple_list(src_tuple);
        let tgt_elements = db.tuple_list(tgt_tuple);
        // Different-length tuples are not comparable
        if src_elements.len() != tgt_elements.len() {
            return false;
        }
        // All corresponding elements must be comparable
        return src_elements.iter().zip(tgt_elements.iter()).all(|(s, t)| {
            types_are_comparable_for_assertion_inner(db, s.type_id, t.type_id, depth + 1, false)
        });
    }

    let source_props = get_properties(db, source);
    let target_props = get_properties(db, target);

    // If both sides have no properties and aren't arrays/tuples, they don't overlap
    if source_props.is_empty() && target_props.is_empty() {
        return false;
    }

    // Build a lookup table for source properties by name.
    use rustc_hash::FxHashMap;
    let mut source_by_name: FxHashMap<Atom, Vec<(TypeId, bool)>> = FxHashMap::default();
    for (name, ty, optional) in &source_props {
        source_by_name
            .entry(*name)
            .or_default()
            .push((*ty, *optional));
    }

    // tsc's comparable relation requires ALL required target properties to
    // exist in the source with comparable types. Just sharing some common
    // property names is not enough — missing required target properties means
    // the types are NOT comparable.
    let mut found_common = false;
    for (target_name, target_ty, target_optional) in &target_props {
        if let Some(source_entries) = source_by_name.get(target_name) {
            found_common = true;
            let any_comparable = source_entries.iter().any(|(source_ty, source_optional)| {
                // If either property is optional, `undefined` is part of the type.
                // E.g., `a?: string` effectively has type `string | undefined`,
                // so `undefined` is comparable to it.
                if (*source_optional || *target_optional)
                    && (*source_ty == TypeId::UNDEFINED || *target_ty == TypeId::UNDEFINED)
                {
                    return true;
                }
                types_are_comparable_inner(db, *source_ty, *target_ty, depth + 1)
            });
            if !any_comparable {
                return false;
            }
        } else if !target_optional {
            // Required target property is missing from source — not comparable.
            return false;
        }
    }
    found_common
}

/// Check if a type contains a `TypeQuery` referencing a specific symbol.
///
/// Used for TS2502 detection (circular reference in type annotation).
/// Traverses the type structure, expanding top-level lazy aliases via the provided callback.
/// Stops recursion at Function, Object, and Mapped types which break the "direct" reference cycle.
pub fn has_type_query_for_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    target_sym_id: u32,
    mut resolve_lazy: impl FnMut(TypeId) -> TypeId,
) -> bool {
    with_flow_visited(|visited| {
        let mut worklist = vec![type_id];
        while let Some(ty) = worklist.pop() {
            if !visited.insert(ty) {
                continue;
            }

            if ty.is_intrinsic() {
                continue;
            }

            let resolved = resolve_lazy(ty);
            if resolved != ty {
                worklist.push(resolved);
                continue;
            }

            let Some(key) = db.lookup(ty) else { continue };
            match key {
                TypeData::TypeQuery(sym_ref) if sym_ref.0 == target_sym_id => {
                    return true;
                }
                TypeData::Array(elem) => worklist.push(elem),
                TypeData::Union(list) | TypeData::Intersection(list) => {
                    let members = db.type_list(list);
                    worklist.extend(members.iter().copied());
                }
                TypeData::Tuple(list) => {
                    let elements = db.tuple_list(list);
                    for elem in elements.iter() {
                        worklist.push(elem.type_id);
                    }
                }
                TypeData::Conditional(id) => {
                    let cond = db.conditional_type(id);
                    worklist.push(cond.check_type);
                    worklist.push(cond.extends_type);
                    worklist.push(cond.true_type);
                    worklist.push(cond.false_type);
                }
                TypeData::Application(id) => {
                    let app = db.type_application(id);
                    worklist.push(app.base);
                    worklist.extend(&app.args);
                }
                TypeData::IndexAccess(obj, idx) => {
                    worklist.push(obj);
                    worklist.push(idx);
                }
                TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) => {
                    worklist.push(inner);
                }
                _ => {
                    // `Function`, `Object`, `ObjectWithIndex`, and `Mapped` intentionally stop
                    // traversal here: they break the "direct" reference cycle check for TS2502,
                    // because recursive types via function return/params or object properties
                    // are allowed.
                }
            }
        }
        false
    })
}

/// Extract contextual type parameters from a type.
///
/// Inspects function shapes, callable shapes (single call signature),
/// type applications (recurse into base), and unions (all members must agree).
/// Returns `None` if the type has no extractable type parameters or if
/// union members disagree.
///
/// This encapsulates the common checker pattern of extracting type parameters
/// from an expected contextual type for generic function inference.
pub fn extract_contextual_type_params(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::types::TypeParamInfo>> {
    extract_contextual_type_params_inner(db, type_id, 0)
}

fn extract_contextual_type_params_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: u32,
) -> Option<Vec<crate::types::TypeParamInfo>> {
    if depth > 20 {
        return None;
    }
    if type_id.is_intrinsic() {
        return None;
    }

    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            if shape.type_params.is_empty() {
                None
            } else {
                Some(shape.type_params.clone())
            }
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if shape.call_signatures.len() != 1 {
                return None;
            }
            let sig = &shape.call_signatures[0];
            if sig.type_params.is_empty() {
                None
            } else {
                Some(sig.type_params.clone())
            }
        }
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            extract_contextual_type_params_inner(db, app.base, depth + 1)
        }
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            if members.is_empty() {
                return None;
            }
            let mut candidate: Option<Vec<crate::types::TypeParamInfo>> = None;
            for &member in members.iter() {
                let params = extract_contextual_type_params_inner(db, member, depth + 1)?;
                if let Some(existing) = &candidate {
                    if existing.len() != params.len()
                        || existing
                            .iter()
                            .zip(params.iter())
                            .any(|(left, right)| left != right)
                    {
                        return None;
                    }
                } else {
                    candidate = Some(params);
                }
            }
            candidate
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::types::TupleElement;

    #[test]
    fn tuple_to_tuple_comparable_same_elements() {
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert!(types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn tuple_to_tuple_comparable_with_never() {
        // [undefined, string] vs [never, string] — should be comparable
        // because never is comparable to everything
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::UNDEFINED,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NEVER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert!(types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn tuple_to_tuple_incomparable_different_lengths() {
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }]);
        assert!(!types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn tuple_to_tuple_incomparable_different_elements() {
        // [number, string] vs [boolean, boolean] — not comparable
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::BOOLEAN,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::BOOLEAN,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert!(!types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn never_comparable_to_any_type() {
        let interner = TypeInterner::new();
        assert!(types_are_comparable(
            &interner,
            TypeId::NEVER,
            TypeId::STRING
        ));
        assert!(types_are_comparable(
            &interner,
            TypeId::NEVER,
            TypeId::NUMBER
        ));
        assert!(types_are_comparable(
            &interner,
            TypeId::STRING,
            TypeId::NEVER
        ));
    }

    #[test]
    fn any_comparable_to_any_type() {
        let interner = TypeInterner::new();
        assert!(types_are_comparable(&interner, TypeId::ANY, TypeId::STRING));
        assert!(types_are_comparable(&interner, TypeId::ANY, TypeId::NUMBER));
        assert!(types_are_comparable(&interner, TypeId::STRING, TypeId::ANY));
    }

    #[test]
    fn unknown_comparable_to_any_type() {
        let interner = TypeInterner::new();
        assert!(types_are_comparable(
            &interner,
            TypeId::UNKNOWN,
            TypeId::STRING
        ));
        assert!(types_are_comparable(
            &interner,
            TypeId::STRING,
            TypeId::UNKNOWN
        ));
    }

    #[test]
    fn test_extract_predicate_signature_function() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{FunctionShape, ParamInfo, TypePredicate, TypePredicateTarget};

        // Function with type predicate
        let fn_with_pred = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(interner.intern_string("x")),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        });

        let sig = super::extract_predicate_signature(&interner, fn_with_pred);
        assert!(sig.is_some());
        let sig = sig.unwrap();
        assert_eq!(sig.predicate.type_id, Some(TypeId::STRING));
        assert_eq!(sig.params.len(), 1);

        // Function without predicate → None
        let fn_no_pred = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        assert!(super::extract_predicate_signature(&interner, fn_no_pred).is_none());

        // Non-function type → None
        assert!(super::extract_predicate_signature(&interner, TypeId::STRING).is_none());
    }

    /// Verify that when object property types are Lazy (unresolved), the
    /// solver's comparable check correctly returns false (not comparable),
    /// because Lazy types have no extractable properties for structural
    /// comparison.  The CHECKER is responsible for resolving Lazy types
    /// before calling this function (via `deep_evaluate_object_properties`).
    #[test]
    fn assertion_comparable_object_with_lazy_property_not_resolved_by_solver() {
        use crate::def::DefId;
        use crate::types::{PropertyInfo, Visibility};

        let db = TypeInterner::new();

        let mode_name = db.intern_string("mode");
        let source = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }]);

        // Target has Lazy property type — solver cannot resolve it
        let lazy_ref = db.lazy(DefId(9999));
        let target = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: lazy_ref,
            write_type: lazy_ref,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }]);

        // Solver returns false because Lazy types are opaque here.
        // The checker resolves Lazy types before calling this function.
        assert!(
            !types_are_comparable_for_assertion(&db, source, target),
            "Unresolved Lazy property should not be comparable at solver level"
        );
    }

    /// When property types are both concrete (no Lazy), objects with a
    /// matching property whose types are comparable should be comparable.
    #[test]
    fn assertion_comparable_objects_with_resolved_enum_property() {
        use crate::def::DefId;
        use crate::types::{PropertyInfo, Visibility};

        let db = TypeInterner::new();

        let mode_name = db.intern_string("mode");
        // Source: { mode: string }
        let source = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }]);

        // Target: { mode: AutomationMode } (enum with string members)
        let structural_union = db.union(vec![
            db.literal_string(""),
            db.literal_string("time"),
            db.literal_string("system"),
        ]);
        let enum_type = db.enum_type(DefId(8888), structural_union);
        let target = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: enum_type,
            write_type: enum_type,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }]);

        // When both sides are resolved, the comparable check succeeds
        // because string is comparable to a string enum.
        assert!(
            types_are_comparable_for_assertion(&db, source, target),
            "Object with string property should be comparable to object with string enum property"
        );
    }

    /// `instance_type_from_constructor` returns the predicate type of
    /// `[Symbol.hasInstance]` (overriding construct signature returns).
    ///
    /// This locks in tsc parity for `interface T { new (): A; [Symbol.hasInstance](v: unknown): value is B; }` —
    /// the predicate type `B` defines the instance type, NOT the construct
    /// signature return `A`. Variable name is verified with two iteration
    /// names (P, K) in `instance_type_from_symbol_has_instance_predicate_works_for_any_value_name`.
    #[test]
    fn instance_type_from_constructor_uses_symbol_has_instance_predicate() {
        use crate::types::{
            CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        // [Symbol.hasInstance](value: unknown): value is STRING
        let has_instance_fn = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        });

        // Constructor: { new (): NUMBER; [Symbol.hasInstance](value: unknown): value is STRING }
        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature::new(vec![], TypeId::NUMBER)],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: has_instance_fn,
                write_type: has_instance_fn,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        let result = super::instance_type_from_constructor(&db, constructor);
        assert_eq!(
            result,
            Some(TypeId::STRING),
            "Predicate type STRING must override construct sig return NUMBER"
        );
    }

    #[test]
    fn instance_type_from_constructor_erases_generic_construct_return_to_any() {
        use crate::def::DefId;
        use crate::types::{CallSignature, CallableShape, TypeParamInfo};

        let db = crate::intern::TypeInterner::new();
        let t_name = db.intern_string("T");
        let t_info = TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        };
        let t_type = db.type_param(t_info);
        let box_base = db.lazy(DefId(4242));
        let box_t = db.application(box_base, vec![t_type]);
        let box_any = db.application(box_base, vec![TypeId::ANY]);

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature {
                type_params: vec![t_info],
                params: vec![],
                this_type: None,
                return_type: box_t,
                type_predicate: None,
                is_method: false,
            }],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_constructor(&db, constructor),
            Some(box_any),
            "generic construct signatures must produce their erased instance type for instanceof"
        );
    }

    #[test]
    fn instance_type_from_symbol_has_instance_erases_generic_predicate_to_any() {
        use crate::def::DefId;
        use crate::types::{
            CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypeParamInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let t_name = db.intern_string("T");
        let t_info = TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        };
        let t_type = db.type_param(t_info);
        let box_base = db.lazy(DefId(4243));
        let box_t = db.application(box_base, vec![t_type]);
        let box_any = db.application(box_base, vec![TypeId::ANY]);
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        let has_instance_fn = db.function(FunctionShape {
            type_params: vec![t_info],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(box_t),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: true,
        });

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: has_instance_fn,
                write_type: has_instance_fn,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_constructor(&db, constructor),
            Some(box_any),
            "generic Symbol.hasInstance predicates must erase their own type parameters to any"
        );
    }

    #[test]
    fn instance_type_from_constructor_uses_generic_construct_when_predicate_collapses_to_any() {
        use crate::def::DefId;
        use crate::types::{
            CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypeParamInfo,
            TypePredicate, TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let t_name = db.intern_string("T");
        let t_info = TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        };
        let t_type = db.type_param(t_info);
        let box_base = db.lazy(DefId(4244));
        let box_t = db.application(box_base, vec![t_type]);
        let box_any = db.application(box_base, vec![TypeId::ANY]);
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        let has_instance_fn = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(TypeId::ANY),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: true,
        });

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature {
                type_params: vec![t_info],
                params: vec![],
                this_type: None,
                return_type: box_t,
                type_predicate: None,
                is_method: false,
            }],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: has_instance_fn,
                write_type: has_instance_fn,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_constructor(&db, constructor),
            Some(box_any),
            "a collapsed any predicate should not hide the concrete erased generic construct candidate"
        );
    }

    /// `instance_type_from_symbol_has_instance` does not depend on the
    /// user-chosen parameter name — `value` and `x` give identical results.
    /// Locks in §25 of `.claude/CLAUDE.md` (no hardcoded user-chosen names).
    #[test]
    fn instance_type_from_symbol_has_instance_predicate_works_for_any_value_name() {
        use crate::types::{
            CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        for &param_name in &["value", "x"] {
            let db = crate::intern::TypeInterner::new();
            let name_atom = db.intern_string(param_name);
            let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

            let fn_id = db.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(name_atom),
                    type_id: TypeId::UNKNOWN,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: Some(TypePredicate {
                    asserts: false,
                    target: TypePredicateTarget::Identifier(name_atom),
                    type_id: Some(TypeId::NUMBER),
                    parameter_index: Some(0),
                }),
                is_constructor: false,
                is_method: false,
            });

            let constructor = db.callable(CallableShape {
                call_signatures: vec![],
                construct_signatures: vec![],
                properties: vec![PropertyInfo {
                    name: has_instance_atom,
                    type_id: fn_id,
                    write_type: fn_id,
                    optional: false,
                    readonly: false,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                }],
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            });

            assert_eq!(
                super::instance_type_from_symbol_has_instance(&db, constructor),
                Some(TypeId::NUMBER),
                "Predicate type must be parameter-name-independent (got param={param_name})"
            );
        }
    }

    /// `asserts value is T` does NOT carry through to instanceof narrowing —
    /// tsc only uses non-asserting predicates for the instanceof candidate.
    #[test]
    fn instance_type_from_symbol_has_instance_ignores_asserts_predicate() {
        use crate::types::{
            CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        let fn_id = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: true,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        });

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: fn_id,
                write_type: fn_id,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_symbol_has_instance(&db, constructor),
            None,
            "asserts predicates must not be used for instanceof narrowing"
        );
    }

    /// Two distinct string literals remain broadly primitive-comparable. The
    /// stricter value-level rule is applied by assertion property overlap, not
    /// by this shared primitive helper.
    #[test]
    fn distinct_string_literals_are_primitive_comparable() {
        let db = TypeInterner::new();
        let lit_draft = db.literal_string("draft");
        let lit_published = db.literal_string("published");
        assert!(
            is_primitive_comparable(&db, lit_draft, lit_published),
            "\"draft\" must remain primitive-comparable to \"published\""
        );
        assert!(
            is_primitive_comparable(&db, lit_published, lit_draft),
            "\"published\" must remain primitive-comparable to \"draft\""
        );
    }

    /// Two identical string literals must be primitive-comparable (same value).
    #[test]
    fn same_string_literal_is_comparable() {
        let db = TypeInterner::new();
        let lit_a = db.literal_string("draft");
        let lit_b = db.literal_string("draft");
        assert!(
            is_primitive_comparable(&db, lit_a, lit_b),
            "\"draft\" must be primitive-comparable to \"draft\""
        );
    }

    /// A string literal must be primitive-comparable to its base primitive.
    #[test]
    fn string_literal_comparable_to_string_primitive() {
        let db = TypeInterner::new();
        let lit = db.literal_string("hello");
        assert!(
            is_primitive_comparable(&db, lit, TypeId::STRING),
            "\"hello\" must be primitive-comparable to `string`"
        );
        assert!(
            is_primitive_comparable(&db, TypeId::STRING, lit),
            "`string` must be primitive-comparable to \"hello\""
        );
    }

    /// Two distinct number literals remain broadly primitive-comparable.
    #[test]
    fn distinct_number_literals_are_primitive_comparable() {
        let db = TypeInterner::new();
        let lit_200 = db.literal_number(200.0);
        let lit_404 = db.literal_number(404.0);
        assert!(
            is_primitive_comparable(&db, lit_200, lit_404),
            "200 must remain primitive-comparable to 404"
        );
    }

    /// Verify that enum structural union types are comparable to their
    /// base primitive type via `is_primitive_comparable` union decomposition.
    #[test]
    fn enum_structural_union_comparable_to_base_primitive() {
        use crate::def::DefId;

        let db = TypeInterner::new();

        // Create enum structural type: "" | "time" | "system"
        let lit_empty = db.literal_string("");
        let lit_time = db.literal_string("time");
        let lit_system = db.literal_string("system");
        let structural_union = db.union(vec![lit_empty, lit_time, lit_system]);

        // Create the enum type
        let enum_type = db.enum_type(DefId(8888), structural_union);

        // string should be comparable to the enum
        assert!(
            is_primitive_comparable(&db, TypeId::STRING, enum_type)
                || is_primitive_comparable(&db, enum_type, TypeId::STRING),
            "string should be primitive-comparable to a string enum"
        );

        // A string literal should also be comparable to the enum
        assert!(
            is_primitive_comparable(&db, lit_empty, enum_type)
                || is_primitive_comparable(&db, enum_type, lit_empty),
            "string literal should be primitive-comparable to a string enum containing it"
        );
    }
}
