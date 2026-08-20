//! The false branch of a logical condition over an `unknown` must absorb the
//! top type. `typeof x === "object" && x !== null` narrows the *else* branch to
//! the union of `narrow_false(typeof === "object")` (which is `unknown`) and the
//! `x === null` residual (`null`); combining those must yield `unknown`, not a
//! non-normalized `unknown | null`.
//!
//! The logical-condition combiner built that union with
//! `union_preserve_members`, which (before the fix) skipped the universal
//! top/bottom sentinel absorption that `normalize_union` applies. The stray
//! `unknown | null` then mis-narrowed under a following `typeof x === "function"`
//! guard, collapsing `x` to `never` so a subsequent call emitted a false
//! `TS2349` ("This expression is not callable").
//!
//! Binder names are varied across cases per the anti-hardcoding gate.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics.iter().map(|d| d.code).collect()
}

/// The original witness: after the object branch returns, `typeof === "function"`
/// must narrow the `unknown` residual to a callable function type.
#[test]
fn function_branch_after_object_and_nonnull_is_callable() {
    let diagnostics = check_source_diagnostics(
        r#"
function dispatch(payload: unknown) {
    if (typeof payload === "object" && payload !== null) { return payload; }
    if (typeof payload === "function") { return payload(); }
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "calling the function-narrowed `payload` must not error, got {:?}",
        codes(&diagnostics)
    );
}

/// Renamed binder + reordered nullish conjunct (`x != null && typeof ...`); the
/// residual is still `unknown`, so the call stays valid.
#[test]
fn function_branch_with_renamed_binder_and_reordered_conjuncts() {
    let diagnostics = check_source_diagnostics(
        r#"
function invoke(candidate: unknown) {
    if (candidate != null && typeof candidate === "object") { return candidate; }
    if (typeof candidate === "function") { return candidate(0, "x"); }
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "reordered guard must still leave a callable residual, got {:?}",
        codes(&diagnostics)
    );
}

/// The residual of `typeof v === "object" && v !== null` over `unknown` is
/// exactly `unknown` (tsc), so it is assignable to nothing narrower.
#[test]
fn else_residual_of_object_and_nonnull_is_unknown() {
    let diagnostics = check_source_diagnostics(
        r#"
function probe(value: unknown) {
    if (typeof value === "object" && value !== null) { return; }
    const widened: unknown = value;
    const tooNarrow: string = value;
    return widened;
}
"#,
    );
    // Only the `string` annotation is wrong; `unknown` is accepted. A spurious
    // `unknown | null` would instead make BOTH assignments behave oddly.
    assert_eq!(
        codes(&diagnostics),
        vec![2322],
        "residual must be plain `unknown`: only the `string` assignment errors, got {:?}",
        codes(&diagnostics)
    );
}

/// `||` false branch over `unknown` is the conjunction of negations; the
/// `unknown` constituent must still absorb.
#[test]
fn or_false_branch_keeps_callable_function_narrowing() {
    let diagnostics = check_source_diagnostics(
        r#"
function handle(input: unknown) {
    if (typeof input === "string" || input === null) { return; }
    if (typeof input === "function") { return input(); }
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "function call after an `||` guard must stay valid, got {:?}",
        codes(&diagnostics)
    );
}
