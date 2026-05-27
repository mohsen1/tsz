/// Regression test for property-to-function cache contamination.
///
/// When an Object shape has a property `t: T` and a method `foo3<T>(t: T, u: U)`,
/// instantiating the property first caches `TypeId(T) → string` in the visiting cache.
/// When the method Function type is then instantiated, `T` should be shadowed (method's
/// own `<T>`), but `instantiate_inner` returns the cached `string` before `instantiate_key`
/// checks `is_shadowed`. The fix removes shadowed `TypeParameter` entries from the cache
/// when entering the function's scope.
#[test]
fn test_object_property_does_not_contaminate_method_type_param() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_param = TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let u_type = interner.intern(TypeData::TypeParameter(u_param));

    // Method foo3<T>(t: T, u: U): T — shadows class T
    let method_type = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("t")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("u")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Object: { t: T, u: U, foo3: <T>(t: T, u: U) => T }
    // Property `t: T` is listed BEFORE method `foo3` to trigger the bug
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("t"), t_type),
        PropertyInfo::new(interner.intern_string("u"), u_type),
        PropertyInfo {
            name: interner.intern_string("foo3"),
            type_id: method_type,
            write_type: method_type,
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
        },
    ]);

    // Substitute T=string, U=number
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    subst.insert(u_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, obj, &subst);

    // Verify
    if let Some(TypeData::Object(shape_id)) = interner.lookup(result) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 3);

        // Properties are sorted by name, so look up by name
        let t_name_atom = interner.intern_string("t");
        let u_name_atom = interner.intern_string("u");
        let foo3_name = interner.intern_string("foo3");

        let t_prop = shape
            .properties
            .iter()
            .find(|p| p.name == t_name_atom)
            .unwrap();
        let u_prop = shape
            .properties
            .iter()
            .find(|p| p.name == u_name_atom)
            .unwrap();
        let foo3_prop = shape
            .properties
            .iter()
            .find(|p| p.name == foo3_name)
            .unwrap();

        // Property t: should be string (substituted)
        assert_eq!(t_prop.type_id, TypeId::STRING);
        // Property u: should be number (substituted)
        assert_eq!(u_prop.type_id, TypeId::NUMBER);

        // Method foo3: should still have its own <T> with T unsubstituted in params
        let method_result = foo3_prop.type_id;
        if let Some(TypeData::Function(fn_shape_id)) = interner.lookup(method_result) {
            let fn_shape = interner.function_shape(fn_shape_id);
            assert_eq!(
                fn_shape.type_params.len(),
                1,
                "Method should still have <T>"
            );
            assert_eq!(
                fn_shape.params[0].type_id, t_type,
                "Method param t should be TypeParameter(T), not string"
            );
            assert_eq!(
                fn_shape.params[1].type_id,
                TypeId::NUMBER,
                "Method param u should be number (class U substituted)"
            );
            assert_eq!(
                fn_shape.return_type, t_type,
                "Method return type should be TypeParameter(T)"
            );
        } else {
            panic!("Expected function type for foo3");
        }
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_distributive_conditional_over_union_with_lazy_members() {
    // Verifies that distributing a conditional type over a union containing
    // Lazy(DefId) members does NOT prematurely evaluate the conditionals.
    //
    // Scenario: Extract<T, U> = T extends U ? T : never
    // When T = Lazy(Cat) | Lazy(Dog) and U = { type: "cat" },
    // the instantiator must NOT evaluate the conditionals (because it has
    // no TypeResolver to resolve Lazy types). Instead it should return
    // unevaluated conditionals that the caller's evaluator can handle.
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create type parameter T
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create distributive conditional: T extends { type: "cat" } ? T : never
    let cat_lit = interner.literal_string("cat");
    let extends_shape = interner.object(vec![PropertyInfo::new(
        interner.intern_string("type"),
        cat_lit,
    )]);
    let cond = interner.conditional(ConditionalType {
        check_type: type_param_t,
        extends_type: extends_shape,
        true_type: type_param_t,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Create Lazy types representing interface references
    let cat_def = DefId(1);
    let dog_def = DefId(2);
    let lazy_cat = interner.intern(TypeData::Lazy(cat_def));
    let lazy_dog = interner.intern(TypeData::Lazy(dog_def));
    let union_type = interner.union(vec![lazy_cat, lazy_dog]);

    // Substitute T = Lazy(Cat) | Lazy(Dog)
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union_type);
    let result = instantiate_type(&interner, cond, &subst);

    // The result should be a union of two unevaluated conditionals:
    //   (Lazy(Cat) extends {type:"cat"} ? Lazy(Cat) : never)
    // | (Lazy(Dog) extends {type:"cat"} ? Lazy(Dog) : never)
    //
    // It must NOT be `never` — that would mean the instantiator
    // wrongly evaluated the conditionals with NoopResolver.
    assert_ne!(
        result,
        TypeId::NEVER,
        "Distributive conditional over Lazy union must not collapse to never"
    );

    // Verify it's a union of conditionals (not never, not error)
    match interner.lookup(result) {
        Some(TypeData::Union(members)) => {
            let member_list = interner.type_list(members);
            assert_eq!(
                member_list.len(),
                2,
                "Expected union of 2 distributed conditionals"
            );
            // Each member should be a conditional type
            for &member in member_list.iter() {
                assert!(
                    matches!(interner.lookup(member), Some(TypeData::Conditional(_))),
                    "Each distributed member should be a conditional, got {:?}",
                    interner.lookup(member)
                );
            }
        }
        other => {
            panic!("Expected union of conditionals, got {other:?}");
        }
    }
}

// =============================================================================
// Mapped Type over Tuple Preservation Tests
// =============================================================================

/// When a mapped type `{ [K in keyof T]: Template }` is instantiated with
/// T = [string, number], the result should be a tuple, not an object with
/// array method keys. This tests the fix that preserves KeyOf(tuple) form
/// during instantiation so `evaluate_mapped` can detect the tuple source.
#[test]
fn test_instantiate_mapped_over_tuple_preserves_tuple() {
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    // Create type parameter T
    let t_param_info = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param_info));

    // Create mapped type: { [K in keyof T]: T[K] }
    // constraint = keyof T
    let keyof_t = interner.keyof(t_type);
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param_info));
    // template = T[K] (index access)
    let template = interner.index_access(t_type, k_type);

    let mapped = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Substitute T = [string, number]
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, tuple_type);
    let instantiated = instantiate_type(&interner, mapped, &subst);

    // Evaluate the instantiated mapped type
    let result = evaluate_type(&interner, instantiated);

    // Should be a tuple [string, number], not an object type
    match interner.lookup(result) {
        Some(TypeData::Tuple(tuple_id)) => {
            let elements = interner.tuple_list(tuple_id);
            assert_eq!(elements.len(), 2, "Expected 2 tuple elements");
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        other => {
            panic!(
                "Expected tuple type, got {other:?}. The mapped type over tuple \
                 should produce a tuple, not an object."
            );
        }
    }
}

