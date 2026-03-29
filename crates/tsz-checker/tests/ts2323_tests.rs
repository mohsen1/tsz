//! Tests for TS2323: Cannot redeclare exported variable

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_error_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// Without lib files, the full export flow can't be tested, but we can verify
/// that non-exported duplicates do NOT produce TS2323.
#[test]
fn test_non_exported_var_redeclaration_not_ts2323() {
    let codes = get_error_codes("var Foo = 2;\nvar Foo = 42;");
    assert!(
        !codes.contains(&2323),
        "Should NOT emit TS2323 for non-exported variables, got: {codes:?}"
    );
}

/// Exported class duplicates should emit TS2300, not TS2323.
/// (TS2323 is only for exported variables.)
#[test]
fn test_exported_class_redeclaration_not_ts2323() {
    let codes = get_error_codes("export class Foo {}\nexport class Foo {}");
    assert!(
        !codes.contains(&2323),
        "Should NOT emit TS2323 for exported classes (only for variables/functions), got: {codes:?}"
    );
}

/// Multiple `export default function()` with bodies should emit TS2393.
#[test]
fn test_duplicate_default_export_function_emits_ts2393() {
    let codes = get_error_codes(
        "export default interface A { a: string; }\nexport default function() { return 1; }\nexport default function() { return 2; }",
    );
    let ts2393_count = codes.iter().filter(|&&c| c == 2393).count();
    assert_eq!(
        ts2393_count, 2,
        "Should emit TS2393 on each duplicate default function implementation, got codes: {codes:?}"
    );
}

/// Single `export default function()` should NOT emit TS2393.
#[test]
fn test_single_default_export_function_no_ts2393() {
    let codes = get_error_codes("export default function() { return 1; }");
    assert!(
        !codes.contains(&2393),
        "Should NOT emit TS2393 for a single default export function, got: {codes:?}"
    );
}
