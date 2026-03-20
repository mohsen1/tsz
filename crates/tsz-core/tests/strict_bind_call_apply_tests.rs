use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::TypeInterner;

fn check_source_with_libs(source: &str) -> Vec<crate::checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn strict_bind_call_apply_bind_this_arg_mismatch_uses_ts2769() {
    let diagnostics = check_source_with_libs(
        r#"
function bar<T extends unknown[]>(callback: (this: 1, ...args: T) => void) {
    callback.bind(2);
}
"#,
    );

    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        codes.contains(&2769),
        "expected TS2769 for bind thisArg overload mismatch, got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2345),
        "bind thisArg overload mismatches should stay TS2769, got: {diagnostics:?}"
    );
}
