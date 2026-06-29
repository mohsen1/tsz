//! Iterator next-send-type diagnostics (TS2763/TS2764/TS2765/TS2766) on
//! function-call-result iterables — regression for #14814.
//!
//! When the iterable is the result of a function call, tsz materializes the
//! call-result `Generator`/`AsyncGenerator` return type into a structural
//! object, which loses the `Generator<Y, R, N>` Application form that the
//! send-type check reads directly. Before the fix the four diagnostics were
//! silently dropped for call results while still firing for `declare const` /
//! explicitly-annotated iterables.
//!
//! tsc derives the send-type from the iterator's declared `TNext` type
//! argument, not from the materialized `next(...[v]: [] | [TNext])` rest-tuple
//! (whose optionality would otherwise suppress the diagnostic). The fix adds a
//! final fallback to the shared generator-argument extraction chain that
//! recovers the surviving Application through the `[Symbol.iterator]()` /
//! `[Symbol.asyncIterator]()` return type — so `TYield`/`TReturn`/`TNext` are
//! all recovered (`yield*` `TReturn` resolution benefits too), and:
//!   - Generator/AsyncGenerator call results report exactly as their
//!     `declare const` form does, and
//!   - non-generator iterables (`Set`, `Map`, `IterableIterator`, a user
//!     interface whose iterator factory returns a non-generator) report
//!     nothing, matching tsc.
//!
//! Verified verbatim (codes, spans, message text) against tsc 6.0.2.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

fn libs() -> Vec<Arc<LibFile>> {
    tsz_checker::test_utils::load_default_lib_files()
}

fn codes(source: &str, lib_files: &[Arc<LibFile>]) -> Vec<u32> {
    let diagnostics = tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        strict_options(),
        lib_files,
    );
    diagnostics
        .iter()
        .filter(|d| d.category == tsz_checker::diagnostics::DiagnosticCategory::Error)
        .map(|d| d.code)
        .collect()
}

/// A call-result `Generator<_, _, TNext>` whose `TNext` rejects `undefined`
/// must report the send-type mismatch at every iteration site, exactly as the
/// `declare const` form does. The producer function name is varied to prove the
/// rule is structural and not keyed to a particular binder.
#[test]
fn call_result_generator_reports_send_mismatch_at_every_site() {
    let libs = libs();
    let source = r#"
declare function buildSeq(): Generator<string, void, number>;
function consume() { for (const x of buildSeq()) { x; } }
const spread = [...buildSeq()];
const [first] = buildSeq();
"#;
    let got = codes(source, &libs);
    for code in [2763u32, 2764, 2765] {
        assert!(
            got.contains(&code),
            "expected TS{code} for a call-result Generator with TNext=number; got {got:?}"
        );
    }
}

/// `yield* call()` delegating to a generator whose `TNext` rejects the
/// containing generator's send type (`undefined`) reports TS2766.
#[test]
fn call_result_yield_star_reports_ts2766() {
    let libs = libs();
    let source = r#"
declare function makeInner(): Generator<string, void, number>;
function* outer(): Generator<string, void, undefined> { yield* makeInner(); }
"#;
    let got = codes(source, &libs);
    assert!(
        got.contains(&2766),
        "expected TS2766 for yield* of a call-result Generator; got {got:?}"
    );
}

/// The async family: for-await over a call-result `AsyncGenerator` reports
/// TS2763, and async `yield*` reports TS2766.
#[test]
fn call_result_async_generator_reports_send_mismatch() {
    let libs = libs();
    let source = r#"
declare function makeAsync(): AsyncGenerator<string, void, number>;
async function consume() { for await (const x of makeAsync()) { x; } }
async function* outer(): AsyncGenerator<string, void, undefined> { yield* makeAsync(); }
"#;
    let got = codes(source, &libs);
    assert!(
        got.contains(&2763),
        "expected TS2763 for for-await of a call-result AsyncGenerator; got {got:?}"
    );
    assert!(
        got.contains(&2766),
        "expected TS2766 for async yield* of a call-result AsyncGenerator; got {got:?}"
    );
}

