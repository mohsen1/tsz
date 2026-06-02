//! TS2322 diagnostic coverage for DOM-call-heavy files where global resolution
//! fuel is exhausted after the first large-lib type graph materialisation.
//!
//! Structural rule: when a file contains N assignments from DOM call expressions
//! whose return types are all incompatible with the declared variable type, `tsc`
//! reports N TS2322 diagnostics — one per assignment.  A global-fuel exhaustion
//! guard that gated the *entire* `ensure_relation_input_ready` step caused tsz to
//! report only the first diagnostic (issue #12144).
//!
//! These tests vary the DOM methods, tag string spellings, and variable names so
//! the fix is proven structural (not name-keyed or limited to one tag).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_lib_files};

fn dom_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts", "dom.iterable.d.ts"]);
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// Two consecutive DOM calls with incompatible targets must each produce TS2322.
///
/// Before the fix, only the first call fired; the second was silently dropped
/// because the first materialization exhausted the global resolution fuel and
/// the outer guard in `ensure_relation_input_ready` short-circuited the second
/// check entirely.
///
/// Variable names are varied (`x1`/`x2`) to prove the fix is not
/// spelling-dependent.
#[test]
fn two_dom_calls_both_report_ts2322() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const x1: number = d.createElement("div");
const x2: number = d.createElement("span");
export {};
"#,
    );
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "both DOM-call assignments must produce TS2322, got codes {codes:?}",
    );
}

/// Two calls to different DOM methods — `createElement` and `querySelector` —
/// must each produce TS2322.  Varying the method proves the fix is not specific
/// to one overload family.
#[test]
fn create_element_and_query_selector_both_report_ts2322() {
    let codes = dom_codes(
        r#"
declare const doc: Document;
const a: number = doc.createElement("p");
const b: number = doc.querySelector("x");
export {};
"#,
    );
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "createElement and querySelector must each produce TS2322, got {codes:?}",
    );
}

/// Full seven-call repro from issue #12144.  All seven DOM-call assignments
/// must produce TS2322, not just the first.
///
/// Variable names follow the issue repro exactly so this test documents the
/// motivating case, then a renamed variant below proves the rule is structural.
#[test]
fn seven_dom_calls_all_report_ts2322() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const a1: number = d.createElement("div");
const a2: number = d.createElement("span");
const a3: number = d.createElement("a");
const a4: number = d.createElement("p");
const a5: number = d.createElement("img");
const a6: number = d.querySelector("x");
const a7: number = d.getElementById("y");
export {};
"#,
    );
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 7,
        "all seven DOM-call assignments must produce TS2322, got {codes:?}",
    );
}

/// Same seven-call repro with renamed variables and a renamed document binding.
/// Proves the fix follows the structural shape (N incompatible DOM-call results)
/// rather than any particular identifier spelling.
#[test]
fn seven_dom_calls_all_report_ts2322_renamed() {
    let codes = dom_codes(
        r#"
declare const myDoc: Document;
const r1: number = myDoc.createElement("section");
const r2: number = myDoc.createElement("article");
const r3: number = myDoc.createElement("header");
const r4: number = myDoc.createElement("footer");
const r5: number = myDoc.createElement("main");
const r6: number = myDoc.querySelector("y");
const r7: number = myDoc.getElementById("z");
export {};
"#,
    );
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 7,
        "renamed seven-call repro must also produce 7 TS2322s, got {codes:?}",
    );
}

/// Negative/fallback case: assignments that are *correct* must not produce
/// TS2322 even in a DOM-call-heavy file where fuel has been exhausted.
/// The fix must not introduce spurious diagnostics on valid expressions.
#[test]
fn correct_assignments_after_dom_calls_produce_no_ts2322() {
    let codes = dom_codes(
        r#"
declare const d: Document;
const bad: number = d.createElement("div");
const ok: Element | null = d.querySelector("x");
export {};
"#,
    );
    // `bad` gets TS2322; `ok` is correct.
    assert!(
        codes.contains(&2322),
        "incorrect assignment should produce TS2322, got {codes:?}",
    );
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "correct assignment after DOM call must not gain a spurious TS2322, got {codes:?}",
    );
}
