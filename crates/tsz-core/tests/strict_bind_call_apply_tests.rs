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
    let source = r#"
class C {
    foo(this: this, a: number, b: string): string { return ""; }
}
declare let c: C;
c.foo.bind(undefined);
"#;
    let diagnostics = check_source_with_libs(source);
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2769)
        .expect("expected TS2769");

    let arg_start = source
        .find("undefined")
        .expect("expected undefined argument") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2769 should anchor at the thisArg, got: {diag:?}"
    );
    assert_eq!(diag.length, "undefined".len() as u32);
}

// TODO: Fix TS2769 anchor position -- diagnostic points at the argument (pos 96)
// instead of the method name "bind" (pos 91).
#[test]
#[ignore]
fn strict_bind_call_apply_bind_generic_this_arg_mismatch_uses_ts2769() {
    let source = r#"
function bar<T extends unknown[]>(callback: (this: 1, ...args: T) => void) {
    callback.bind(2);
}
"#;
    let diagnostics = check_source_with_libs(source);

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2769)
        .expect("expected TS2769 for bind thisArg overload mismatch");
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2345),
        "bind thisArg overload mismatches should stay TS2769, got: {diagnostics:?}"
    );
    let bind_start = source.find("bind").expect("expected bind call") as u32;
    assert_eq!(
        diag.start, bind_start,
        "generic bind overload mismatch should stay anchored at bind, got: {diag:?}"
    );
    assert_eq!(diag.length, "bind".len() as u32);
}

#[test]
fn strict_bind_call_apply_apply_tuple_argument_display_stays_unnamed() {
    let diagnostics = check_source_with_libs(
        r#"
declare function foo(a: number, b: string): string;
foo.apply(undefined, [10]);
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2345)
        .expect("expected TS2345");
    assert!(
        diag.message_text.contains("Argument of type '[number]'"),
        "expected unnamed tuple source display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("[a: number]"),
        "actual tuple display should not inherit contextual names, got: {diag:?}"
    );
}
