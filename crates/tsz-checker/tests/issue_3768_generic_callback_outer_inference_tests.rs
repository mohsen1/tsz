//! Regression tests for [issue #3768]: generic callbacks must be instantiated
//! from the outer call's contextual inference, not compared against their own
//! still-generic type parameter.
//!
//! Both repros from the issue exited 0 on tsc 6.0.3 but emitted spurious
//! TS2345 on tsz at the time the issue was filed. They now pass on main; these
//! tests lock in the fix so it cannot silently regress.
//!
//! [issue #3768]: https://github.com/mohsen1/tsz/issues/3768

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn generic_identity_callback_instantiates_from_outer_string_argument() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function map<T, U>(x: T, f: (s: T) => U): U;
declare function identity<V>(y: V): V;

var s = map("", identity);
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "Did not expect TS2345 — `identity` must be instantiated as `(s: string) => string` from outer `T = string`. Got: {diagnostics:?}"
    );
}

#[test]
fn generic_callback_in_object_property_instantiates_from_outer_rest_args() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function wrap<X>(x: X): { x: X };
declare function call<A extends unknown[], T>(x: { x: (...args: A) => T }, ...args: A): T;

const leak = call(wrap(<T>(x: T) => x), 1);
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "Did not expect TS2345 — the wrapped generic `<T>(x: T) => x` must be instantiated as `(x: number) => number` from outer `A = [number]`. Got: {diagnostics:?}"
    );
}
