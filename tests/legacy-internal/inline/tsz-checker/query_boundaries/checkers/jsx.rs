//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/checkers/jsx.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 01980b9026ede83511288af898e0017f5f3dd7b800529556f4ee9864c34e294d 852 unresolved_jsx_signature_substitution_preserves_captured_same_named_binder
    #[test]
    fn unresolved_jsx_signature_substitution_preserves_captured_same_named_binder() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("jsx-exact-domain.tsx");
        let name = interner.intern_string("U");
        let captured = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let local = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..captured
        };
        let captured_type = interner.fresh_type_param(captured);
        let local_type = interner.fresh_type_param(local);
        let function = FunctionShape {
            type_params: vec![local],
            params: vec![
                ParamInfo::unnamed(captured_type),
                ParamInfo::unnamed(local_type),
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        let instantiated = instantiate_function_shape_preserving_unresolved_params(
            &interner,
            &function,
            &TypeSubstitution::new(),
        );

        assert_eq!(
            tsz_solver::type_param_info(&interner, instantiated.params[0].type_id),
            Some(captured),
        );
        assert_eq!(
            tsz_solver::type_param_info(&interner, instantiated.params[1].type_id),
            Some(local),
        );
    }
// TSZ_INLINE_TEST_END 01980b9026ede83511288af898e0017f5f3dd7b800529556f4ee9864c34e294d

// TSZ_INLINE_TEST_BEGIN 240ced38678cd3b5928318fe44fde4a97d9165b9f2aaa7846c222d0bfa7c2b65 899 component_element_type_check_uses_relation_outcome_boundary
    #[test]
    fn component_element_type_check_uses_relation_outcome_boundary() {
        let source = include_str!("jsx.rs");
        let helper = source
            .split("pub(crate) fn component_satisfies_element_type")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn props_are_assignable").next())
            .expect("failed to isolate JSX ElementType relation helper");
        let compact_helper = helper.split_whitespace().collect::<String>();
        let legacy = concat!("diagnostic_relation", "_boolean_guard(");

        assert!(
            compact_helper
                .contains("checker.jsx_element_type_relation_outcome(source,target).related"),
            "JSX ElementType compatibility should route relation decisions through \
             the JSX element-type RelationRequest"
        );
        assert!(
            !compact_helper.contains("checker.assign_relation_outcome(source,target).related"),
            "JSX ElementType compatibility should not use generic assignment request routing"
        );
        assert!(
            !helper.contains(legacy),
            "JSX assignability boundary should not use raw diagnostic relation \
             boolean guards"
        );
    }
// TSZ_INLINE_TEST_END 240ced38678cd3b5928318fe44fde4a97d9165b9f2aaa7846c222d0bfa7c2b65

// TSZ_INLINE_TEST_BEGIN c06cdfa5f4361bb71226a3cde271c406eb7f3cf94477597a4cf4bf726852dbb7 927 props_are_assignable_uses_jsx_props_relation_outcome_boundary
    #[test]
    fn props_are_assignable_uses_jsx_props_relation_outcome_boundary() {
        let source = include_str!("jsx.rs");

        assert!(
            source.contains("checker.jsx_props_relation_outcome(source, target).related"),
            "JSX props assignability boundary should route relation decisions \
             through the JSX props relation outcome boundary"
        );
    }
// TSZ_INLINE_TEST_END c06cdfa5f4361bb71226a3cde271c406eb7f3cf94477597a4cf4bf726852dbb7
