//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/substitute.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c7e53373739a9fff93d4e7b9e01cafba5dfbd1293d34f396210300b9a485695a 486 test_substitute_exact_type_handles_shared_hash_consed_nodes
    /// Regression: `substitute_exact_type` must substitute every occurrence
    /// of `from`, including hash-consed nodes that appear via multiple paths.
    /// Previously the visit-once `seen` set caused later occurrences of a
    /// shared node to be returned unchanged.
    #[test]
    fn test_substitute_exact_type_handles_shared_hash_consed_nodes() {
        let interner = TypeInterner::new();

        // Type parameter `T`.
        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });

        // Two named base types `Bar` and `Foo` (modeled as `Lazy(DefId)`).
        let bar = interner.lazy(DefId(101));
        let foo = interner.lazy(DefId(102));

        // Inner `Bar<T>` — the interner is hash-consed, so referencing this
        // structure twice yields the *same* TypeId.
        let bar_of_t = interner.application(bar, vec![t_param]);
        let bar_of_t_again = interner.application(bar, vec![t_param]);
        assert_eq!(
            bar_of_t, bar_of_t_again,
            "interner should return the same TypeId for structurally identical Application types"
        );

        // Outer `Foo<Bar<T>, Bar<T>>` — both args are the same shared node.
        let outer = interner.application(foo, vec![bar_of_t, bar_of_t]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(outer, t_param, TypeId::STRING, &mut memo);

        // Expected: `Foo<Bar<string>, Bar<string>>`.
        let expected_inner = interner.application(bar, vec![TypeId::STRING]);
        let expected = interner.application(foo, vec![expected_inner, expected_inner]);
        assert_eq!(
            result, expected,
            "shared hash-consed node should be substituted on every occurrence"
        );

        // Sanity: pre-fix output would have been `Foo<Bar<string>, Bar<T>>`.
        let buggy_outer = interner.application(foo, vec![expected_inner, bar_of_t]);
        assert_ne!(
            result, buggy_outer,
            "second occurrence of shared node was left unsubstituted (pre-fix bug)"
        );
    }
// TSZ_INLINE_TEST_END c7e53373739a9fff93d4e7b9e01cafba5dfbd1293d34f396210300b9a485695a

// TSZ_INLINE_TEST_BEGIN bed0d3aba8c84159b04b54d1d7aa5dbfb3f34f37ecd7ab94c745aad20e2a3c8f 536 test_substitute_exact_type_reuses_memo_without_corrupting_shared_node
    #[test]
    fn test_substitute_exact_type_reuses_memo_without_corrupting_shared_node() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });

        let bar = interner.lazy(DefId(201));
        let baz = interner.lazy(DefId(202));
        let foo = interner.lazy(DefId(203));

        let bar_of_t = interner.application(bar, vec![t_param]);
        let baz_of_bar_t = interner.application(baz, vec![bar_of_t]);
        let outer = interner.application(foo, vec![bar_of_t, bar_of_t, baz_of_bar_t]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(outer, t_param, TypeId::STRING, &mut memo);

        let bar_of_string = interner.application(bar, vec![TypeId::STRING]);
        let baz_of_bar_string = interner.application(baz, vec![bar_of_string]);
        let expected =
            interner.application(foo, vec![bar_of_string, bar_of_string, baz_of_bar_string]);
        assert_eq!(
            result, expected,
            "third visit to a shared node must reuse the substituted memo value"
        );

        let corrupted = interner.application(foo, vec![bar_of_string, bar_of_string, baz_of_bar_t]);
        assert_ne!(
            result, corrupted,
            "memo lookup was corrupted back to the original unsubstituted node"
        );
    }
// TSZ_INLINE_TEST_END bed0d3aba8c84159b04b54d1d7aa5dbfb3f34f37ecd7ab94c745aad20e2a3c8f

// TSZ_INLINE_TEST_BEGIN c2f3b0a200064ba072e685339fb14e40fcda1f87e850e1343638780893299d0e 577 test_substitute_exact_type_reaches_index_access_and_template_spans
    #[test]
    fn test_substitute_exact_type_reaches_index_access_and_template_spans() {
        let interner = TypeInterner::new();

        let k_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let obj = interner.lazy(DefId(301));
        let indexed = interner.index_access(obj, k_param);
        let dot = interner.intern_string(".");
        let template = interner.template_literal(vec![
            TemplateSpan::Type(k_param),
            TemplateSpan::Text(dot),
            TemplateSpan::Type(indexed),
        ]);
        let branch = interner.union(vec![indexed, template]);
        let meta = interner.literal_string("meta");

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(branch, k_param, meta, &mut memo);

        let expected_indexed = interner.index_access(obj, meta);
        let expected_template = interner.template_literal(vec![
            TemplateSpan::Type(meta),
            TemplateSpan::Text(dot),
            TemplateSpan::Type(expected_indexed),
        ]);
        let expected = interner.union(vec![expected_indexed, expected_template]);
        assert_eq!(
            result, expected,
            "distributive branch substitution must update T[K] and template-literal K spans"
        );
    }
