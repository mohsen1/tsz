//! The `await`-outside-async grammar check (TS1308) for a concise
//! (expression-bodied) arrow.
//!
//! Structural rule: when a function body is an expression rather than a
//! block, `tsc` still runs `checkAwaitExpression` over it, because it reaches
//! that check from the expression itself. tsz reaches the same check from a
//! fixed set of *statement*-level roots (`return`, `if`, expression
//! statement, `for await`, variable declaration, decorator argument, property
//! initializer), and a concise arrow body is none of them — it has no
//! `ReturnStatement` node at all — so nothing ever visited it and the
//! diagnostic was silently dropped. tsz now roots the same scan at the
//! concise body in `check_function_type_impl`, inside the body-checking
//! region where `function_depth` and the function's own async context are
//! already established.
//!
//! Every expectation below is pinned against a real
//! `tsc@7.0.2 --noEmit --strict --pretty false --target es2017` run, not
//! recalled. Tracker: #16059.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

/// Diagnostics for `source`, minus the TS2318 missing-default-lib noise.
fn diagnostic_codes(source: &str) -> Vec<u32> {
    let lib_files = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &lib_files)
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .map(|diagnostic| diagnostic.code)
        .collect()
}

/// The #16059 witness: a concise arrow body at top level of a script.
///
/// tsc: `FILE(1,29): error TS1308`.
#[test]
fn concise_arrow_body_await_reports_ts1308() {
    let codes = diagnostic_codes("const inner = (): number => await 1;\n");
    assert_eq!(
        codes,
        vec![1308],
        "a concise arrow body must be scanned for the await grammar check"
    );
}

/// The block-bodied sibling, which already worked. Kept as the control that
/// pins the two forms to the same answer — the whole defect was the two
/// disagreeing.
///
/// tsc: `FILE(1,38): error TS1308`.
#[test]
fn block_arrow_body_await_reports_ts1308_control() {
    let codes = diagnostic_codes("const inner = (): number => { return await 1; };\n");
    assert_eq!(
        codes,
        vec![1308],
        "block-bodied control must keep reporting TS1308"
    );
}

/// Renamed binder: the check keys off the body's syntactic position, never
/// off the variable's name.
///
/// tsc: `FILE(1,37): error TS1308`.
#[test]
fn concise_arrow_body_await_reports_ts1308_with_renamed_binder() {
    let codes = diagnostic_codes("const somethingElse = (): number => await 1;\n");
    assert_eq!(codes, vec![1308], "binder name must not affect the check");
}

/// The `await` nested inside a larger body expression rather than being the
/// whole body — the scan must walk into the expression, not just test its
/// root kind.
///
/// tsc: `FILE(1,33): error TS1308`.
#[test]
fn concise_arrow_body_await_nested_in_larger_expression_reports_ts1308() {
    let codes = diagnostic_codes("const inner = (): number => 1 + await 2;\n");
    assert_eq!(
        codes,
        vec![1308],
        "the scan must descend into the body expression"
    );
}

/// A concise body reached through an object literal's property initializer
/// rather than a variable declaration — an arrow whose owning position is a
/// different one of the checker's scan roots.
///
/// tsc: `FILE(1,32): error TS1308`.
#[test]
fn concise_arrow_body_await_in_object_literal_property_reports_ts1308() {
    let codes = diagnostic_codes("const obj = { m: (): number => await 1 };\n");
    assert_eq!(
        codes,
        vec![1308],
        "an arrow in a property initializer must be scanned too"
    );
}

/// Top level of a *module*. Being in a module licenses top-level `await`, but
/// this `await` is not at top level — it is inside a function — so TS1308
/// still applies. This is the case that a naive "is the file a module" fix
/// would get wrong.
///
/// tsc: `FILE(1,36): error TS1308`.
#[test]
fn concise_arrow_body_await_at_module_top_level_still_reports_ts1308() {
    let codes = diagnostic_codes("export const inner = (): number => await 1;\n");
    assert_eq!(
        codes,
        vec![1308],
        "a module's top-level await allowance does not extend into a function body"
    );
}

/// A contextually typed callback arrow. This is the shape whose body
/// `check_function_type_impl` visits more than once — once while building the
/// type environment and again with contextual parameter types — so it is the
/// case that would double-report if the new scan root sat outside the
/// body-checking region's visit guards. The exact-equality assertion is the
/// point: one TS1308, not two.
///
/// tsc: `FILE(2,16): error TS1308`.
#[test]
fn concise_arrow_body_await_in_contextual_callback_reports_one_ts1308() {
    let codes = diagnostic_codes(
        "declare function run(cb: (x: number) => number): void;\nrun((x) => x + await 1);\n",
    );
    assert_eq!(
        codes,
        vec![1308],
        "a re-visited contextual callback body must report TS1308 exactly once"
    );
}

/// The generic sibling of the above, where inference re-enters the body with
/// freshly instantiated parameter types.
///
/// tsc: `FILE(2,34): error TS1308`.
#[test]
fn concise_arrow_body_await_in_generic_contextual_callback_reports_one_ts1308() {
    let codes = diagnostic_codes(
        "declare function map<T, U>(xs: T[], cb: (x: T) => U): U[];\nconst r = map([1, 2], (x) => x + await 1);\n",
    );
    assert_eq!(
        codes,
        vec![1308],
        "generic inference re-entry must not duplicate the grammar diagnostic"
    );
}

/// The negative control that a "always report on a concise body" fix fails:
/// an `async` arrow's concise body may contain `await`.
///
/// tsc: clean.
#[test]
fn concise_async_arrow_body_await_is_clean() {
    let codes = diagnostic_codes("const inner = async (): Promise<number> => await 1;\n");
    assert!(
        codes.is_empty(),
        "an async arrow's concise body legitimately awaits; got {codes:?}"
    );
}

/// The generic fallback: a concise body with no `await` anywhere must stay
/// silent, so the new scan root cannot report on its own.
///
/// tsc: clean.
#[test]
fn concise_arrow_body_without_await_is_clean() {
    let codes = diagnostic_codes("const inner = (): number => 1 + 2;\n");
    assert!(
        codes.is_empty(),
        "a concise body with no await must stay clean; got {codes:?}"
    );
}
// A concise body nested inside an *async* function (`async function outer() {
// const inner = (): number => await 1; }`) is TS1308 in tsc but stays silent
// in tsz even with this scan root in place — measured on this branch, both
// the `async function` and `async` arrow enclosing forms. That residue is a
// second, independent defect: `CheckerContext::async_depth` is a
// nesting-depth accumulator, so `in_async_context()` answers "is *any*
// enclosing function async", and the scan correctly reaches the `await` only
// to be told it is legal. PR #16058 scopes that flag to the immediately
// enclosing function. Deliberately not asserted here — this file must not
// encode the other PR's behavior, and #16059 records the pairing.
