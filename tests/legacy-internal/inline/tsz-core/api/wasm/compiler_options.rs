//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/api/wasm/compiler_options.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1708b774854b7673babf166b55261298fda313cb3f7c0aeed9285ee65591ac8b 276 parse_compiler_options_json_accepts_valid_input
    #[test]
    fn parse_compiler_options_json_accepts_valid_input() {
        let parsed = parse_compiler_options_json(r#"{"strict":true,"module":99}"#);
        assert!(parsed.is_ok(), "valid options JSON should parse");
    }
// TSZ_INLINE_TEST_END 1708b774854b7673babf166b55261298fda313cb3f7c0aeed9285ee65591ac8b

// TSZ_INLINE_TEST_BEGIN 3ead6f1d53022eaa190635d05d879431a8d4483c8087b866ebb66c561ff505db 282 parse_compiler_options_json_uses_separate_target_and_module_domains
    #[test]
    fn parse_compiler_options_json_uses_separate_target_and_module_domains() {
        let parsed =
            parse_compiler_options_json(r#"{"target":"ES2015","module":"ES2015"}"#).unwrap();

        assert_eq!(parsed.target, Some(2));
        assert_eq!(parsed.module, Some(5));
        assert_eq!(
            parsed.to_checker_options().module,
            crate::common::ModuleKind::ES2015
        );
    }
// TSZ_INLINE_TEST_END 3ead6f1d53022eaa190635d05d879431a8d4483c8087b866ebb66c561ff505db

// TSZ_INLINE_TEST_BEGIN 18f0dd69624d28535c5deb84d0496de0657a66a9364308ef3243f1a5fa1f40a8 295 to_checker_options_uses_shared_target_numeric_conversion
    #[test]
    fn to_checker_options_uses_shared_target_numeric_conversion() {
        let options = parse_compiler_options_json(r#"{"target":12}"#)
            .unwrap()
            .to_checker_options();

        assert_eq!(options.target, crate::common::ScriptTarget::ES2025);
    }
// TSZ_INLINE_TEST_END 18f0dd69624d28535c5deb84d0496de0657a66a9364308ef3243f1a5fa1f40a8

// TSZ_INLINE_TEST_BEGIN 49e9b630d04c90d76bb9484444b8d4ebd91f9408727917345b0d6c78a5265ffe 304 to_checker_options_starts_from_shared_defaults
    #[test]
    fn to_checker_options_starts_from_shared_defaults() {
        let options = CompilerOptions::default().to_checker_options();
        let defaults = crate::checker::context::CheckerOptions::default();

        assert!(options.strict);
        assert!(options.no_implicit_any);
        assert!(options.strict_bind_call_apply);
        assert!(options.use_unknown_in_catch_variables);
        assert!(options.always_strict);
        assert!(options.strict_builtin_iterator_return);
        assert_eq!(options.jsx_factory, defaults.jsx_factory);
        assert_eq!(options.jsx_fragment_factory, defaults.jsx_fragment_factory);
        assert_eq!(options.target, defaults.target);
        assert_eq!(options.module, defaults.module);
        assert_eq!(
            options.no_unchecked_side_effect_imports,
            defaults.no_unchecked_side_effect_imports
        );
    }
// TSZ_INLINE_TEST_END 49e9b630d04c90d76bb9484444b8d4ebd91f9408727917345b0d6c78a5265ffe

// TSZ_INLINE_TEST_BEGIN 389cfbc96a2e10ea1e690959597001b34cfe7d7ee36fb2e1bca86b390d45a857 325 to_checker_options_strict_false_matches_shared_resolver_shape
    #[test]
    fn to_checker_options_strict_false_matches_shared_resolver_shape() {
        let options = parse_compiler_options_json(r#"{"strict":false}"#)
            .unwrap()
            .to_checker_options();

        assert!(!options.strict);
        assert!(!options.no_implicit_any);
        assert!(!options.strict_null_checks);
        assert!(!options.strict_function_types);
        assert!(!options.strict_bind_call_apply);
        assert!(!options.strict_property_initialization);
        assert!(!options.no_implicit_this);
        assert!(!options.use_unknown_in_catch_variables);
        assert!(!options.strict_builtin_iterator_return);
        assert!(
            options.always_strict,
            "strict:false should not clobber the shared alwaysStrict default \
             (tsc 6.0: alwaysStrict is not a strict-family member)"
        );
    }
// TSZ_INLINE_TEST_END 389cfbc96a2e10ea1e690959597001b34cfe7d7ee36fb2e1bca86b390d45a857

// TSZ_INLINE_TEST_BEGIN 4ea7678fe1496a676a89506816fd49c504b8a7cb3bcde39ae3d18c08d63628f8 349 to_checker_options_strict_false_with_explicit_member_true
    /// Issue #3861 ordering through the WASM lane: an explicit member wins
    /// over the `strict: false` umbrella contraction.
    #[test]
    fn to_checker_options_strict_false_with_explicit_member_true() {
        let options = parse_compiler_options_json(r#"{"strict":false,"strictNullChecks":true}"#)
            .unwrap()
            .to_checker_options();

        assert!(!options.strict);
        assert!(options.strict_null_checks, "explicit member wins (#3861)");
        assert!(!options.no_implicit_any);
        assert!(!options.strict_function_types);
    }
// TSZ_INLINE_TEST_END 4ea7678fe1496a676a89506816fd49c504b8a7cb3bcde39ae3d18c08d63628f8

// TSZ_INLINE_TEST_BEGIN 29cd42c6d016ba59d12185739ae540018077cb30351aa9dbe52a4fb2e57fdade 361 to_checker_options_individual_flags_override_strict
    #[test]
    fn to_checker_options_individual_flags_override_strict() {
        let options = parse_compiler_options_json(
            r#"{
                "strict": true,
                "noImplicitAny": false,
                "strictNullChecks": false,
                "strictBindCallApply": false,
                "strictBuiltinIteratorReturn": false,
                "useUnknownInCatchVariables": false
            }"#,
        )
        .unwrap()
        .to_checker_options();

        assert!(options.strict);
        assert!(!options.no_implicit_any);
        assert!(!options.strict_null_checks);
        assert!(!options.strict_bind_call_apply);
        assert!(!options.strict_builtin_iterator_return);
        assert!(!options.use_unknown_in_catch_variables);
    }
// TSZ_INLINE_TEST_END 29cd42c6d016ba59d12185739ae540018077cb30351aa9dbe52a4fb2e57fdade

// TSZ_INLINE_TEST_BEGIN 93854eb7c82fa46176e0f50ed0601a294c2cce8e5f510b2c2d5307d9289ee38d 384 to_checker_options_preserves_downlevel_iteration
    #[test]
    fn to_checker_options_preserves_downlevel_iteration() {
        let options = parse_compiler_options_json(r#"{"downlevelIteration":true}"#)
            .unwrap()
            .to_checker_options();

        assert!(options.downlevel_iteration);
    }
// TSZ_INLINE_TEST_END 93854eb7c82fa46176e0f50ed0601a294c2cce8e5f510b2c2d5307d9289ee38d

// TSZ_INLINE_TEST_BEGIN 311757b1d4617a3d65449b0a7a11a6c126b2602ae3605c3bea470d187c8c87da 393 parse_compiler_options_json_ignores_no_types_and_symbols
    #[test]
    fn parse_compiler_options_json_ignores_no_types_and_symbols() {
        let parsed = parse_compiler_options_json(r#"{"noTypesAndSymbols":true}"#).unwrap();
        assert!(
            !parsed.to_checker_options().no_types_and_symbols,
            "WASM compiler options should ignore noTypesAndSymbols"
        );
    }
// TSZ_INLINE_TEST_END 311757b1d4617a3d65449b0a7a11a6c126b2602ae3605c3bea470d187c8c87da

// TSZ_INLINE_TEST_BEGIN 657c34a2ca9483cfc97ec5dfd10194248a48005040614d316868f8fbefd9c799 402 parse_compiler_options_json_rejects_string_boolean
    #[test]
    fn parse_compiler_options_json_rejects_string_boolean() {
        let parsed = serde_json::from_str::<CompilerOptions>(r#"{"strict":"true"}"#);
        assert!(
            parsed.is_err(),
            "string-typed booleans should be rejected in WASM compiler options"
        );
    }
// TSZ_INLINE_TEST_END 657c34a2ca9483cfc97ec5dfd10194248a48005040614d316868f8fbefd9c799

// TSZ_INLINE_TEST_BEGIN 1f65c762b9b6dbf914c7df6e939e94b9546ede2fa14b36ccff6e236668de31d0 411 parse_compiler_options_json_rejects_comma_separated_boolean_string
    #[test]
    fn parse_compiler_options_json_rejects_comma_separated_boolean_string() {
        let parsed = serde_json::from_str::<CompilerOptions>(r#"{"strict":"true, false"}"#);
        assert!(
            parsed.is_err(),
            "comma-separated boolean strings should be rejected in WASM compiler options"
        );
    }
// TSZ_INLINE_TEST_END 1f65c762b9b6dbf914c7df6e939e94b9546ede2fa14b36ccff6e236668de31d0
