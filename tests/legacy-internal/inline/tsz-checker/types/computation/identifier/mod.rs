//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/computation/identifier/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 83f12eebb0dd55a3e239513baa2f5eea2bcfb5b85f1e1c6eb67d1781b7f9b364 20 ts1212_expression_usage_of_strict_mode_reserved_word
    /// TS1212 must fire when a strict-mode reserved word is used as an expression.
    /// In ESM (.ts files), strict mode is always on, so `var interface = 1; interface;`
    /// should emit TS1212 at the expression usage of `interface`.
    #[test]
    fn ts1212_expression_usage_of_strict_mode_reserved_word() {
        let codes = check_source_codes("var interface = 1;\ninterface;");
        assert!(
            codes.contains(&1212),
            "Expected TS1212 for expression usage of `interface`: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 83f12eebb0dd55a3e239513baa2f5eea2bcfb5b85f1e1c6eb67d1781b7f9b364

// TSZ_INLINE_TEST_BEGIN 3568fed7d1a1fa173bc8caf5ba1a9cd561199a2d3a8b9c9a1bcc0ec10dd718b6 30 ts1212_all_reserved_words_in_expression
    /// All strict-mode reserved words should trigger TS1212 at expression position.
    #[test]
    fn ts1212_all_reserved_words_in_expression() {
        for word in &[
            "implements",
            "interface",
            "let",
            "package",
            "private",
            "protected",
            "public",
            "static",
            "yield",
        ] {
            let source = format!("var {word} = 1;\n{word};");
            let codes = check_source_codes(&source);
            assert!(
                codes.contains(&1212),
                "Expected TS1212 for expression usage of `{word}`: {codes:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END 3568fed7d1a1fa173bc8caf5ba1a9cd561199a2d3a8b9c9a1bcc0ec10dd718b6

// TSZ_INLINE_TEST_BEGIN 3c6242e88294295dd5a62a916b5a5bfe0974d64d2c61beda9c1a0d65e0981831 53 no_ts1212_for_regular_identifiers
    /// Non-reserved identifiers should NOT get TS1212.
    #[test]
    fn no_ts1212_for_regular_identifiers() {
        let codes = check_source_codes("var foo = 1;\nfoo;");
        assert!(
            !codes.contains(&1212),
            "Should not emit TS1212 for regular identifier: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 3c6242e88294295dd5a62a916b5a5bfe0974d64d2c61beda9c1a0d65e0981831

// TSZ_INLINE_TEST_BEGIN 5d5f3593faff55de1100ccb353aabffc092eec448710940c45ee3664662076aa 67 ts1361_type_only_import_in_value_computed_property
    /// TS1361 must fire when a type-only import is used in a value position
    /// (object literal computed property name). Ensures that
    /// `source_file_has_value_import_binding_named` correctly checks
    /// `ImportClauseData::is_type_only` (not `ImportDeclData::is_type_only`,
    /// which is always false for regular import declarations).
    #[test]
    fn ts1361_type_only_import_in_value_computed_property() {
        let codes = check_source_codes(
            r#"
import type { onInit } from './hooks';
const o = { [onInit]: 0 };
"#,
        );
        assert!(
            codes.contains(&1361),
            "Expected TS1361 for type-only import used in object literal computed property: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 5d5f3593faff55de1100ccb353aabffc092eec448710940c45ee3664662076aa

// TSZ_INLINE_TEST_BEGIN eea90aaa338996f848426757e107eb9c0ffb8d948d4f437cbf89765988d34e50 84 no_ts1361_for_regular_import_with_same_name
    /// TS1361 must NOT fire when a regular (non-type-only) import is used
    /// in value position. The value import binding shadows any type-only
    /// import of the same name.
    #[test]
    fn no_ts1361_for_regular_import_with_same_name() {
        let codes = check_source_codes(
            r#"
import { onInit } from './hooks';
const o = { [onInit]: 0 };
"#,
        );
        assert!(
            !codes.contains(&1361),
            "Should not emit TS1361 for regular (non-type-only) import: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END eea90aaa338996f848426757e107eb9c0ffb8d948d4f437cbf89765988d34e50

// TSZ_INLINE_TEST_BEGIN 76e3d245ca764b6aeed6577b2b1e1f811a0463b2c45967ff22accd0bc2888eaf 100 ts1361_respects_per_specifier_type_only
    /// When `import { type Foo }` is used, `Foo` is type-only per-specifier.
    /// Using `Foo` in a value position should emit TS1361.
    #[test]
    fn ts1361_respects_per_specifier_type_only() {
        let codes = check_source_codes(
            r#"
import { type Foo } from './hooks';
let x = Foo;
"#,
        );
        assert!(
            codes.contains(&1361),
            "Expected TS1361 for per-specifier type-only import used as value: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 76e3d245ca764b6aeed6577b2b1e1f811a0463b2c45967ff22accd0bc2888eaf
