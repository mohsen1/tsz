//! Tests for literal-return widening in generator return-type inference.
//!
//! `function*() { return 1; }` should infer `Generator<never, number, any>`
//! (widened), not `Generator<never, 1, any>`. Matches tsc's async wrapper
//! widening applied in the same file.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

const GENERATOR_STUBS: &str = r#"
interface IteratorYieldResult<TYield> { done?: false; value: TYield; }
interface IteratorReturnResult<TReturn> { done: true; value: TReturn; }
type IteratorResult<T, TReturn = any> = IteratorYieldResult<T> | IteratorReturnResult<TReturn>;
interface Iterator<T, TReturn = any, TNext = undefined> {
    next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    return?(value?: TReturn): IteratorResult<T, TReturn>;
    throw?(e?: any): IteratorResult<T, TReturn>;
}
interface Iterable<T, TReturn = unknown, TNext = unknown> {
    [Symbol.iterator](): Iterator<T, TReturn, TNext>;
}
interface IterableIterator<T, TReturn = any, TNext = undefined> extends Iterator<T, TReturn, TNext> {
    [Symbol.iterator](): IterableIterator<T, TReturn, TNext>;
}
interface Generator<T = unknown, TReturn = any, TNext = any> extends IterableIterator<T, TReturn, TNext> {
    next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    return(value: TReturn): IteratorResult<T, TReturn>;
    throw(e: any): IteratorResult<T, TReturn>;
    [Symbol.iterator](): Generator<T, TReturn, TNext>;
}
"#;

fn get_diagnostics(user_source: &str) -> Vec<(u32, String)> {
    let full_source = format!("{GENERATOR_STUBS}\n{user_source}");
    let mut parser = ParserState::new("test.ts".to_string(), full_source);
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        Default::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn generator_return_literal_widens_tresult_to_number() {
    // Assignment to a non-Generator-typed target produces a TS2322 whose
    // source type is the synthesized `Generator<T, R, N>` from the body.
    // With the fix, the literal `1` widens to `number`, so the error
    // message shows `Generator<..., number, ...>` (not `..., 1, ...`).
    let source = r#"
const g: number = function*() { return 1; }();
"#;
    let diags = get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for generator assigned to number, got: {diags:?}"
    );
    // The message refers to the body-inferred return type. The widened form
    // should mention `number` (the widened type) not just `1` (literal).
    let has_number = ts2322.iter().any(|(_, m)| m.contains("number"));
    let has_only_literal_one = ts2322
        .iter()
        .all(|(_, m)| m.contains(" 1,") || m.contains(" 1>"));
    assert!(
        has_number,
        "Expected TS2322 message to mention widened 'number' TReturn, got: {ts2322:?}"
    );
    assert!(
        !has_only_literal_one,
        "TS2322 message should not preserve literal '1' as TReturn after widening: {ts2322:?}"
    );
}

#[test]
fn generator_return_literal_string_widens_to_string() {
    let source = r#"
const g: number = function*() { return "hello"; }();
"#;
    let diags = get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for generator returning literal string assigned to number, got: {diags:?}"
    );
    assert!(
        ts2322.iter().any(|(_, m)| m.contains("string")),
        "Expected TS2322 message to mention widened 'string' TReturn, got: {ts2322:?}"
    );
}
