//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/generic_call/resolve/finalize.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b62b9de24de3ff32b906a1a0a6d2651eab0a2a12eaa85aedcc7c8b4a9006a469 1919 exact_domain_includes_constraint_and_default_only_owned_binders
    #[test]
    fn exact_domain_includes_constraint_and_default_only_owned_binders() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("metadata.ts");
        let constraint_name = interner.intern_string("U");
        let default_name = interner.intern_string("W");

        let owned_constraint_info = scoped_param(constraint_name, file, 1);
        let foreign_constraint_info = scoped_param(constraint_name, file, 2);
        let owned_default_info = scoped_param(default_name, file, 3);
        let foreign_default_info = scoped_param(default_name, file, 4);
        let owned_constraint = interner.fresh_type_param(owned_constraint_info);
        let foreign_constraint = interner.fresh_type_param(foreign_constraint_info);
        let owned_default = interner.fresh_type_param(owned_default_info);
        let foreign_default = interner.fresh_type_param(foreign_default_info);

        let constraint = interner.tuple(vec![
            TupleElement::fixed(owned_constraint),
            TupleElement::fixed(foreign_constraint),
        ]);
        let default = interner.tuple(vec![
            TupleElement::fixed(owned_default),
            TupleElement::fixed(foreign_default),
        ]);
        let carrier = TypeParamInfo {
            name: interner.intern_string("Carrier"),
            constraint: Some(constraint),
            default: Some(default),
            is_const: false,
            origin: TypeParamOrigin::User,
        };
        let func = FunctionShape {
            type_params: vec![owned_constraint_info, owned_default_info, carrier],
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        let mut substitution = TypeSubstitution::new();
        substitution.insert(constraint_name, TypeId::NUMBER);
        substitution.insert(default_name, TypeId::STRING);
        let mut checker = CompatChecker::new(&interner);
        let evaluator = CallEvaluator::new(&interner, &mut checker);
        evaluator.protect_call_owned_type_parameters(&func, &mut substitution);

        assert_eq!(
            tuple_members(
                &interner,
                instantiate_type(&interner, constraint, &substitution),
            ),
            vec![TypeId::NUMBER, foreign_constraint],
        );
        assert_eq!(
            tuple_members(
                &interner,
                instantiate_type(&interner, default, &substitution),
            ),
            vec![TypeId::STRING, foreign_default],
        );
    }
// TSZ_INLINE_TEST_END b62b9de24de3ff32b906a1a0a6d2651eab0a2a12eaa85aedcc7c8b4a9006a469
