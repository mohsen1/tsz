//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/flow.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ac48c0570815fbcd30fc43cbe32e95186aa19c0894a64f7903ccd7437d1bc0ec 1183 void_and_undefined_are_assertion_comparable_both_directions
    #[test]
    fn void_and_undefined_are_assertion_comparable_both_directions() {
        let db = TypeInterner::new();
        // `void` and `undefined` overlap in tsc's comparable relation, in both
        // assertion directions.
        assert!(types_are_comparable_for_assertion(
            &db,
            TypeId::VOID,
            TypeId::UNDEFINED
        ));
        assert!(types_are_comparable_for_assertion(
            &db,
            TypeId::UNDEFINED,
            TypeId::VOID
        ));
        // The rule stays scoped to void/undefined — unrelated primitives stay
        // incomparable.
        assert!(!types_are_comparable_for_assertion(
            &db,
            TypeId::UNDEFINED,
            TypeId::STRING
        ));
        assert!(!types_are_comparable_for_assertion(
            &db,
            TypeId::VOID,
            TypeId::NUMBER
        ));
    }
// TSZ_INLINE_TEST_END ac48c0570815fbcd30fc43cbe32e95186aa19c0894a64f7903ccd7437d1bc0ec

// TSZ_INLINE_TEST_BEGIN e216a79ba926602023b22177f562edac5b87b60c426bfa7bcbe360ee4e51d2bf 1212 singleton_predicate_excludes_base_primitives
    #[test]
    fn singleton_predicate_excludes_base_primitives() {
        let interner = TypeInterner::new();
        // Base primitives cannot hold a top-level singleton: source literals
        // widen against them.
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::NUMBER
        ));
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::STRING
        ));
        // `boolean` is two literals but is treated as a non-singleton primitive,
        // mirroring tsc's explicit `TypeFlags.Boolean` carve-out.
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::BOOLEAN
        ));
    }
// TSZ_INLINE_TEST_END e216a79ba926602023b22177f562edac5b87b60c426bfa7bcbe360ee4e51d2bf

// TSZ_INLINE_TEST_BEGIN c87be12746b2ffda61c3c10af509be368b94292ac8dc657514a6dc1a36665950 1233 singleton_predicate_includes_unit_types
    #[test]
    fn singleton_predicate_includes_unit_types() {
        let interner = TypeInterner::new();
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            TypeId::BOOLEAN_TRUE
        ));
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            TypeId::NULL
        ));
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            TypeId::UNDEFINED
        ));
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            interner.literal_number(1.0)
        ));
    }
// TSZ_INLINE_TEST_END c87be12746b2ffda61c3c10af509be368b94292ac8dc657514a6dc1a36665950

// TSZ_INLINE_TEST_BEGIN fb8a65970e4f83bd01aad9629b6ca2daf71a73d346b91070cf4d6e06e0cf0f79 1254 singleton_predicate_unions_use_any_member
    #[test]
    fn singleton_predicate_unions_use_any_member() {
        let interner = TypeInterner::new();
        // No singleton member -> false (source widens).
        let primitive_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            primitive_union
        ));
        // Any singleton member -> true (source preserved), even alongside a
        // non-singleton member.
        let mixed_union = interner.union(vec![interner.literal_number(1.0), TypeId::STRING]);
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            mixed_union
        ));
    }
// TSZ_INLINE_TEST_END fb8a65970e4f83bd01aad9629b6ca2daf71a73d346b91070cf4d6e06e0cf0f79

// TSZ_INLINE_TEST_BEGIN 9f41cc4757a2d603000c6cfd31d051875126cd8e6443331cfddaed40ff1cd395 1272 singleton_predicate_boolean_union_member_counts_as_singleton
    #[test]
    fn singleton_predicate_boolean_union_member_counts_as_singleton() {
        let interner = TypeInterner::new();
        // tsc stores `boolean` in a union as `true | false` (unit members), so
        // `string | boolean` has singleton capacity even though a bare
        // `boolean` target does not. Build the member list explicitly so the
        // interner's union normalization cannot pre-flatten the intrinsic
        // away and mask the member-level rule.
        let with_boolean = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            with_boolean
        ));
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::BOOLEAN
        ));
    }
// TSZ_INLINE_TEST_END 9f41cc4757a2d603000c6cfd31d051875126cd8e6443331cfddaed40ff1cd395

