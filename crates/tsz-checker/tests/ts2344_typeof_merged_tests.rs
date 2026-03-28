//! Tests for TS2344 false positives with typeof and merged type/value symbols.

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

/// TS2344 false positive: `typeof Input` against constraint when same-name
/// type alias exists (e.g., `type Input = GetValue<typeof Input>` + `const Input = ...`).
///
/// Root cause: For a merged `TYPE_ALIAS` + `VARIABLE` symbol, `get_type_of_symbol`
/// returns the type alias's `Lazy(DefId)` during circular resolution. But `typeof`
/// always refers to the value side, so the value declaration type should be used.
#[test]
fn test_no_false_ts2344_for_typeof_with_merged_type_alias_value() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

interface Base {
    value: unknown
}
interface Derived<T = any> extends Base {
    value: T
}
type GetValue<T extends Base> = T['value']

declare function makeDerived<T>(x: T): Derived<T>

type Input = GetValue<typeof Input>
const Input = makeDerived({ foo: 1 })
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for typeof Input against Base. Derived extends Base.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}
