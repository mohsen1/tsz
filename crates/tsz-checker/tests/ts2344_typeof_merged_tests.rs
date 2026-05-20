//! Tests for TS2344 false positives with typeof and merged type/value symbols.

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
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
