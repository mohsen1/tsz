use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn await_using_in_non_async_function_emits_ts2852() {
    let codes = check_codes(
        r#"
function f() {
    await using x = getResource();
}
"#,
    );

    assert!(
        codes.contains(&2852),
        "expected TS2852 for await using inside a non-async function, got {codes:?}"
    );
}

#[test]
fn await_using_in_async_function_does_not_emit_ts2852() {
    let codes = check_codes(
        r#"
async function f() {
    await using x = getResource();
}
"#,
    );

    assert!(
        !codes.contains(&2852),
        "did not expect TS2852 for await using inside an async function, got {codes:?}"
    );
}

#[test]
fn await_using_in_async_function_with_await_context_does_not_emit_ts2852() {
    let codes = check_codes(
        r#"
async function f() {
    await Promise.resolve();
    await using x = getResource();
}
"#,
    );

    assert!(
        !codes.contains(&2852),
        "did not expect TS2852 when await-context bits accompany await using, got {codes:?}"
    );
}

#[test]
fn await_using_in_nested_non_async_function_under_async_parent_emits_ts2852() {
    let codes = check_codes(
        r#"
async function outer() {
    function inner() {
        await using x = getResource();
    }
}
"#,
    );

    assert!(
        codes.contains(&2852),
        "expected TS2852 for await using inside a nested non-async function, got {codes:?}"
    );
}

#[test]
fn top_level_await_using_in_script_emits_ts2853() {
    let codes = check_codes("await using x = getResource();");

    assert!(
        codes.contains(&2853),
        "expected TS2853 for top-level await using in a script file, got {codes:?}"
    );
}

#[test]
fn top_level_await_using_in_module_does_not_emit_ts2853() {
    let codes = check_codes(
        r#"
await using x = getResource();
export {};
"#,
    );

    assert!(
        !codes.contains(&2853),
        "did not expect TS2853 for top-level await using in a module, got {codes:?}"
    );
}
