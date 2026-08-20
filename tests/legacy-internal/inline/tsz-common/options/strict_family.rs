//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/options/strict_family.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 398361174ff7e0042abb54e933cf9c2d723391f00f86f2db061370703929a806 138 table_matches_tsc_6_0_strict_option_name
    /// Pin the table to tsc 6.0's `StrictOptionName` union
    /// (`TypeScript/src/compiler/utilities.ts`). `alwaysStrict` must NOT be
    /// present: tsc 6.0 removed it from the strict family.
    #[test]
    fn table_matches_tsc_6_0_strict_option_name() {
        let names: Vec<&str> = STRICT_FAMILY.iter().map(|member| member.name).collect();
        assert_eq!(
            names,
            [
                "noImplicitAny",
                "noImplicitThis",
                "strictNullChecks",
                "strictFunctionTypes",
                "strictBindCallApply",
                "strictPropertyInitialization",
                "strictBuiltinIteratorReturn",
                "useUnknownInCatchVariables",
            ]
        );
        assert!(!names.contains(&"alwaysStrict"));
        assert!(!names.contains(&"noImplicitReturns"));
    }
// TSZ_INLINE_TEST_END 398361174ff7e0042abb54e933cf9c2d723391f00f86f2db061370703929a806

// TSZ_INLINE_TEST_BEGIN fbcf3023deccc67d80b75d00c58b3405791be9e7b50875e4e692e4d9461abb63 160 strict_false_then_explicit_member_true_keeps_member
    /// Issue #3861 ordering: `strict: false` plus an explicit member keeps
    /// the explicit member while contracting the rest of the family.
    #[test]
    fn strict_false_then_explicit_member_true_keeps_member() {
        let mut options = CheckerOptions::default();
        apply_strict_family(
            &mut options,
            &StrictFamilyOverrides {
                strict: Some(false),
                strict_null_checks: Some(true),
                ..Default::default()
            },
        );

        assert!(!options.strict);
        assert!(options.strict_null_checks, "explicit member wins (#3861)");
        assert!(!options.no_implicit_any);
        assert!(!options.no_implicit_this);
        assert!(!options.strict_function_types);
        assert!(!options.strict_bind_call_apply);
        assert!(!options.strict_property_initialization);
        assert!(!options.strict_builtin_iterator_return);
        assert!(!options.use_unknown_in_catch_variables);
    }
// TSZ_INLINE_TEST_END fbcf3023deccc67d80b75d00c58b3405791be9e7b50875e4e692e4d9461abb63

// TSZ_INLINE_TEST_BEGIN 2afc8f29b5bf553ac7aa5158ad3a8f86887bc5b6eb12cdcf8a17e5cc27c6d697 185 strict_true_then_explicit_member_false_keeps_member_off
    /// Issue #3861 ordering, inverse permutation: `strict: true` plus an
    /// explicit `false` member keeps the member off.
    #[test]
    fn strict_true_then_explicit_member_false_keeps_member_off() {
        let mut options = CheckerOptions::default();
        apply_strict_family(
            &mut options,
            &StrictFamilyOverrides {
                strict: Some(true),
                strict_function_types: Some(false),
                ..Default::default()
            },
        );

        assert!(options.strict);
        assert!(
            !options.strict_function_types,
            "explicit member wins (#3861)"
        );
        assert!(options.strict_null_checks);
        assert!(options.no_implicit_any);
    }
// TSZ_INLINE_TEST_END 2afc8f29b5bf553ac7aa5158ad3a8f86887bc5b6eb12cdcf8a17e5cc27c6d697

// TSZ_INLINE_TEST_BEGIN b805d0166026f21c8fdb96f17c5e5f483b19c9f183f1e62c0063dffba8dff2fb 208 no_umbrella_applies_only_explicit_members
    /// With no `strict` umbrella, explicit members still apply and the rest
    /// of the family keeps the existing values.
    #[test]
    fn no_umbrella_applies_only_explicit_members() {
        let mut options = CheckerOptions {
            strict_null_checks: false,
            no_implicit_any: false,
            ..CheckerOptions::default()
        };
        apply_strict_family(
            &mut options,
            &StrictFamilyOverrides {
                strict_null_checks: Some(true),
                ..Default::default()
            },
        );

        assert!(options.strict, "untouched: umbrella not provided");
        assert!(options.strict_null_checks, "explicit member applied");
        assert!(!options.no_implicit_any, "non-provided member untouched");
    }
// TSZ_INLINE_TEST_END b805d0166026f21c8fdb96f17c5e5f483b19c9f183f1e62c0063dffba8dff2fb

// TSZ_INLINE_TEST_BEGIN ed210176c675dff3564d15f5ab9459c3bf2ef619999ed0cd4549e3630d442714 229 empty_overrides_are_a_no_op
    /// Empty overrides are a no-op.
    #[test]
    fn empty_overrides_are_a_no_op() {
        let mut options = CheckerOptions {
            strict: false,
            strict_null_checks: false,
            ..CheckerOptions::default()
        };
        apply_strict_family(&mut options, &StrictFamilyOverrides::default());

        assert!(!options.strict);
        assert!(!options.strict_null_checks);
        assert!(options.no_implicit_any);
    }
// TSZ_INLINE_TEST_END ed210176c675dff3564d15f5ab9459c3bf2ef619999ed0cd4549e3630d442714

// TSZ_INLINE_TEST_BEGIN bc8e11a312eb73b057d3d0c16454c2efbde0cf71fb94fd01d4160eac97b085f2 246 always_strict_is_independent_of_strict
    /// tsc 6.0: `alwaysStrict` is not a strict-family member
    /// (`computedOptions.alwaysStrict` resolves `alwaysStrict !== false`
    /// independent of `strict`), so neither umbrella direction touches it.
    #[test]
    fn always_strict_is_independent_of_strict() {
        let mut options = CheckerOptions {
            always_strict: false,
            ..CheckerOptions::default()
        };
        expand_strict(&mut options, true);
        assert!(
            !options.always_strict,
            "strict: true must not force alwaysStrict (tsc 6.0)"
        );

        let mut options = CheckerOptions::default();
        assert!(options.always_strict, "tsc 6.0 default: alwaysStrict on");
        expand_strict(&mut options, false);
        assert!(
            options.always_strict,
            "strict: false must not reset alwaysStrict (tsc 6.0)"
        );
    }
// TSZ_INLINE_TEST_END bc8e11a312eb73b057d3d0c16454c2efbde0cf71fb94fd01d4160eac97b085f2

// TSZ_INLINE_TEST_BEGIN 1efad2d5474773af96c145e3762eff830936d890b038300766e319a2324d73c2 268 expand_strict_false_leaves_non_family_flags
    /// Non-family flags are never touched by the umbrella.
    #[test]
    fn expand_strict_false_leaves_non_family_flags() {
        let mut options = CheckerOptions {
            no_implicit_returns: true,
            exact_optional_property_types: true,
            no_unchecked_indexed_access: true,
            ..CheckerOptions::default()
        };
        expand_strict(&mut options, false);

        assert!(!options.strict);
        assert!(!options.strict_null_checks);
        assert!(options.no_implicit_returns);
        assert!(options.exact_optional_property_types);
        assert!(options.no_unchecked_indexed_access);
    }
// TSZ_INLINE_TEST_END 1efad2d5474773af96c145e3762eff830936d890b038300766e319a2324d73c2
