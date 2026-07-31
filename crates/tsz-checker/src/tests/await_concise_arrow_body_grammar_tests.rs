//! Regression tests for TS1308 (`await` outside an async function) on a
//! concise (expression) arrow body.
//!
//! A concise body has no wrapping `ExpressionStatement`/`ReturnStatement`
//! node, so the `await`-grammar walk (`check_await_expression`) was never
//! invoked on it: `check_statement_with_request`'s dispatcher
//! (`crates/tsz-checker/src/statements.rs`) only ran the check from the
//! `ExpressionStatement` and `ReturnStatement` arms, and fell through to the
//! catch-all `_` arm for a bare expression body, which called
//! `get_type_of_node_with_request` alone. tsc's `checkAwaitExpression` walk
//! is reached from a concise arrow body exactly like any other expression, so
//! `(): number => await 1` reported `TS1308` in tsc but nothing in tsz. The
//! block-bodied form (`(): number => { return await 1; }`) already worked,
//! since its `return` statement carries its own check.

use crate::test_utils::check_source_codes;

/// #16059's exact repro: a non-async, non-nested concise arrow body with a
/// bare `await`. tsc reports TS1308.
#[test]
fn concise_arrow_body_top_level_await_reports_ts1308() {
    let source = r#"
const inner = (): number => await 1;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&1308),
        "a concise arrow body's `await` outside any async function must report TS1308; got {codes:?}"
    );
}

/// The already-working block-bodied sibling stays working: same shape, body
/// wrapped in braces with an explicit `return`.
#[test]
fn block_arrow_body_top_level_await_reports_ts1308() {
    let source = r#"
const inner = (): number => { return await 1; };
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&1308),
        "the block-bodied sibling already reported TS1308 before this fix; got {codes:?}"
    );
}

/// Renamed-binder control (anti-hardcoding): different identifier, different
/// literal value and return type, same shape.
#[test]
fn concise_arrow_body_await_reports_ts1308_renamed_binders() {
    let source = r#"
const computeFlag = (): boolean => await true;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&1308),
        "renamed-binder concise arrow body must still report TS1308; got {codes:?}"
    );
}

/// `await` nested inside a larger concise-body expression (not the direct
/// body), e.g. `1 + await 2`. The traversal must descend past the top-level
/// binary expression to find the await.
#[test]
fn concise_arrow_body_nested_await_reports_ts1308() {
    let source = r#"
const inner = (): number => 1 + await 2;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&1308),
        "a nested `await` inside a concise arrow body's expression must report TS1308; got {codes:?}"
    );
}

/// Fallback/positive control: a concise-bodied *async* arrow's own `await` is
/// legal, and must not report TS1308. Guards against a blanket fix that fires
/// on any concise body regardless of the immediately enclosing function.
#[test]
fn concise_async_arrow_body_await_is_clean() {
    let source = r#"
const inner = async (): Promise<number> => await Promise.resolve(1);
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&1308),
        "an async arrow's own concise-body await is legal; got {codes:?}"
    );
}

/// A concise arrow body nested *inside* another concise arrow body: the
/// traversal must not stop at the outer arrow's own boundary check and must
/// still reach the inner arrow when it is itself visited as a body.
#[test]
fn concise_arrow_body_nested_inside_another_concise_arrow_reports_ts1308() {
    let source = r#"
const outer = (): (() => number) => (): number => await 1;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&1308),
        "a concise arrow body nested inside another concise arrow body must still report its own TS1308; got {codes:?}"
    );
}
