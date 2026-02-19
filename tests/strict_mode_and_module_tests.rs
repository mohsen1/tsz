use crate::parser::ParserState;
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::checker::context::CheckerOptions;
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
        "Expected TS1101 for 'with' in alwaysStrict mode, got: {:?}",
        codes
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
    // Should have TS2591 (detailed node types error)
    assert!(
        codes.contains(&2591),
        "Expected TS2591 for anonymous module, got: {:?}",
        codes
    );
}
