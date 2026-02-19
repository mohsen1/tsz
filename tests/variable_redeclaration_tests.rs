use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
#[ignore = "Regression: TS2403 not being emitted for incompatible var redeclarations"]
fn test_variable_redeclaration_incompatible() {
    let source = r#"
var x = 1;
var x = "string"; // TS2403
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
    assert!(
        codes.contains(&2403),
        "Expected TS2403 for incompatible redeclaration, got: {:?}",
        codes
    );
}

#[test]
fn test_variable_redeclaration_compatible() {
    let source = r#"
var x = 1;
var x = 2; // OK
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
    assert!(
        !codes.contains(&2403),
        "Unexpected TS2403 for compatible redeclaration, got: {:?}",
        codes
    );
}
