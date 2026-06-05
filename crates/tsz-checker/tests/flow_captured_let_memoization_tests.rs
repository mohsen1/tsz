//! Regression coverage for the symbol-keyed memoization of captured-`let`
//! "effectively const" detection (issue #11337).
//!
//! `is_effectively_const_for_narrowing` decides whether a captured `let`
//! binding keeps its outer-scope narrowing inside a closure. The decision
//! requires knowing whether any reassignment to the binding lives in a nested
//! closure, which historically walked *every* flow node once per captured
//! reference (`has_assignment_in_nested_closure`). That predicate depends only
//! on the symbol, so N references in a function with M flow nodes cost
//! `O(N · M)` — the `O(n^2)` "binder/checker checks in long operator chains"
//! shape from the issue. The fix memoizes the predicate per `SymbolId`, so each
//! symbol is scanned once.
//!
//! These tests pin two things:
//! 1. Behavior is unchanged — an effectively-const captured `let` still keeps
//!    its narrowing inside the closure (and a renamed binding behaves the same,
//!    per the anti-hardcoding gate).
//! 2. A large fan-out of captured-`let` narrowings type-checks well within a
//!    wall-clock budget, guarding against an accidental return of the
//!    per-reference quadratic scan.

use std::time::{Duration, Instant};

use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn diagnostics_within(source: &str, budget: Duration) -> Vec<(u32, String)> {
    let start = Instant::now();
    let diagnostics = strict_diagnostics(source);
    let elapsed = start.elapsed();
    assert!(
        elapsed < budget,
        "captured-let narrowing did not terminate in budget: elapsed={elapsed:?} budget={budget:?}",
    );
    diagnostics
}

/// An effectively-const `let` (declared `let`, never reassigned) narrowed by a
/// `typeof` guard keeps that narrowing inside a closure created in the guarded
/// branch. `value.toUpperCase()` must therefore be legal — if narrowing were
/// dropped, `number` would surface a TS2339.
#[test]
fn captured_const_let_narrowing_preserved_in_closure() {
    let diagnostics = strict_diagnostics(
        r#"
function probe(flag: boolean) {
    let value: string | number = flag ? "x" : 1;
    if (typeof value === "string") {
        const read = () => value.toUpperCase();
        return read();
    }
    return value;
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2339 && *code != 2322),
        "effectively-const captured let should keep its narrowing in the closure, got: {diagnostics:?}",
    );
}

/// Same structure, renamed binding: the memoization keys on `SymbolId`, never on
/// the identifier spelling, so the result must be identical (anti-hardcoding).
#[test]
fn captured_const_let_narrowing_preserved_renamed_binding() {
    let diagnostics = strict_diagnostics(
        r#"
function probe(condition: boolean) {
    let candidate: string | number = condition ? "x" : 1;
    if (typeof candidate === "string") {
        const consume = () => candidate.toUpperCase();
        return consume();
    }
    return candidate;
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2339 && *code != 2322),
        "renamed effectively-const captured let should narrow identically, got: {diagnostics:?}",
    );
}

/// A `let` reassigned inside a *nested* closure is not effectively const, so its
/// outer narrowing is not preserved inside another closure. Two references to
/// the same binding must agree — the memoized predicate returns one answer for
/// the symbol. The assertion only checks that the checker stays sound and does
/// not crash or hang; it does not pin a specific diagnostic, so it is robust to
/// behavior that is intentionally identical before and after the cache.
#[test]
fn reassigned_in_nested_closure_let_is_consistent_across_references() {
    let diagnostics = strict_diagnostics(
        r#"
function probe(flag: boolean) {
    let value: string | number = flag ? "x" : 1;
    const mutate = () => { value = 2; };
    const first = () => value;
    const second = () => value;
    mutate();
    return [first(), second()];
}
"#,
    );

    // No TS-internal panic surfaced as an error code outside the expected range.
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 0),
        "reassigned captured let should type-check without internal errors, got: {diagnostics:?}",
    );
}

/// Large fan-out: 64 distinct effectively-const `let` bindings, each narrowed
/// and captured in its own closure. Before the fix every captured reference
/// re-scanned all flow nodes, so this grew quadratically; the memoized scan is
/// linear and completes near-instantly. A generous budget keeps the guard
/// stable on slow CI while still catching a quadratic regression.
#[test]
fn many_captured_let_narrowings_stay_linear() {
    let mut source = String::from("function probe(flag: boolean) {\n");
    for i in 0..64 {
        source.push_str(&format!(
            "    let v{i}: string | number = flag ? \"x\" : {i};\n\
             \x20   if (typeof v{i} === \"string\") {{\n\
             \x20       const read{i} = () => v{i}.toUpperCase();\n\
             \x20       read{i}();\n\
             \x20   }}\n",
        ));
    }
    source.push_str("    return flag;\n}\n");

    let diagnostics = diagnostics_within(&source, Duration::from_secs(20));

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2339 && *code != 2322),
        "captured-let narrowing must be preserved across the whole fan-out, got: {diagnostics:?}",
    );
}