// TSZ_INLINE_TEST_BEGIN c241fa2c5597d3cc01368cb30cb3be64923dea7347884ba002bc44a133d71d23 1291 singleton_predicate_conditional_answers_through_default_constraint
    #[test]
    fn singleton_predicate_conditional_answers_through_default_constraint() {
        let interner = TypeInterner::new();
        let check = interner.type_param(crate::types::TypeParamInfo::simple(
            interner.intern_string("T"),
        ));
        // `T extends string ? "a" | "b" : number` — default constraint
        // contains units -> singleton-capable.
        let unit_branch = interner.union(vec![
            interner.literal_string("a"),
            interner.literal_string("b"),
        ]);
        let cond_unit = interner.conditional(crate::types::ConditionalType {
            check_type: check,
            extends_type: TypeId::STRING,
            true_type: unit_branch,
            false_type: TypeId::NUMBER,
            is_distributive: true,
        });
        assert!(type_could_have_top_level_singleton_types(
            &interner, cond_unit
        ));
        // `T extends string ? string : number` — all-primitive constraint.
        let cond_prim = interner.conditional(crate::types::ConditionalType {
            check_type: check,
            extends_type: TypeId::STRING,
            true_type: TypeId::STRING,
            false_type: TypeId::NUMBER,
            is_distributive: true,
        });
        assert!(!type_could_have_top_level_singleton_types(
            &interner, cond_prim
        ));
    }
// TSZ_INLINE_TEST_END c241fa2c5597d3cc01368cb30cb3be64923dea7347884ba002bc44a133d71d23

// TSZ_INLINE_TEST_BEGIN d542992abfcc4a0ffeea33e308f58e3ea06ddafac600e4c14ed2acb68af09b83 1326 tuple_to_tuple_comparable_same_elements
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
// TSZ_INLINE_TEST_END d542992abfcc4a0ffeea33e308f58e3ea06ddafac600e4c14ed2acb68af09b83

// TSZ_INLINE_TEST_BEGIN e4ba1f26590a2cd965f86613300f41c796dba391662b02a57780977bfdc0f224 1360 tuple_to_tuple_comparable_with_never
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
// TSZ_INLINE_TEST_END e4ba1f26590a2cd965f86613300f41c796dba391662b02a57780977bfdc0f224

// TSZ_INLINE_TEST_BEGIN a82f7a5204a69aa91294f90fd34cdc715435e9aa19529ce2c0027d4772a994c6 1396 tuple_to_tuple_incomparable_different_lengths
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
// TSZ_INLINE_TEST_END a82f7a5204a69aa91294f90fd34cdc715435e9aa19529ce2c0027d4772a994c6

// TSZ_INLINE_TEST_BEGIN 5c2931d4a90f846982ec72a6d52df802e4ba0d2a291141323ae76b50098ab6c7 1422 tuple_to_tuple_incomparable_different_elements
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
// TSZ_INLINE_TEST_END 5c2931d4a90f846982ec72a6d52df802e4ba0d2a291141323ae76b50098ab6c7

// TSZ_INLINE_TEST_BEGIN 3231a0e2c3a583010937ec4072677d5fa5879ae5ad30ea076ded6aecf03c810e 1457 never_comparable_to_any_type
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
// TSZ_INLINE_TEST_END 3231a0e2c3a583010937ec4072677d5fa5879ae5ad30ea076ded6aecf03c810e

// TSZ_INLINE_TEST_BEGIN 5066f25e4c1f980ed1b95cfac1c471d582048dde6938ddfda6c8cbd02b33369f 1477 any_comparable_to_any_type
    #[test]
    fn any_comparable_to_any_type() {
        let interner = TypeInterner::new();
        assert!(types_are_comparable(&interner, TypeId::ANY, TypeId::STRING));
        assert!(types_are_comparable(&interner, TypeId::ANY, TypeId::NUMBER));
        assert!(types_are_comparable(&interner, TypeId::STRING, TypeId::ANY));
    }
// TSZ_INLINE_TEST_END 5066f25e4c1f980ed1b95cfac1c471d582048dde6938ddfda6c8cbd02b33369f

// TSZ_INLINE_TEST_BEGIN 0a6d5241debbe426fa4ece22376ef52bbbed86c3764b9cfcc3d2f029e76b80d7 1485 unknown_comparable_to_any_type
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
// TSZ_INLINE_TEST_END 0a6d5241debbe426fa4ece22376ef52bbbed86c3764b9cfcc3d2f029e76b80d7

// TSZ_INLINE_TEST_BEGIN 48e32baae08cd3f9acb7f5fae84a1e76c593b749d17643a93633828d7c6541cc 1500 test_extract_predicate_signature_function
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
// TSZ_INLINE_TEST_END 48e32baae08cd3f9acb7f5fae84a1e76c593b749d17643a93633828d7c6541cc

// TSZ_INLINE_TEST_BEGIN 1f369a7d252d7fad0d249068e3f5d426a0f5b0fc0d713223eb62f4dbbcb1f452 1553 assertion_comparable_object_with_lazy_property_not_resolved_by_solver
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
            non_widening: false,
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
            non_widening: false,
        }]);

        // Solver returns false because Lazy types are opaque here.
        // The checker resolves Lazy types before calling this function.
        assert!(
            !types_are_comparable_for_assertion(&db, source, target),
            "Unresolved Lazy property should not be comparable at solver level"
        );
    }
// TSZ_INLINE_TEST_END 1f369a7d252d7fad0d249068e3f5d426a0f5b0fc0d713223eb62f4dbbcb1f452

