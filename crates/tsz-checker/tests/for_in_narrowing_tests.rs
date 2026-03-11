use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
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
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn for_in_loop_body_preserves_outer_truthy_narrowing_through_initializer_assignment() {
    let source = r#"
const o: { [key: string]: string } | undefined = {};
if (o) {
    for (const key in o) {
        const value = o[key];
        value;
    }
}
"#;

    let codes = check_strict(source);
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 inside for-in body after outer truthy narrowing, got codes: {codes:?}"
    );
}