// TSZ_INLINE_TEST_END c2f3b0a200064ba072e685339fb14e40fcda1f87e850e1343638780893299d0e

// TSZ_INLINE_TEST_BEGIN d98d8e08452d4088a7eac4e0998b718c1703bcb1fc464134dbdd822a45fd6031 624 test_substitute_exact_type_reaches_object_property_types
    /// Regression for the distributive-conditional-over-deferred-union family
    /// (issue #10864): when a distributive conditional's check side is a
    /// deferred union the per-member rewrite runs through
    /// `substitute_exact_type`. The true branch is frequently an object literal
    /// (`{ value: T }`, `{ kind; value: T }`), so substitution must reach into
    /// object property read/write types — otherwise every union member collapses
    /// to one widened object and the conditional becomes over-constrained.
    #[test]
    fn test_substitute_exact_type_reaches_object_property_types() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let value_atom = interner.intern_string("value");
        let nested_atom = interner.intern_string("inner");

        // `{ value: { inner: T } }` — the distribution variable is two object
        // levels deep, so the rewrite must recurse structurally.
        let inner = interner.object(vec![PropertyInfo::new(nested_atom, t_param)]);
        let branch = interner.object(vec![PropertyInfo::new(value_atom, inner)]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(branch, t_param, TypeId::NUMBER, &mut memo);

        let expected_inner = interner.object(vec![PropertyInfo::new(nested_atom, TypeId::NUMBER)]);
        let expected = interner.object(vec![PropertyInfo::new(value_atom, expected_inner)]);
        assert_eq!(
            result, expected,
            "object-valued distributive branch must substitute the variable inside property types"
        );
        assert_ne!(
            result, branch,
            "pre-fix behaviour left object property types unsubstituted, widening the branch"
        );
    }
// TSZ_INLINE_TEST_END d98d8e08452d4088a7eac4e0998b718c1703bcb1fc464134dbdd822a45fd6031

// TSZ_INLINE_TEST_BEGIN 2b27820dcea2ff7f9a7ee02bdb2dda904582db33cc6d69e0e750766d3b6c8d4d 666 test_substitute_exact_type_reaches_callable_call_signature
    /// Callable branch types carry the distribution variable in their call
    /// signatures. When a distributive conditional's true/false branch is a
    /// type literal with call signatures (`{ (arg: T): T }`), the solver
    /// represents it as `TypeData::Callable`. Without the `Callable` arm in
    /// `substitute_exact_type_db` every union member would collapse to the
    /// same hash-consed Callable (T still free) instead of a per-member union.
    #[test]
    fn test_substitute_exact_type_reaches_callable_call_signature() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let arg_atom = interner.intern_string("arg");

        // `{ (arg: T): T }` — call signature with T in param and return.
        let callable = interner.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![ParamInfo::required(arg_atom, t_param)],
                this_type: None,
                return_type: t_param,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(callable, t_param, TypeId::STRING, &mut memo);

        // The substituted Callable should have `(arg: string): string`.
        let expected = interner.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![ParamInfo::required(arg_atom, TypeId::STRING)],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        assert_eq!(
            result, expected,
            "Callable call-signature param/return types must be substituted"
        );
        assert_ne!(
            result, callable,
            "pre-fix: Callable was returned unchanged with T still free"
        );
    }
// TSZ_INLINE_TEST_END 2b27820dcea2ff7f9a7ee02bdb2dda904582db33cc6d69e0e750766d3b6c8d4d

// TSZ_INLINE_TEST_BEGIN 25bbd25d49458887c7dc3e5c1e8eed4d4208a6b1478be89dbdb90dddcb9123a0 734 test_substitute_exact_type_object_no_match_preserves_identity
    /// A no-op substitution (the variable does not occur in the object) must
    /// return the original hash-consed `TypeId` so identity-based caches and
    /// display aliases are preserved.
    #[test]
    fn test_substitute_exact_type_object_no_match_preserves_identity() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let value_atom = interner.intern_string("value");
        let branch = interner.object(vec![PropertyInfo::new(value_atom, TypeId::STRING)]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(branch, t_param, TypeId::NUMBER, &mut memo);

        assert_eq!(
            result, branch,
            "object without the substituted variable must keep its original TypeId"
        );
    }
// TSZ_INLINE_TEST_END 25bbd25d49458887c7dc3e5c1e8eed4d4208a6b1478be89dbdb90dddcb9123a0