// TSZ_INLINE_TEST_BEGIN d22805b513d63b426592f7674dc942b1409db92bf2560aebf796dfc92cad8001 1607 assertion_comparable_objects_with_resolved_enum_property
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
            non_widening: false,
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
            non_widening: false,
        }]);

        // When both sides are resolved, the comparable check succeeds
        // because string is comparable to a string enum.
        assert!(
            types_are_comparable_for_assertion(&db, source, target),
            "Object with string property should be comparable to object with string enum property"
        );
    }
// TSZ_INLINE_TEST_END d22805b513d63b426592f7674dc942b1409db92bf2560aebf796dfc92cad8001

// TSZ_INLINE_TEST_BEGIN 0c3e2c763fc1222b9ff5331417dea0cf96072042b752502724dcafc91579cb77 1672 instance_type_from_constructor_uses_symbol_has_instance_predicate
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
                non_widening: false,
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
// TSZ_INLINE_TEST_END 0c3e2c763fc1222b9ff5331417dea0cf96072042b752502724dcafc91579cb77

// TSZ_INLINE_TEST_BEGIN 4e73fa6473d95e5c06dff5807f575f4370c15173087fae34b8f89800a034175d 1738 instance_type_from_constructor_erases_generic_construct_return_to_any
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
            origin: crate::types::TypeParamOrigin::User,
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
                declaration_group: 0,
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
// TSZ_INLINE_TEST_END 4e73fa6473d95e5c06dff5807f575f4370c15173087fae34b8f89800a034175d

// TSZ_INLINE_TEST_BEGIN cb03a2c2399932ed53b9e781aff37b1155c92a9d68348a9b9c79e80378f7c1e2 1782 instance_type_from_symbol_has_instance_erases_generic_predicate_to_any
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
            origin: crate::types::TypeParamOrigin::User,
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
                non_widening: false,
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
// TSZ_INLINE_TEST_END cb03a2c2399932ed53b9e781aff37b1155c92a9d68348a9b9c79e80378f7c1e2

// TSZ_INLINE_TEST_BEGIN 977ae6b218b095c74bbed088c45f768961941d25c5ff00b1cccc17e4a3f1fce1 1858 instance_type_from_constructor_uses_generic_construct_when_predicate_collapses_to_any
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
            origin: crate::types::TypeParamOrigin::User,
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
                declaration_group: 0,
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
                non_widening: false,
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
// TSZ_INLINE_TEST_END 977ae6b218b095c74bbed088c45f768961941d25c5ff00b1cccc17e4a3f1fce1

// TSZ_INLINE_TEST_BEGIN 2369267dfb6df8ea28d9718135f2924ce01be606180ec17ad26c45a0736d91cc 1945 instance_type_from_symbol_has_instance_predicate_works_for_any_value_name
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
                    non_widening: false,
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
// TSZ_INLINE_TEST_END 2369267dfb6df8ea28d9718135f2924ce01be606180ec17ad26c45a0736d91cc

// TSZ_INLINE_TEST_BEGIN 679b53376b8caf66d28718524c61bca04d9c92969412a3878f6e32fe554f40c2 2012 instance_type_from_symbol_has_instance_ignores_asserts_predicate
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
                non_widening: false,
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
// TSZ_INLINE_TEST_END 679b53376b8caf66d28718524c61bca04d9c92969412a3878f6e32fe554f40c2

// TSZ_INLINE_TEST_BEGIN 2f6a21b7b3728e67693aed2ddb32836a0f2aedeec0d7dce49bbf185b93ce26f5 2078 distinct_string_literals_are_primitive_comparable
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
// TSZ_INLINE_TEST_END 2f6a21b7b3728e67693aed2ddb32836a0f2aedeec0d7dce49bbf185b93ce26f5

// TSZ_INLINE_TEST_BEGIN 859257791d651e8c4cd01a387e685d3dcc346704e9fabedb54b1889291cf90f0 2094 same_string_literal_is_comparable
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
// TSZ_INLINE_TEST_END 859257791d651e8c4cd01a387e685d3dcc346704e9fabedb54b1889291cf90f0

// TSZ_INLINE_TEST_BEGIN 39f491bb498345c447269a42111a776b26aab9d84dd5c9f954dff27c22bf0b3e 2106 string_literal_comparable_to_string_primitive
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
// TSZ_INLINE_TEST_END 39f491bb498345c447269a42111a776b26aab9d84dd5c9f954dff27c22bf0b3e

// TSZ_INLINE_TEST_BEGIN e500b4adb2b0b49d5334c25c103b3fd356685e4aa1c3174d2ce94f3054b6641e 2121 distinct_number_literals_are_primitive_comparable
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
// TSZ_INLINE_TEST_END e500b4adb2b0b49d5334c25c103b3fd356685e4aa1c3174d2ce94f3054b6641e

// TSZ_INLINE_TEST_BEGIN ad2cb3571a39d765b4e53748e95fca943a6efa2980b00f389c73ddd6f44a3dc0 2134 enum_structural_union_comparable_to_base_primitive
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
// TSZ_INLINE_TEST_END ad2cb3571a39d765b4e53748e95fca943a6efa2980b00f389c73ddd6f44a3dc0
