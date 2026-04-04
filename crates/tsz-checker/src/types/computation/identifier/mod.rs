//! Identifier type computation for `CheckerState`.
//!
//! Resolves the type of identifier expressions by looking up symbols through
//! the binder, checking TDZ violations, validating definite assignment,
//! applying flow-based narrowing, and handling intrinsic/global names.

mod core;
mod resolution;

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_codes;

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

    /// Non-reserved identifiers should NOT get TS1212.
    #[test]
    fn no_ts1212_for_regular_identifiers() {
        let codes = check_source_codes("var foo = 1;\nfoo;");
        assert!(
            !codes.contains(&1212),
            "Should not emit TS1212 for regular identifier: {codes:?}"
        );
    }

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
}
