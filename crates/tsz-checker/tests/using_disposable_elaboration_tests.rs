//! Regression tests for the relation-derived elaboration tail on a failed
//! `using` declaration (TS2850) and, more broadly, for symbol-keyed missing
//! property elaboration (TS2741) on a non-array target.
//!
//! Structural rule: when an object relation fails only on late-bound-symbol
//! members, `tsc` lists those members in TS2741/TS2739 for **non-array** targets
//! (e.g. `Disposable`'s `[Symbol.dispose]`) and omits them only for array-like
//! targets (where the iteration protocol makes them implicitly satisfied). The
//! `using` check routes a failed sync `using` through the shared assignability
//! gateway against the global `Disposable` interface so the nested tail is the
//! real relation reason rather than a hand-built string; `await using` (TS2851)
//! carries no tail.
//!
//! Owner: solver `explain_object_failure` (reason) +
//! `error_reporter::assignability` (emission) +
//! `check_using_declaration_disposable`.
//!
//! The cases are fully self-contained (an inline `Symbol`/`Disposable` so the
//! relation stays in one arena), and they vary the source member name where it
//! reaches the rendered output to lock the shape as structural, not a fixture
//! spelling (CLAUDE.md anti-hardcoding gate).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{DEFAULT_LIB_NAMES, check_source_with_libs, load_compiled_lib_files};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(body: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    // Use TypeScript's real compiled libs so the well-known `Symbol.dispose`
    // resolves and renders as `[Symbol.dispose]` (the stripped test-lib bundle
    // does not model well-known symbols faithfully). `esnext.disposable` carries
    // `Disposable`/`AsyncDisposable`; it is not in the default bundle.
    let mut names: Vec<String> = DEFAULT_LIB_NAMES
        .iter()
        .map(|name| format!("lib.{name}"))
        .collect();
    names.push("lib.esnext.disposable.d.ts".to_string());
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let libs = load_compiled_lib_files(&name_refs);
    assert!(
        !libs.is_empty(),
        "compiled TypeScript libs must be available for these tests",
    );
    check_source_with_libs(
        body,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

/// Full elaboration text (primary message plus every related-information line,
/// in order) of the single diagnostic with `code`.
fn elaboration(body: &str, code: u32) -> String {
    let diags = check(body);
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

/// Sync `using` whose initializer lacks `[Symbol.dispose]`: the TS2850 top line
/// keeps its wording and gains the relation-derived missing-property tail that
/// `tsc` attaches via `checkTypeAssignableTo(initType, Disposable)`.
#[test]
fn using_missing_dispose_attaches_symbol_missing_tail() {
    let text = elaboration(
        r#"
declare const x: { foo: number };
function f() { using r = x; }
"#,
        2850,
    );
    assert_eq!(
        text,
        "The initializer of a 'using' declaration must be either an object with a \
         '[Symbol.dispose]()' method, or be 'null' or 'undefined'.\n\
         Property '[Symbol.dispose]' is missing in type '{ foo: number; }' but required \
         in type 'Disposable'.",
    );
}

/// Same rule, different source member name — the tail must echo the renamed
/// source type, never a hard-coded `{ foo: number; }`.
#[test]
fn using_missing_dispose_tail_is_member_name_independent() {
    let text = elaboration(
        r#"
declare const handle: { resourceId: string };
function open() { using h = handle; }
"#,
        2850,
    );
    assert_eq!(
        text,
        "The initializer of a 'using' declaration must be either an object with a \
         '[Symbol.dispose]()' method, or be 'null' or 'undefined'.\n\
         Property '[Symbol.dispose]' is missing in type '{ resourceId: string; }' but \
         required in type 'Disposable'.",
    );
}

/// `await using` (TS2851) carries NO tail in `tsc`, so the sync-only routing
/// must leave it flat — a single message line, no related information.
#[test]
fn await_using_missing_dispose_stays_flat() {
    let diags = check(
        r#"
declare const x: { foo: number };
async function f() { await using r = x; }
"#,
    );
    let ts2851 = diags
        .iter()
        .find(|d| d.code == 2851)
        .expect("expected TS2851 for await using on a non-disposable object");
    assert!(
        ts2851.related_information.is_empty(),
        "await using (TS2851) must carry no elaboration tail, got: {:?}",
        ts2851
            .related_information
            .iter()
            .map(|i| i.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// A genuinely disposable object must not error at all — the tail machinery only
/// runs on a real failure, and the error decision itself is unchanged.
#[test]
fn using_disposable_object_is_clean() {
    let diags = check(
        r#"
declare const x: { [Symbol.dispose](): void; foo: number };
function f() { using r = x; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "a `[Symbol.dispose]`-bearing object must not raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// The broader explain fix: a plain assignment to `Disposable` whose source lacks
/// only the symbol member now produces the symbol-keyed TS2741 (matching `tsc`),
/// instead of collapsing to a flat `TypeMismatch` with no member listed. This is
/// the same `explain_object_failure` path the `using` tail rides, exercised on a
/// surface that does not depend on the disposable feature gate.
#[test]
fn plain_assignment_to_disposable_lists_symbol_member() {
    let text = elaboration(
        r#"
declare const x: { foo: number };
const d: Disposable = x;
"#,
        2741,
    );
    assert_eq!(
        text,
        "Property '[Symbol.dispose]' is missing in type '{ foo: number; }' but required \
         in type 'Disposable'.",
    );
}

/// #16862 regression: a `using` initializer is never excess-property (freshness)
/// checked against `Disposable` -- an object literal with a valid dispose method
/// plus extra properties is a perfectly good disposable, `tsc` does not run
/// freshness here even though the source is a fresh object literal.
#[test]
fn using_fresh_literal_with_extra_property_is_clean() {
    let diags = check(
        r#"
function f() { using r = { [Symbol.dispose]() {}, extra: 1 }; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "extra properties on a fresh disposable literal must not raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Same rule for `await using` (TS2851): a fresh literal with a valid
/// `[Symbol.asyncDispose]` plus extra properties is clean.
#[test]
fn await_using_fresh_literal_with_extra_property_is_clean() {
    let diags = check(
        r#"
async function f() { await using r = { async [Symbol.asyncDispose]() {}, extra: 1 }; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2851),
        "extra properties on a fresh async-disposable literal must not raise TS2851, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Freshness suppression must not swallow a genuine structural mismatch: a
/// `[Symbol.dispose]` whose own type is not callable at all is still rejected,
/// extra properties or not.
#[test]
fn using_fresh_literal_wrong_dispose_type_still_errors() {
    let diags = check(
        r#"
function f() { using r = { [Symbol.dispose]: 42, extra: 1 }; }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2850),
        "a non-callable [Symbol.dispose] must still raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Freshness suppression must not swallow a genuine structural mismatch: a
/// `[Symbol.dispose]` with a required parameter is not assignable to
/// `Disposable`'s zero-arg signature, extra properties or not.
#[test]
fn using_fresh_literal_wrong_dispose_arity_still_errors() {
    let diags = check(
        r#"
function f() { using r = { [Symbol.dispose](x: number) {}, extra: 1 }; }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2850),
        "a required-param [Symbol.dispose] must still raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// A fresh literal entirely missing a dispose member is still rejected -- the
/// freshness fix only suppresses excess-property checking, not the underlying
/// presence/shape requirement.
#[test]
fn using_fresh_literal_missing_dispose_still_errors() {
    let diags = check(
        r#"
function f() { using r = { notDispose() {} }; }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2850),
        "a fresh literal with no dispose member must still raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// The same object routed through a variable (never fresh) was already clean
/// before this fix and must stay clean -- pins the non-regression discriminator
/// from #16862's own repro.
#[test]
fn using_non_fresh_variable_with_extra_property_is_clean() {
    let diags = check(
        r#"
const o = { [Symbol.dispose]() {}, extra: 1 };
function f() { using r = o; }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2850),
        "a disposable object routed through a variable must not raise TS2850, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
