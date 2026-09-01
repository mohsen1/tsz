//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/functions/checking/nonlocal_type_params.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b993a7b83251aff27f9ad8b733a5258d95eb8d1f380f969564aa075916dfd480 223 hoisting_alpha_pairs_distinct_scoped_declarations_by_name
    #[test]
    fn hoisting_alpha_pairs_distinct_scoped_declarations_by_name() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nonlocal-hoist.ts");
        let name = interner.intern_string("T");
        let source_param = scoped_param(name, file, 1);
        let target_param = scoped_param(name, file, 2);
        let source = function(vec![], interner.fresh_type_param(source_param));
        let target = function(vec![target_param], interner.fresh_type_param(target_param));
        let mut checker = SubtypeChecker::new(&interner);

        let (hoisted, replacements) = checker
            .hoist_matching_nonlocal_type_params(&source, &target)
            .expect("distinct declarations with the same name alpha-pair");

        assert_eq!(hoisted, vec![source_param]);
        assert!(replacements.is_empty());
    }
// TSZ_INLINE_TEST_END b993a7b83251aff27f9ad8b733a5258d95eb8d1f380f969564aa075916dfd480

// TSZ_INLINE_TEST_BEGIN de80a51d741cf8c421fd1a53d0aa9bcd0fb7ca3d1a3b26f2591fe18fea9ff856 242 hoisting_does_not_pair_differently_named_declarations
    #[test]
    fn hoisting_does_not_pair_differently_named_declarations() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nonlocal-hoist-negative.ts");
        let source_param = scoped_param(interner.intern_string("T"), file, 1);
        let target_param = scoped_param(interner.intern_string("U"), file, 2);
        let source = function(vec![], interner.fresh_type_param(source_param));
        let target = function(vec![target_param], interner.fresh_type_param(target_param));
        let mut checker = SubtypeChecker::new(&interner);

        assert!(
            checker
                .hoist_matching_nonlocal_type_params(&source, &target)
                .is_none()
        );
    }
// TSZ_INLINE_TEST_END de80a51d741cf8c421fd1a53d0aa9bcd0fb7ca3d1a3b26f2591fe18fea9ff856