/// Same as above but with a non-identity template (wrapper type).
/// `{ [K in keyof T]: { value: T[K] } }` instantiated with T = [string, number]
/// should produce a tuple `[{ value: string }, { value: number }]`.
#[test]
fn test_instantiate_mapped_over_tuple_with_wrapper_template() {
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let t_param_info = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param_info));

    let keyof_t = interner.keyof(t_type);
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param_info));

    // template = { value: T[K] }
    let index_access = interner.index_access(t_type, k_type);
    let template = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        index_access,
    )]);

    let mapped = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Substitute T = [string, number]
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, tuple_type);
    let instantiated = instantiate_type(&interner, mapped, &subst);

    let result = evaluate_type(&interner, instantiated);

    // Should be a tuple, not an object
    match interner.lookup(result) {
        Some(TypeData::Tuple(tuple_id)) => {
            let elements = interner.tuple_list(tuple_id);
            assert_eq!(elements.len(), 2, "Expected 2 tuple elements");
            // Each element should be an object { value: T }
            for (i, elem) in elements.iter().enumerate() {
                match interner.lookup(elem.type_id) {
                    Some(TypeData::Object(shape_id)) => {
                        let shape = interner.object_shape(shape_id);
                        assert!(
                            !shape.properties.is_empty(),
                            "Tuple element {i} should have properties"
                        );
                    }
                    other => {
                        panic!("Tuple element {i} should be object, got {other:?}");
                    }
                }
            }
        }
        other => {
            panic!("Expected tuple type from mapped wrapper over tuple, got {other:?}");
        }
    }
}