/// Control: a call-result `Generator` whose `TNext` accepts `undefined`
/// (`undefined` itself, `unknown`, or a union containing `undefined`) must
/// stay clean — recovery must not over-report.
#[test]
fn call_result_generator_accepting_undefined_is_clean() {
    let libs = libs();
    let source = r#"
declare function gUndef(): Generator<string, void, undefined>;
declare function gUnknown(): Generator<string, void, unknown>;
declare function gOptional(): Generator<string, void, number | undefined>;
function a() { for (const x of gUndef()) { x; } }
function b() { for (const x of gUnknown()) { x; } }
function c() { for (const x of gOptional()) { x; } }
const s = [...gUnknown()];
"#;
    let got = codes(source, &libs);
    for code in [2763u32, 2764, 2765, 2766] {
        assert!(
            !got.contains(&code),
            "TS{code} must not fire when TNext accepts undefined; got {got:?}"
        );
    }
}

/// Control: non-generator call-result iterables (`Set`, `Map`,
/// `IterableIterator`) send `undefined` to a `TNext` of `unknown`, so no
/// send-type diagnostic is forced — the recovery only fires for generator-like
/// Applications.
#[test]
fn call_result_non_generator_iterables_are_clean() {
    let libs = libs();
    let source = r#"
declare function makeSet(): Set<number>;
declare function makeMap(): Map<string, number>;
declare function makeII(): IterableIterator<string>;
function a() { for (const x of makeSet()) { x; } }
function b() { for (const x of makeMap()) { x; } }
function c() { for (const x of makeII()) { x; } }
const s = [...makeSet()];
const [m] = makeMap();
const i = [...makeII()];
"#;
    let got = codes(source, &libs);
    for code in [2763u32, 2764, 2765, 2766] {
        assert!(
            !got.contains(&code),
            "TS{code} must not fire for non-generator call-result iterables; got {got:?}"
        );
    }
}

/// Control: a user iterable whose iterator factory returns a non-generator with
/// an optional `next(...args: [] | [N])` accepts a no-argument `next()`, so tsc
/// (and tsz) report nothing. The recovery must not synthesize a `TNext` from
/// the optional rest-tuple.
#[test]
fn call_result_optional_rest_next_interface_is_clean() {
    let libs = libs();
    let source = r#"
interface OptIter {
    [Symbol.iterator](): OptIter;
    next(...args: [] | [boolean]): IteratorResult<number, void>;
}
declare function makeOpt(): OptIter;
function a() { for (const x of makeOpt()) { x; } }
const s = [...makeOpt()];
"#;
    let got = codes(source, &libs);
    for code in [2763u32, 2764, 2765, 2766] {
        assert!(
            !got.contains(&code),
            "TS{code} must not fire for an optional-rest next() iterable; got {got:?}"
        );
    }
}

/// The recovery is wired into the shared generator-argument fallback chain, so
/// it also restores `TReturn` extraction for `yield*` of a call result: the
/// result of `yield* makeG()` is the delegated generator's `TReturn`. tsc
/// resolves it from the call result, so a downstream type mismatch must report
/// TS2322 just as it does for an annotated generator.
#[test]
fn call_result_yield_star_return_type_is_recovered() {
    let libs = libs();
    let source = r#"
declare function makeG(): Generator<string, number, undefined>;
function* g() {
    const r = yield* makeG();
    const bad: string = r;
}
"#;
    assert!(
        codes(source, &libs).contains(&2322),
        "yield* of a call-result Generator must resolve its TReturn (number) so the \
         string annotation reports TS2322"
    );
}

/// The `declare const` / annotated forms already reported correctly; keep them
/// as a parity anchor so the call-result fix is not confused with a change to
/// the direct-Application path.
#[test]
fn declared_and_annotated_forms_still_report() {
    let libs = libs();
    let declared = r#"
declare const g: Generator<string, void, number>;
function consume() { for (const x of g) { x; } }
"#;
    assert!(
        codes(declared, &libs).contains(&2763),
        "declared-const Generator must still report TS2763"
    );

    let annotated = r#"
declare function build(): Generator<string, void, number>;
const it: Generator<string, void, number> = build();
function consume() { for (const x of it) { x; } }
"#;
    assert!(
        codes(annotated, &libs).contains(&2763),
        "explicitly-annotated Generator must still report TS2763"
    );
}
