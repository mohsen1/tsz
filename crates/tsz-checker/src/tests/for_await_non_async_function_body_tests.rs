//! Regression tests for TS1103 (`for await` outside an async function) in a
//! non-top-level, non-static-block function body — issue #16071.
//!
//! `check_for_await_statement` (`crates/tsz-checker/src/types/type_checking/
//! core_statement_checks.rs`) branches on `function_depth`: the `> 0` arm
//! handled only the class-static-block case (TS18038) and then returned
//! unconditionally, so every other function body — free functions, class
//! methods/constructors/accessors, object-literal methods, functions nested
//! inside an async one — fell through silently instead of reporting TS1103.
//! TS1103 is to `for await` exactly what TS1308 is to a bare `await`
//! expression's non-top-level arm.
//!
//! The class-member-body cases depend on #16070 (merged), which made
//! `ctx.function_depth` raise for method/constructor/accessor bodies; without
//! it those three shapes fall through to the top-level branch (TS1431/TS1432)
//! regardless of this fix.
//!
//! Every diagnostic this function can emit is also suppressed program-wide
//! when the program has a real syntax parse error, not just the current
//! file — the conformance fixture `parser.forAwait.es2018.ts` puts a
//! genuinely malformed `for await (const x in y) {}` (parser reports TS1005,
//! `'of' expected`) in one `@filename` block alongside an otherwise-valid
//! non-async `for await...of` in another, and `tsc` reports only the parse
//! error across the whole program. The suppression check below approximates
//! that with a single file (`has_parse_errors` is set per parsed unit in the
//! `check_source_with_parse_health` harness), which is enough to pin that a
//! parse error anywhere in the checked unit suppresses TS1103 for a `for
//! await` elsewhere in it.

use crate::test_utils::{check_source_codes, check_source_codes_with_parse_health};

/// Core repro from #16071: a non-async free function body.
#[test]
fn free_function_for_await_reports_ts1103() {
    let codes = check_source_codes(
        r#"
function f() {
    for await (const x of []) {}
}
"#,
    );
    assert!(
        codes.contains(&1103),
        "a `for await` in a non-async free function body must report TS1103; got {codes:?}"
    );
}

/// Class method body — the shape #16071 names explicitly.
#[test]
fn class_method_for_await_reports_ts1103() {
    let codes = check_source_codes(
        r#"
class K {
    m() {
        for await (const x of []) {}
    }
}
"#,
    );
    assert!(
        codes.contains(&1103),
        "a `for await` in a non-async class method body must report TS1103; got {codes:?}"
    );
}

/// Object-literal method body.
#[test]
fn object_literal_method_for_await_reports_ts1103() {
    let codes = check_source_codes(
        r#"
const o = {
    m() {
        for await (const x of []) {}
    },
};
"#,
    );
    assert!(
        codes.contains(&1103),
        "a `for await` in a non-async object-literal method body must report TS1103; got {codes:?}"
    );
}

/// Constructor body.
#[test]
fn constructor_for_await_reports_ts1103() {
    let codes = check_source_codes(
        r#"
class K {
    constructor() {
        for await (const x of []) {}
    }
}
"#,
    );
    assert!(
        codes.contains(&1103),
        "a `for await` in a non-async constructor body must report TS1103; got {codes:?}"
    );
}

/// Get-accessor body.
#[test]
fn accessor_for_await_reports_ts1103() {
    let codes = check_source_codes(
        r#"
class K {
    get v() {
        for await (const x of []) {}
        return 1;
    }
}
"#,
    );
    assert!(
        codes.contains(&1103),
        "a `for await` in a non-async accessor body must report TS1103; got {codes:?}"
    );
}

/// Renamed-binder control (anti-hardcoding): different identifiers, same shape.
#[test]
fn for_await_reports_ts1103_renamed_binders() {
    let codes = check_source_codes(
        r#"
function pollUntilSettled() {
    for await (const ready of []) {}
}
"#,
    );
    assert!(
        codes.contains(&1103),
        "renamed-binder `for await` must still report TS1103; got {codes:?}"
    );
}

/// A non-async function nested inside an async one must still report: the
/// grammar check is keyed on the innermost enclosing function's own async
/// context, not any enclosing function's.
#[test]
fn non_async_function_nested_in_async_still_reports_ts1103() {
    let codes = check_source_codes(
        r#"
async function outer() {
    function inner() {
        for await (const x of []) {}
    }
}
"#,
    );
    assert!(
        codes.contains(&1103),
        "a non-async function nested inside an async one must still report TS1103; got {codes:?}"
    );
}

/// Positive control: inside an actual async function, `for await` is legal
/// and must not report TS1103.
#[test]
fn async_function_for_await_is_clean() {
    let codes = check_source_codes(
        r#"
async function f() {
    for await (const x of []) {}
}
"#,
    );
    assert!(
        !codes.contains(&1103),
        "`for await` inside an async function must not report TS1103; got {codes:?}"
    );
}

/// Negative control: the already-correct class-static-block sibling (TS18038)
/// must be unaffected by the new `else` arm.
#[test]
fn class_static_block_for_await_still_reports_ts18038_not_ts1103() {
    let codes = check_source_codes(
        r#"
class K {
    static {
        for await (const x of []) {}
    }
}
"#,
    );
    assert!(
        codes.contains(&18038),
        "a `for await` in a class static block must still report TS18038; got {codes:?}"
    );
    assert!(
        !codes.contains(&1103),
        "a class static block's `for await` must not also report TS1103; got {codes:?}"
    );
}

/// A `for` loop with no `await` in the same non-async body must not report
/// anything from the new arm — the rooting must not synthesize a diagnostic
/// of its own.
#[test]
fn plain_for_of_no_await_is_clean_of_ts1103() {
    let codes = check_source_codes(
        r#"
function f() {
    for (const x of []) {}
}
"#,
    );
    assert!(
        !codes.contains(&1103),
        "a plain `for..of` with no `await` must not report TS1103; got {codes:?}"
    );
}

/// A real syntax parse error in the checked unit (`for await (... in ...)`,
/// which parses as TS1005 `'of' expected`, not TS1103 — `in` is not valid
/// after `for await`) suppresses TS1103 for an unrelated, otherwise-valid
/// non-async `for await...of` in the same unit. `parser.forAwait.es2018.ts`'s
/// exact shape: without this guard tsz reports the parse error plus a
/// spurious TS1103 that `tsc` never emits once the program has a parse error.
#[test]
fn parse_error_elsewhere_in_unit_suppresses_ts1103() {
    let codes = check_source_codes_with_parse_health(
        r#"
for await (const x in y) {
}
function f5() {
    let y: any;
    for await (const x of y) {
    }
}
"#,
    );
    assert!(
        codes.contains(&1005),
        "the malformed `for await (... in ...)` must still report the parse error; got {codes:?}"
    );
    assert!(
        !codes.contains(&1103),
        "a parse error elsewhere in the unit must suppress TS1103 for the otherwise-valid `for await`; got {codes:?}"
    );
}