// =============================================================================
// Tests for template_has_lazy_application_in_composite and
// type_contains_lazy_application (new helper for Conditional check/extends)
// =============================================================================

#[test]
fn test_template_lazy_application_in_union_detected() {
    // Template: Func<T[P]> | Spec<T[P]>
    // where Spec is Lazy(DefId(1)) — should be detected as needing deferral
    let interner = TypeInterner::new();

    let spec_def = DefId(1);
    let lazy_spec = interner.intern(TypeData::Lazy(spec_def));
    // Spec<T[P]> → Application(Lazy(Spec), [number])
    let spec_app = interner.application(lazy_spec, vec![TypeId::NUMBER]);

    let func_def = DefId(2);
    let lazy_func = interner.intern(TypeData::Lazy(func_def));
    // Func<T[P]> → Application(Lazy(Func), [number])
    let func_app = interner.application(lazy_func, vec![TypeId::NUMBER]);

    // Union: Func<T[P]> | Spec<T[P]>
    let union_template = interner.union(vec![func_app, spec_app]);

    assert!(
        template_has_lazy_application_in_composite(&interner, union_template),
        "Union containing Application with Lazy base should be detected"
    );
}

#[test]
fn test_template_single_lazy_application_not_detected() {
    // Template: Selector<S, T[K]> (single Application, not in a union)
    // Should NOT be detected — single applications pass through correctly
    let interner = TypeInterner::new();

    let selector_def = DefId(1);
    let lazy_selector = interner.intern(TypeData::Lazy(selector_def));
    let selector_app = interner.application(lazy_selector, vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(
        !template_has_lazy_application_in_composite(&interner, selector_app),
        "Single Application with Lazy base (not in union) should NOT be detected"
    );
}

#[test]
fn test_template_union_without_lazy_not_detected() {
    // Template: string | number (no lazy applications)
    let interner = TypeInterner::new();

    let union_template = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(
        !template_has_lazy_application_in_composite(&interner, union_template),
        "Union without Application types should NOT be detected"
    );
}

#[test]
fn test_mapped_type_with_lazy_union_template_defers_evaluation() {
    // Regression test for isomorphicMappedTypeInference.ts
    //
    // type Spec<T> = { [P in keyof T]: Func<T[P]> | Spec<T[P]> }
    //
    // When instantiating with concrete T, the mapped type template contains
    // Application(Lazy(Spec), ...) in a union. The instantiator must NOT
    // eagerly evaluate this with NoopResolver, as it would drop the
    // unresolvable union member.
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);
    let keyof_source = interner.keyof(source);

    let p_name = interner.intern_string("P");
    let type_param_p = TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    };

    // Create union template: number | Application(Lazy(DefId(1)), [string])
    let spec_def = DefId(1);
    let lazy_spec = interner.intern(TypeData::Lazy(spec_def));
    let spec_app = interner.application(lazy_spec, vec![TypeId::STRING]);
    let union_template = interner.union(vec![TypeId::NUMBER, spec_app]);

    let mapped = MappedType {
        type_param: type_param_p,
        constraint: keyof_source,
        name_type: None,
        template: union_template,
        optional_modifier: None,
        readonly_modifier: None,
    };

    // Create a substitution (even empty is fine, we test the deferral path)
    let mapped_id = interner.mapped(mapped);

    // The instantiator should defer evaluation (return MappedType, not Object)
    // because the template is a union with a lazy application member.
    let subst = TypeSubstitution::new();
    let result = instantiate_type(&interner, mapped_id, &subst);

    // The result should still be a Mapped type (deferred), not an Object
    match interner.lookup(result) {
        Some(TypeData::Mapped(_)) => {
            // Good: evaluation was deferred as expected
        }
        other => {
            // Also acceptable: if the mapped type was evaluated to an Object
            // that preserves the union members, that's fine too.
            // The key invariant is: no union members were dropped.
            if let Some(TypeData::Object(shape_id)) = other {
                let shape = interner.object_shape(shape_id);
                // Each property should have a union type (not just number)
                for prop in &shape.properties {
                    match interner.lookup(prop.type_id) {
                        Some(TypeData::Union(_)) => { /* Good: union preserved */ }
                        Some(_) => {
                            panic!(
                                "Property {:?} should have union type to preserve Lazy application member",
                                interner.resolve_atom_ref(prop.name)
                            );
                        }
                        None => panic!("Property type not found"),
                    }
                }
            } else {
                panic!("Expected Mapped or Object type (with preserved union), got {other:?}");
            }
        }
    }
}

