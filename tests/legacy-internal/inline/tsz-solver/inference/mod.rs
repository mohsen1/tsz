//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/inference/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a46d670a9369f015029346fe58540b149c19da7701d4274d1b95002038a3021a 150 infers_nested_parameter_after_alpha_rename
    /// Baseline: a type parameter nested inside an array parameter is still
    /// recovered after the alpha-rename. `(p: A[]) ~ number[]` infers `A=number`.
    #[test]
    fn infers_nested_parameter_after_alpha_rename() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("A");
        let tp_a = interner.type_param(user_param(a, None));

        let param_ty = interner.array(tp_a);
        let arg_ty = interner.array(TypeId::NUMBER);

        let bindings = infer_type_arguments_from_param_args(
            &interner,
            &[user_param(a, None)],
            &[(param_ty, arg_ty)],
        );

        assert_eq!(bindings, vec![(a, TypeId::NUMBER)]);
    }
// TSZ_INLINE_TEST_END a46d670a9369f015029346fe58540b149c19da7701d4274d1b95002038a3021a

// TSZ_INLINE_TEST_BEGIN 1d59b5257a964955f9b5e49ee4d21776cf300346727d6f0a9ab95231818322ad 173 same_named_source_parameter_does_not_drive_inference
    /// A type parameter that appears only in the *argument* (source) position,
    /// sharing a declared parameter's name, must not be treated as that
    /// inference variable. Matching the concrete target `string` against a
    /// source `A` previously wired the foreign `A` to our variable as a spurious
    /// upper bound (the fp-ts name leak); now it contributes nothing.
    #[test]
    fn same_named_source_parameter_does_not_drive_inference() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("A");
        // A distinct declaration of `A` (constrained) standing in for the
        // leaked argument-side parameter.
        let foreign_a = interner.type_param(user_param(a, Some(TypeId::BOOLEAN)));

        let bindings = infer_type_arguments_from_param_args(
            &interner,
            &[user_param(a, None)],
            &[(TypeId::STRING, foreign_a)],
        );

        assert!(
            bindings.is_empty(),
            "a same-named source-only parameter must not let us infer the declared parameter, got {bindings:?}"
        );
    }
// TSZ_INLINE_TEST_END 1d59b5257a964955f9b5e49ee4d21776cf300346727d6f0a9ab95231818322ad

// TSZ_INLINE_TEST_BEGIN 34f56f11cbe892faf14b86dd72d39f53c46e6e436e131e1c6e2170490d938997 198 legit_inference_unperturbed_by_same_named_source_parameter
    /// A legitimate inference must survive a same-named source parameter in
    /// another pair: `(p: A) ~ number` binds `A=number`, and a second pair whose
    /// source is a foreign `A` must not perturb that binding. Under the
    /// name-collision bug the foreign `A` injected a `string` upper bound that
    /// corrupted the result.
    #[test]
    fn legit_inference_unperturbed_by_same_named_source_parameter() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("A");
        let tp_a = interner.type_param(user_param(a, None));
        let foreign_a = interner.type_param(user_param(a, Some(TypeId::BOOLEAN)));

        let bindings = infer_type_arguments_from_param_args(
            &interner,
            &[user_param(a, None)],
            &[(tp_a, TypeId::NUMBER), (TypeId::STRING, foreign_a)],
        );

        assert_eq!(bindings, vec![(a, TypeId::NUMBER)]);
    }
// TSZ_INLINE_TEST_END 34f56f11cbe892faf14b86dd72d39f53c46e6e436e131e1c6e2170490d938997
