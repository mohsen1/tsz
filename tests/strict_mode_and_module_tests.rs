use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_always_strict_with_statement() {
    let source = r#"
// @alwaysStrict: true
with (x) {}
"#;
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
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Should have TS1101 (with not allowed in strict mode)
    assert!(
        codes.contains(&1101),
        "Expected TS1101 for 'with' in alwaysStrict mode, got: {codes:?}"
    );
}

#[test]
fn test_anonymous_module_detailed_error() {
    let source = r#"
module {
    export var x = 1;
}
"#;
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
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Anonymous modules should NOT emit TS2591 — the parser already emits TS1437.
    // tsc does not additionally suggest @types/node for anonymous module declarations.
    assert!(
        !codes.contains(&2591),
        "Should NOT emit TS2591 for anonymous module (tsc only emits TS1437), got: {codes:?}"
    );
    // Verify the parser produced TS1437
    let parse_codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        parse_codes.contains(&1437),
        "Expected TS1437 from parser for anonymous module, got: {parse_codes:?}"
    );
}
