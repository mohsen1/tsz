// Regression tests for https://github.com/mohsen1/tsz/issues/3787 — under
// `--noLib` an async function declaration must NOT trigger TS2318 ("Cannot
// find global type 'Promise'") even though the `Promise` type isn't
// available. With `noLib`, the user owns the global type surface; tsc skips
// the check.

use tsz_checker::context::{CheckerOptions, ScriptTarget};

fn promise_2318_diagnostics(source: &str, no_lib: bool) -> Vec<String> {
    tsz_checker::test_utils::check_source(
        source,
        "a.ts",
        CheckerOptions {
            no_lib,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2318 && d.message_text.contains("Promise"))
    .map(|d| d.message_text)
    .collect()
}

#[test]
fn async_function_under_no_lib_does_not_emit_ts2318_for_promise() {
    // Issue's repro: with `--noLib`, defining an async function must not
    // emit TS2318 for `Promise`. Other globals are intentionally missing
    // under noLib (Array, Boolean, etc.) — those TS2318s are expected and
    // unrelated; we filter to just Promise.
    let source = "async function f() {}\n";
    let promise_diags = promise_2318_diagnostics(source, /* no_lib */ true);
    assert!(
        promise_diags.is_empty(),
        "TS2318(Promise) must not fire under --noLib, got {promise_diags:?}"
    );
}

#[test]
fn async_function_with_lib_still_emits_ts2318_when_promise_missing() {
    // Without `--noLib`, the existing TS2318-when-Promise-missing behavior
    // is preserved. The test environment does not load lib files, so
    // `Promise` is genuinely missing — the Promise-specific check must
    // still fire.
    let source = "async function f() {}\n";
    let promise_diags = promise_2318_diagnostics(source, /* no_lib */ false);
    assert!(
        !promise_diags.is_empty(),
        "TS2318(Promise) should still fire when Promise is missing and noLib is unset"
    );
}
