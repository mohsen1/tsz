//! Regression tests for issue #14792.
//!
//! When a generic call `f<T, U>(x: T, fn: (x: T) => U): U` (or a covariant
//! wrapper of `U` such as `(...): U[]`) is directly contextually typed by a
//! MISMATCHING annotation, `tsc` seeds the return-position type parameter `U`
//! from the outer contextual type with `InferencePriority.ReturnType` — a
//! low-priority hint that can never override the callback's bottom-up inference.
//! The callback body is checked against the FINAL inferred `U`, and any
//! annotation mismatch is reported once, at the assignment site.
//!
//! tsz previously let the contextual return type win in two ways:
//!   * solver — when the contextual type was a literal REFINEMENT of the
//!     bottom-up inferred type (`{ v: 5 }` narrower than `{ v: number }`), the
//!     `should_use_contextual_return_substitution` override replaced the
//!     inferred `U` with the contextual type even though the real callback
//!     evidence (`{ v: number }`) was not assignable to it. The error then
//!     surfaced inside the callback body (or as a spurious extra diagnostic)
//!     instead of at the assignment.
//!   * checker — the contextual-return suppression that keeps the outer
//!     annotation off a pinned object-literal callback return only fired for a
//!     BARE return type parameter, so the `(...): U[]` array-wrapped form still
//!     pushed the outer element type onto the callback body and double-reported.
//!
//! Anti-hardcoding: the structural rule is "the callback return is fully
//! determined by concrete-pinned inputs, so the contextual return type cannot
//! refine it", so the tests vary binder names and target shapes rather than
//! matching any specific identifier or rendered message.

use tsz_checker::test_utils::{
    DiagnosticShape, assert_diagnostic_shapes_exactly, check_source_diagnostics,
    diagnostic_code_message_refs,
};

fn assert_clean(source: &str, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "{context}: expected no diagnostics, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn inline_callback_literal_target_reports_once_at_assignment() {
    // The callback `(x) => ({ v: x })` returns `{ v: number }`; the literal
    // target `{ v: 5 }` cannot refine it, so the single TS2322 lands on the
    // assignment — never inside the callback body (no second diagnostic).
    let source = "\
declare function call<A, B>(x: A, fn: (x: A) => B): B;
const bad: { v: 5 } = call(1, (x) => ({ v: x }));
";
    let diagnostics = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(2322)
            .at(2, 7)
            .with_message_fragment("is not assignable to type '{ v: 5; }'")],
    );
}

#[test]
fn extracted_callback_literal_target_reports_once_at_assignment() {
    // Same shape with the callback hoisted to a variable: the bottom-up `U`
    // is `{ v: number }`, so the result `{ v: number }` is checked against the
    // annotation `{ v: 6 }` once, at the assignment.
    let source = "\
declare function pipe<S, R>(x: S, fn: (x: S) => R): R;
const cb = (x: number) => ({ v: x });
const bad: { v: 6 } = pipe(1, cb);
";
    let diagnostics = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(2322)
            .at(3, 7)
            .with_message_fragment("is not assignable to type '{ v: 6; }'")],
    );
}

#[test]
fn block_body_callback_literal_target_reports_once_at_assignment() {
    let source = "\
declare function run<P, Q>(x: P, fn: (x: P) => Q): Q;
const bad: { v: 7 } = run(1, (x) => { return { v: x }; });
";
    let diagnostics = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(2322)
            .at(2, 7)
            .with_message_fragment("is not assignable to type '{ v: 7; }'")],
    );
}

#[test]
fn primitive_target_still_reports_once_at_assignment() {
    // Control: a primitive (non-refining) mismatch already behaved correctly and
    // must keep reporting exactly one diagnostic at the assignment.
    let source = "\
declare function call<A, B>(x: A, fn: (x: A) => B): B;
const bad: { v: string } = call(1, (x) => ({ v: x }));
";
    let diagnostics = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(2322)
            .at(2, 7)
            .with_message_fragment("is not assignable to type '{ v: string; }'")],
    );
}

#[test]
fn compatible_target_stays_clean() {
    assert_clean(
        "\
declare function call<A, B>(x: A, fn: (x: A) => B): B;
const ok: { v: number } = call(1, (x) => ({ v: x }));
",
        "compatible object target",
    );
}

#[test]
fn array_wrapped_callback_literal_target_reports_once_at_assignment() {
    // `map<T, U>(arr: T[], fn: (x: T) => U): U[]`: the call return wraps the
    // bare callback return `U`. The pinned object-literal callback return cannot
    // be refined by the contextual element type, so the only diagnostic is the
    // assignment-level array mismatch — no extra error inside the callback body.
    let source = "\
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const bad: { v: string }[] = map([1], (x) => ({ v: x }));
";
    let diagnostics = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(2322)
            .at(2, 7)
            .with_message_fragment("is not assignable to type '{ v: string; }[]'")],
    );
}

#[test]
fn array_wrapped_compatible_target_stays_clean() {
    assert_clean(
        "\
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const ok: { v: number }[] = map([1], (x) => ({ v: x }));
",
        "compatible array target",
    );
}

#[test]
fn contextual_literal_union_supertype_still_narrows() {
    // Guard against over-suppression: when the bottom-up evidence DOES fit the
    // contextual type (the lambda literal `1` is assignable to `0 | 1 | 2`), the
    // contextual return substitution still applies and the call stays clean.
    assert_clean(
        "\
declare function invoke<T>(f: () => T): T;
const xx: 0 | 1 | 2 = invoke(() => 1);
",
        "literal fits contextual union",
    );
}

#[test]
fn contextual_literal_union_mismatch_reports_at_assignment() {
    // The lambda returns `5`, which does not fit `0 | 1 | 2`; the inferred type
    // is kept and the mismatch is reported at the assignment, matching tsc.
    let source = "\
declare function invoke<T>(f: () => T): T;
const yy: 0 | 1 | 2 = invoke(() => 5);
";
    let diagnostics = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diagnostics,
        &[DiagnosticShape::code(2322)
            .at(2, 7)
            .with_message_fragment("is not assignable to type '0 | 1 | 2'")],
    );
}
