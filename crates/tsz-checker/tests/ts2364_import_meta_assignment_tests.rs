//! Tests for TS2364: The left-hand side of an assignment expression must be a
//! variable or a property access.
//!
//! Specifically tests that `import.meta = ...` is rejected (TS2364) while
//! `import.meta.prop = ...` is allowed (it's a real property access).

use tsz_binder::BinderState;
use tsz_checker::state::CheckerState;
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
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_error_codes(source).contains(&code)
}

#[test]
fn test_import_meta_direct_assignment_is_invalid() {
    // `import.meta = foo` must emit TS2364 because import.meta itself
    // is not a valid assignment target (it's a meta-property, not a variable
    // or property access).
    let source = r#"
const foo: any = {};
import.meta = foo;
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Should emit TS2364 for direct assignment to import.meta"
    );
}

#[test]
fn test_import_meta_property_assignment_is_valid() {
    // `import.meta.foo = value` is fine — it's a property access on import.meta
    let source = r#"
import.meta.foo = 42;
"#;
    let codes = get_error_codes(source);
    assert!(
        !codes.contains(&2364),
        "Should NOT emit TS2364 for assignment to import.meta.foo; got codes: {:?}",
        codes
    );
}

#[test]
fn test_import_meta_compound_assignment_is_invalid() {
    // `import.meta += foo` must also emit TS2364
    let source = r#"
import.meta += 1;
"#;
    assert!(
        has_error_with_code(source, 2364),
        "Should emit TS2364 for compound assignment to import.meta"
    );
}