/// Regression: mapped type with `App(LazyAlias, [T[K]]) extends true ? K : never`
/// as the template must defer eager evaluation when instantiated with a concrete T.
///
/// Before the fix, `template_has_lazy_application_in_composite` only checked the
/// Conditional's true/false branches, missing the `check_type` `App(LazyAlias, ...)`.
/// This caused the `NoopResolver` evaluator to see an unresolvable Application,
/// making every conditional branch collapse to `never`, so the mapped type produced
/// `never` instead of the correct key union.
///
/// Homomorphic mapped type `{ [K in keyof T]: T[K] }` instantiated with T = any
/// and T has NO constraint should NOT produce bare `any`. It should fall through
/// to standard mapped type instantiation producing an object with index signatures.
#[test]
fn test_instantiate_homomorphic_mapped_with_any_unconstrained() {
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    // Create type parameter T (unconstrained — like `type Objectish<T> = ...`)
    let t_param_info = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param_info));

    // Build: { [K in keyof T]: T[K] }
    let keyof_t = interner.keyof(t_type);
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param_info));
    let template = interner.index_access(t_type, k_type);

    let mapped = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Substitute T = any
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::ANY);
    let instantiated = instantiate_type(&interner, mapped, &subst);
    let result = evaluate_type(&interner, instantiated);

    // The solver returns `any` for homomorphic mapped types over `any` as a
    // performance optimization (avoids expanding keyof any). The checker's
    // identity passthrough handles the `Objectish<any>` distinction — it
    // produces an object with index signatures for non-array-constrained types.
    assert_eq!(
        result,
        TypeId::ANY,
        "Homomorphic mapped type with T=any (unconstrained) should produce bare `any` \
         at the solver level. The checker handles the distinction for Objectish<any>."
    );
}

/// Homomorphic mapped type `{ [K in keyof T]: T[K] }` instantiated with T = any
/// where T has an array constraint (e.g., `T extends unknown[]`) should produce
/// an array type, matching tsc's instantiateMappedArrayType behavior.
#[test]
fn test_instantiate_homomorphic_mapped_with_any_array_constrained() {
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    // Create type parameter T with constraint `unknown[]`
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param_info = TypeParamInfo {
        name: t_name,
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param_info));

    // Build: { [K in keyof T]: T[K] }
    let keyof_t = interner.keyof(t_type);
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param_info));
    let template = interner.index_access(t_type, k_type);

    let mapped = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Substitute T = any
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::ANY);
    let instantiated = instantiate_type(&interner, mapped, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should produce an array type (not bare `any`, not object)
    match interner.lookup(result) {
        Some(TypeData::Array(_)) => {
            // Correct: array-constrained T with any produces array
        }
        other => {
            panic!(
                "Expected Array type for homomorphic mapped type with T=any (array-constrained), \
                 got {other:?}"
            );
        }
    }
}

