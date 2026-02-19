use crate::parser::ParserState;
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::checker::context::CheckerOptions;
use tsz_solver::TypeInterner;

#[test]
fn test_overload_compatibility_parameter_property() {
    let source = r#"
class C1 {
    constructor(public p1: string);
    constructor(public p3: any) {}
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
    // TSC does not report TS2394 here.
    assert!(
        !codes.contains(&2394),
        "Unexpected TS2394 for overload compatibility with parameter property, got: {:?}",
        codes
    );
}
