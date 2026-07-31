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

// --- Exactly-once and additional-position coverage (#16062) ---
//
// The cases above assert `codes.contains(&1308)`, which is satisfied by one
// diagnostic or by five. The rooting that landed for #16061 sits in
// `check_statement_with_request`'s catch-all arm, and `check_function_type_impl`
// visits a contextually typed callback's body more than once — once while
// building the type environment, then again with contextual parameter types.
// It reports once today; nothing pinned that, so a future change to the
// visit path could start double-reporting and every test above would still
// pass. These count the diagnostics instead.
//
// Expectations pinned against a live `tsc@7.0.2 --noEmit --strict
// --pretty false --target es2017` run, not recalled.

/// How many TS1308s `source` produces.
fn ts1308_count(source: &str) -> usize {
    check_source_codes(source)
        .into_iter()
        .filter(|code| *code == 1308)
        .count()
}

/// A contextually typed callback: the body is visited twice. tsc reports one
/// TS1308, so tsz must report exactly one.
#[test]
fn concise_arrow_body_await_in_contextual_callback_reports_exactly_one_ts1308() {
    let count = ts1308_count(
        r#"
declare function take(cb: () => number): void;
take((): number => await 1);
"#,
    );
    assert_eq!(
        count, 1,
        "a twice-visited contextual callback body must report TS1308 exactly once"
    );
}

/// The generic sibling, where inference re-enters the body with freshly
/// instantiated parameter types — a second, distinct re-visit path.
#[test]
fn concise_arrow_body_await_in_generic_contextual_callback_reports_exactly_one_ts1308() {
    let count = ts1308_count(
        r#"
declare function map<T, U>(xs: T[], cb: (x: T) => U): U[];
const r = map([1, 2], (x) => x + await 1);
"#,
    );
    assert_eq!(
        count, 1,
        "generic inference re-entry must not duplicate the grammar diagnostic"
    );
}

/// A concise body reached through an object-literal property initializer
/// rather than a variable declaration — a different owning position for the
/// arrow, and one no case above exercises.
///
/// tsc: `FILE(2,32): error TS1308`.
#[test]
fn concise_arrow_body_await_in_object_literal_property_reports_exactly_one_ts1308() {
    let count = ts1308_count(
        r#"
const obj = { m: (): number => await 1 };
"#,
    );
    assert_eq!(
        count, 1,
        "an arrow in a property initializer must be scanned exactly once"
    );
}

/// Top level of a **module**.
///
/// #16061 recorded this case as out of scope on the premise that "TS1431 /
/// TS1432 apply there instead, a different rule entirely". That premise does
/// not hold for this shape: those rules govern a `for await` / top-level
/// `await` *at* the module's top level, and this `await` is not at top level
/// — it is inside a function body, so the ordinary TS1308 rule applies and a
/// module's top-level-await allowance does not reach it.
///
/// Verified against the oracle rather than argued:
/// `tsc@7.0.2` on `export const inner = (): number => await 1;` reports
/// `FILE(1,36): error TS1308` — the same diagnostic as the script case, not
/// TS1431 or TS1432.
#[test]
fn concise_arrow_body_await_at_module_top_level_reports_ts1308_not_ts1431() {
    let codes = check_source_codes(
        r#"
export const inner = (): number => await 1;
"#,
    );
    assert!(
        codes.contains(&1308),
        "being in a module does not license `await` inside a function body; got {codes:?}"
    );
    assert!(
        !codes.contains(&1431) && !codes.contains(&1432),
        "the top-level-await rules do not apply to an await inside a function; got {codes:?}"
    );
}

/// The fallback control: a concise body with no `await` anywhere must stay
/// clean, so the new rooting cannot report on its own.
///
/// tsc: clean.
#[test]
fn concise_arrow_body_without_await_reports_no_ts1308() {
    let count = ts1308_count(
        r#"
const inner = (): number => 1 + 2;
"#,
    );
    assert_eq!(count, 0, "a concise body with no await must stay clean");
}

// --- The #16058 x #16061 intersection ---
//
// A concise body nested inside an *async* function needs both fixes and
// could not be asserted by either PR alone. #16061 supplied the traversal
// root; without #16058 the scan arrived and was told the `await` was legal,
// because `CheckerContext::async_depth` was a nesting-depth accumulator and
// `in_async_context()` answered "is *any* enclosing function async" rather
// than "is the function that owns this node async". #16059 named this
// pairing and #16058's own case 6 used a block body to route around the
// missing root. Both are on main now, so the case is finally pinnable — and
// it is the one that regresses if either fix is later reverted or narrowed.

/// A plain arrow with a concise body, nested inside an `async function`.
///
/// tsc: `FILE(3,33): error TS1308`.
#[test]
fn concise_arrow_body_await_nested_in_async_function_reports_ts1308() {
    let count = ts1308_count(
        r#"
async function outer(): Promise<number> {
    const inner = (): number => await 1;
    return inner();
}
"#,
    );
    assert_eq!(
        count, 1,
        "the enclosing function's asyncness must not license `await` in a nested plain arrow"
    );
}

/// The same, with an `async` *arrow* as the enclosing function — the second
/// of the two body-entry paths that maintain the async flag.
///
/// tsc: `FILE(3,33): error TS1308`.
#[test]
fn concise_arrow_body_await_nested_in_async_arrow_reports_ts1308() {
    let count = ts1308_count(
        r#"
const outer = async (): Promise<number> => {
    const inner = (): number => await 1;
    return inner();
};
"#,
    );
    assert_eq!(
        count, 1,
        "an enclosing async arrow must not license `await` in a nested plain arrow"
    );
}
