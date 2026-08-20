//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/parallel/diagnostics.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN db34393d93948f16cb8ecb5d192e317f32b61dd656d92ff0e82dfae60ab9fb19 1377 ts2339_suppression_no_ts2454
    /// TS2339 is only suppressed when it cascades from a TS2454 on the same
    /// receiver position; standalone TS2339s with no TS2454 present must survive.
    #[test]
    fn ts2339_suppression_no_ts2454() {
        let arena = NodeArena::default();
        let mut diagnostics = vec![
            Diagnostic::error(
                "test.ts".to_string(),
                10,
                3,
                "Property 'foo' does not exist on type 'Bar'.".to_string(),
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            ),
            Diagnostic::error(
                "test.ts".to_string(),
                50,
                3,
                "Property 'baz' does not exist on type 'Qux'.".to_string(),
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            ),
        ];
        let original_len = diagnostics.len();
        suppress_parallel_ts2339_cascade_diagnostics(&arena, &mut diagnostics);
        assert_eq!(diagnostics.len(), original_len);
    }
// TSZ_INLINE_TEST_END db34393d93948f16cb8ecb5d192e317f32b61dd656d92ff0e82dfae60ab9fb19

// TSZ_INLINE_TEST_BEGIN 5338fae54c5e6014f38bab3ad9267dd3232d8cea516c78925c51ab63371c9fa9 1403 ts2339_suppression_with_unrelated_ts2454
    /// Only TS2339 whose receiver's source position matches a TS2454 start is
    /// suppressed; a TS2454 at an unrelated position must not suppress other TS2339s.
    #[test]
    fn ts2339_suppression_with_unrelated_ts2454() {
        let arena = NodeArena::default();
        let mut diagnostics = vec![
            Diagnostic::error(
                "test.ts".to_string(),
                0,
                5,
                "Variable 'x' is used before being assigned.".to_string(),
                diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
            ),
            Diagnostic::error(
                "test.ts".to_string(),
                100,
                3,
                "Property 'foo' does not exist.".to_string(),
                diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
            ),
        ];
        let original_len = diagnostics.len();
        suppress_parallel_ts2339_cascade_diagnostics(&arena, &mut diagnostics);
        // The TS2339 has no parent `VARIABLE_DECLARATION` in the empty arena,
        // so no AST traversal can link it to the TS2454 — nothing should be removed.
        assert_eq!(diagnostics.len(), original_len);
    }
// TSZ_INLINE_TEST_END 5338fae54c5e6014f38bab3ad9267dd3232d8cea516c78925c51ab63371c9fa9

// TSZ_INLINE_TEST_BEGIN 546372cb2c01a318cd54b00d841c7391a5c5c22643f25c94924a03d6e6693d03 1429 ts2339_suppression_empty_diagnostics
    #[test]
    fn ts2339_suppression_empty_diagnostics() {
        let arena = NodeArena::default();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        suppress_parallel_ts2339_cascade_diagnostics(&arena, &mut diagnostics);
        assert!(diagnostics.is_empty());
    }
// TSZ_INLINE_TEST_END 546372cb2c01a318cd54b00d841c7391a5c5c22643f25c94924a03d6e6693d03
