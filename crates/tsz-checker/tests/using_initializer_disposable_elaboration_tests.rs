//! Regression tests for TS2850 / TS2854 (`using` / `await using` initializer
//! must be disposable) *elaboration*.
//!
//! Structural rule: when the initializer of a `using` (resp. `await using`)
//! declaration is not disposable, tsc reports the grammar-level top line
//! (`The initializer of a 'using' declaration must be either an object with a
//! '[Symbol.dispose]()' method, or be 'null' or 'undefined'.`) followed by the
//! *same* relation-reason elaboration it produces for the equivalent
//! assignment to the global `Disposable` (resp. `AsyncDisposable`) interface —
//! the missing-`[Symbol.dispose]` frame, or, for a union source, the offending
//! member's `Type 'X' is not assignable to type 'Disposable'.` frame.
//!
//! Previously tsz emitted only the flat top line and dropped the tail. The fix
//! routes the disposability failure through the shared `relation -> reason ->
//! diagnostic` gateway (`analyze_assignability_failure` against the global
//! `Disposable`/`AsyncDisposable`), so the chain matches tsc. The disposability
//! *decision* is unchanged (`type_has_disposable_method`); only the display is
//! enriched.
//!
//! The rule is structural (independent of identifier spelling), so the cases
//! below vary the binder name of the initializer.

use std::sync::Arc;

use tsz_binder::lib_loader::LibFile;

use crate::test_utils::{
    DEFAULT_LIB_NAMES, check_source_with_libs, load_lib_files, strict_checker_options,
};

/// Lib bundle that includes the explicit-resource-management typings
/// (`Disposable`, `AsyncDisposable`, `Symbol.dispose`, `Symbol.asyncDispose`).
fn disposable_libs() -> Vec<Arc<LibFile>> {
    let mut names: Vec<&str> = DEFAULT_LIB_NAMES.to_vec();
    names.push("esnext.disposable.d.ts");
    load_lib_files(&names)
}

/// Full elaboration text (primary message plus every related-information line)
/// of the single diagnostic with `code` in `source`, checked strict with the
/// disposable lib bundle.
fn elaboration(source: &str, code: u32) -> String {
    let libs = disposable_libs();
    assert!(
        !libs.is_empty(),
        "disposable lib bundle must load for this test to be meaningful"
    );
    let diags = check_source_with_libs(source, "test.ts", strict_checker_options(), &libs);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// A `using` initializer whose object type lacks `[Symbol.dispose]` must carry
/// the missing-property relation reason against `Disposable` beneath TS2850.
#[test]
fn using_non_disposable_object_elaborates_missing_dispose_against_disposable() {
    let text = elaboration(
        r#"
declare const widget: { foo: number };
function f() { using handle = widget; }
"#,
        2850,
    );
    assert_eq!(
        text,
        "The initializer of a 'using' declaration must be either an object with a '[Symbol.dispose]()' method, or be 'null' or 'undefined'.\n\
         Property '[Symbol.dispose]' is missing in type '{ foo: number; }' but required in type 'Disposable'.",
    );
}

/// A `using` initializer that is a union with a non-disposable member must
/// elaborate the offending member against `Disposable`.
#[test]
fn using_union_with_non_disposable_member_elaborates_member_against_disposable() {
    let text = elaboration(
        r#"
declare const resource: Disposable | number;
function f() { using held = resource; }
"#,
        2850,
    );
    assert_eq!(
        text,
        "The initializer of a 'using' declaration must be either an object with a '[Symbol.dispose]()' method, or be 'null' or 'undefined'.\n\
         Type 'number' is not assignable to type 'Disposable'.",
    );
}

/// An `await using` initializer that lacks both dispose methods must carry the
/// missing-property relation reason against `AsyncDisposable` beneath TS2851.
#[test]
fn await_using_non_disposable_object_elaborates_against_async_disposable() {
    let text = elaboration(
        r#"
declare const widget: { foo: number };
async function f() { await using handle = widget; }
"#,
        2851,
    );
    assert_eq!(
        text,
        "The initializer of an 'await using' declaration must be either an object with a '[Symbol.asyncDispose]()' or '[Symbol.dispose]()' method, or be 'null' or 'undefined'.\n\
         Property '[Symbol.asyncDispose]' is missing in type '{ foo: number; }' but required in type 'AsyncDisposable'.",
    );
}

/// A genuinely disposable object must not trigger TS2850 at all (the decision
/// path is unchanged); this guards against the elaboration routing flipping the
/// accept/reject verdict.
#[test]
fn disposable_object_does_not_report_ts2850() {
    let libs = disposable_libs();
    let diags = check_source_with_libs(
        r#"
declare const widget: Disposable;
function f() { using handle = widget; }
"#,
        "test.ts",
        strict_checker_options(),
        &libs,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "a Disposable initializer must not report TS2850, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