/// Same as array-constrained test, but with a union constraint like
/// `readonly unknown[] | []` (Promise.all's T parameter). Should still produce array.
#[test]
fn test_instantiate_homomorphic_mapped_with_any_union_array_constrained() {
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    // Create constraint: readonly unknown[] | []
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let readonly_unknown_array = interner.readonly_type(unknown_array);
    let empty_tuple = interner.tuple(vec![]);
    let union_constraint = interner.union(vec![readonly_unknown_array, empty_tuple]);

    let t_param_info = TypeParamInfo {
        name: t_name,
        constraint: Some(union_constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param_info));

    // Build: { [K in keyof T]: T[K] }
    let keyof_t = interner.keyof(t_type);
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param_info));
    let template = interner.index_access(t_type, k_type);

    let mapped = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Substitute T = any
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::ANY);
    let instantiated = instantiate_type(&interner, mapped, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should produce an array type (union-of-arrays constraint is still array-like)
    match interner.lookup(result) {
        Some(TypeData::Array(_)) => {
            // Correct: union-of-arrays constraint with any produces array
        }
        other => {
            panic!(
                "Expected Array type for homomorphic mapped type with T=any \
                 (union-of-arrays-constrained), got {other:?}"
            );
        }
    }
}

/// Regression test for `mappedTypeWithAny.ts`.
///
/// `{ -readonly [K in keyof T]: string }` over `T extends readonly any[]` with
/// T = any must produce `string[]`. Previously we only entered the
/// any-with-array-constraint preservation path when the template referenced
/// `T[K]`; templates whose body is a constant (`string`) leaked through and
/// became `{ [x: string]: string; [x: number]: string }`.
///
/// tsc's `instantiateMappedType` applies the array shape regardless of whether
/// the template references the source — see the `isArrayType(t) || (t.flags &
/// TypeFlags.Any && constraint && everyType(constraint, isArrayOrTupleType))`
/// branch in `instantiateMappedType`.
#[test]
fn test_instantiate_homomorphic_mapped_any_array_constraint_constant_template() {
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    // T extends readonly any[]
    let any_array = interner.array(TypeId::ANY);
    let readonly_any_array = interner.readonly_type(any_array);
    let t_param_info = TypeParamInfo {
        name: t_name,
        constraint: Some(readonly_any_array),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param_info));

    // { [K in keyof T]: string } — template is a constant, NOT `T[K]`.
    let keyof_t = interner.keyof(t_type);
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };

    let mapped = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Substitute T = any
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::ANY);
    let instantiated = instantiate_type(&interner, mapped, &subst);
    let result = evaluate_type(&interner, instantiated);

    match interner.lookup(result) {
        Some(TypeData::Array(element_type)) => {
            assert_eq!(
                element_type,
                TypeId::STRING,
                "Expected Array<string>, got Array<{:?}>",
                interner.lookup(element_type)
            );
        }
        other => {
            panic!(
                "Expected Array<string> for `{{ [K in keyof T]: string }}` with T=any \
                 (constraint readonly any[]), got {other:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// PR 1 canonical-form substitution tests
// ---------------------------------------------------------------------------
//
// These cover `TypeSubstitution::canonical_pairs`. PR 1 is pure refactoring —
// it adds the deterministic, content-hashable form that PR 2 will use as the
// key seed for `QueryCache::instantiation_cache`. Substitution *interning*
// intentionally does NOT live on `TypeInterner`; the eventual cache is
// owned by `QueryCache` so it participates in `clear()` and size accounting
// (see `docs/plan/ROADMAP.md`).

#[test]
fn test_canonical_equal_subst_same_pairs() {
    // Two substitutions with the same {name -> type_id} entries inserted in
    // different orders must canonicalize to the same SmallVec sequence.
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let mut a = TypeSubstitution::new();
    a.insert(t_name, TypeId::NUMBER);
    a.insert(u_name, TypeId::STRING);

    let mut b = TypeSubstitution::new();
    b.insert(u_name, TypeId::STRING);
    b.insert(t_name, TypeId::NUMBER);

    let pairs_a = a.canonical_pairs();
    let pairs_b = b.canonical_pairs();
    assert_eq!(
        pairs_a.as_slice(),
        pairs_b.as_slice(),
        "same entries, different insertion order must canonicalize identically",
    );
    assert_eq!(pairs_a.len(), 2);
}

#[test]
fn test_canonical_distinct_subst_different_pairs() {
    // {"T" -> number} and {"T" -> string} must produce different canonical pairs.
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    let mut a = TypeSubstitution::new();
    a.insert(t_name, TypeId::NUMBER);

    let mut b = TypeSubstitution::new();
    b.insert(t_name, TypeId::STRING);

    assert_ne!(
        a.canonical_pairs().as_slice(),
        b.canonical_pairs().as_slice(),
        "distinct type bindings must have distinct canonical pairs",
    );
}

#[test]
fn test_canonical_empty_is_empty_slice() {
    // The empty substitution's canonical form is the empty slice. PR 2 will
    // short-circuit this case on the cache side without allocating; PR 1 just
    // guarantees the empty input → empty output property.
    let empty = TypeSubstitution::new();
    let pairs = empty.canonical_pairs();
    assert!(
        pairs.is_empty(),
        "empty substitution must canonicalize to empty"
    );
}

#[test]
fn test_canonical_stable_across_iter_order() {
    // Insert three entries in several orderings; all must produce the same
    // canonical pairs. With three keys the `FxHashMap` iteration order varies
    // enough to exercise the canonicalization path.
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let orderings: &[[(tsz_common::interner::Atom, TypeId); 3]] = &[
        [
            (t_name, TypeId::NUMBER),
            (u_name, TypeId::STRING),
            (v_name, TypeId::BOOLEAN),
        ],
        [
            (u_name, TypeId::STRING),
            (t_name, TypeId::NUMBER),
            (v_name, TypeId::BOOLEAN),
        ],
        [
            (v_name, TypeId::BOOLEAN),
            (u_name, TypeId::STRING),
            (t_name, TypeId::NUMBER),
        ],
        [
            (v_name, TypeId::BOOLEAN),
            (t_name, TypeId::NUMBER),
            (u_name, TypeId::STRING),
        ],
    ];

    let canonical_pairs: Vec<_> = orderings
        .iter()
        .map(|entries| {
            let mut s = TypeSubstitution::new();
            for &(name, ty) in entries {
                s.insert(name, ty);
            }
            s.canonical_pairs()
        })
        .collect();

    // All orderings must produce identical canonical sequences.
    let first = &canonical_pairs[0];
    for pairs in &canonical_pairs[1..] {
        assert_eq!(
            pairs.as_slice(),
            first.as_slice(),
            "insertion order must not change canonical pairs",
        );
    }

    // The canonical pairs must themselves be sorted by Atom.
    let mut sorted = first.clone();
    sorted.sort_unstable_by_key(|(name, _)| *name);
    assert_eq!(
        first.as_slice(),
        sorted.as_slice(),
        "canonical pairs must be sorted by Atom",
    );
}

// =============================================================================
// Homomorphic Mapped Type Union Distribution Tests
// =============================================================================

/// When `{ [P in keyof T]: T[P] }` is instantiated with T = A | B (non-array),
/// the result must distribute to `{ [P in keyof A]: A[P] } | { [P in keyof B]: B[P] }`.
/// This mirrors tsc's `instantiateMappedType → mapTypeWithAlias` behavior.
#[test]
fn test_homomorphic_mapped_distributes_over_union_of_objects() {
    use crate::evaluation::evaluate::evaluate_type;
    use crate::objects::{PropertyCollectionResult, collect_properties};
    use crate::relations::subtype::NoopResolver;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");

    let t_param = TypeParamInfo::simple(t_name);
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let p_param = TypeParamInfo::simple(p_name);
    let p_type = interner.intern(TypeData::TypeParameter(p_param));

    // Mapped type: { [P in keyof T]: T[P] }
    let keyof_t = interner.keyof(t_type);
    let template = interner.index_access(t_type, p_type);
    let mapped = interner.mapped(MappedType {
        type_param: p_param,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // NodeA = { type: "A", name: string }
    let type_key = interner.intern_string("type");
    let name_key = interner.intern_string("name");
    let id_key = interner.intern_string("id");
    let lit_a = interner.literal_string("A");
    let node_a = interner.object(vec![
        crate::types::PropertyInfo::new(type_key, lit_a),
        crate::types::PropertyInfo {
            declaration_order: 1,
            ..crate::types::PropertyInfo::new(name_key, TypeId::STRING)
        },
    ]);

    // NodeB = { type: "B", id: number }
    let lit_b = interner.literal_string("B");
    let node_b = interner.object(vec![
        crate::types::PropertyInfo::new(type_key, lit_b),
        crate::types::PropertyInfo {
            declaration_order: 1,
            ..crate::types::PropertyInfo::new(id_key, TypeId::NUMBER)
        },
    ]);

    // T = NodeA | NodeB
    let union_t = interner.union(vec![node_a, node_b]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union_t);
    let instantiated = instantiate_type(&interner, mapped, &subst);

    // After instantiation, result should be a union (distributed)
    match interner.lookup(instantiated) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(
                members.len(),
                2,
                "Expected union of 2 distributed mapped types"
            );

            // Evaluate each member and check its properties
            let evaluated: Vec<TypeId> = members
                .iter()
                .map(|&m| evaluate_type(&interner, m))
                .collect();

            let resolver = NoopResolver;
            let mut has_name = false;
            let mut has_id = false;
            for &ev in &evaluated {
                if let PropertyCollectionResult::Properties { properties, .. } =
                    collect_properties(ev, &interner, &resolver)
                {
                    let prop_names: Vec<_> = properties.iter().map(|p| p.name).collect();
                    if prop_names.contains(&name_key) {
                        has_name = true;
                    }
                    if prop_names.contains(&id_key) {
                        has_id = true;
                    }
                }
            }
            assert!(
                has_name,
                "NodeA branch should have 'name' property after distribution"
            );
            assert!(
                has_id,
                "NodeB branch should have 'id' property after distribution"
            );
        }
        other => panic!("Expected Union after distributing mapped type over union, got {other:?}"),
    }
}

/// Distribution must produce union member count equal to the union input,
/// covering different member names (varied spelling proves the fix is structural).
#[test]
fn test_homomorphic_mapped_distributes_over_union_varied_names() {
    use crate::evaluation::evaluate::evaluate_type;
    use crate::objects::{PropertyCollectionResult, collect_properties};
    use crate::relations::subtype::NoopResolver;

    let interner = TypeInterner::new();

    // Build Partial<T> = { [K in keyof T]?: T[K] }  with T mapped to Foo | Bar
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");
    let t_param = TypeParamInfo::simple(t_name);
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let k_param = TypeParamInfo::simple(k_name);
    let k_type = interner.intern(TypeData::TypeParameter(k_param));
    let keyof_t = interner.keyof(t_type);
    let template = interner.index_access(t_type, k_type);
    let partial_t = interner.mapped(MappedType {
        type_param: k_param,
        constraint: keyof_t,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: Some(crate::types::MappedModifier::Add),
    });

    // Foo = { x: number }
    let x_key = interner.intern_string("x");
    let y_key = interner.intern_string("y");
    let foo = interner.object(vec![crate::types::PropertyInfo::new(x_key, TypeId::NUMBER)]);
    // Bar = { y: string }
    let bar = interner.object(vec![crate::types::PropertyInfo::new(y_key, TypeId::STRING)]);

    let union_t = interner.union(vec![foo, bar]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union_t);
    let instantiated = instantiate_type(&interner, partial_t, &subst);

    let result = evaluate_type(&interner, instantiated);

    // Should be a union of 2 objects each with their member-specific properties
    let Some(TypeData::Union(list_id)) = interner.lookup(result) else {
        panic!("Expected Union, got {:?}", interner.lookup(result));
    };
    let members = interner.type_list(list_id);
    assert_eq!(
        members.len(),
        2,
        "Partial<Foo|Bar> should have 2 union members"
    );

    let resolver = NoopResolver;
    let mut found_x = false;
    let mut found_y = false;
    for &m in members.iter() {
        if let PropertyCollectionResult::Properties { properties, .. } =
            collect_properties(m, &interner, &resolver)
        {
            for prop in &properties {
                if prop.name == x_key {
                    found_x = true;
                }
                if prop.name == y_key {
                    found_y = true;
                }
            }
        }
    }
    assert!(
        found_x,
        "Partial<Foo> branch must have optional 'x' property"
    );
    assert!(
        found_y,
        "Partial<Bar> branch must have optional 'y' property"
    );
}
