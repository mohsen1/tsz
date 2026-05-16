//! Regression tests for readonly tuple sources at generic call-site array
//! constraints (issue #5804).
//!
//! Structural rule: at a generic-call constraint boundary, when the
//! constraint structurally requires a mutable `Array<X>` / tuple, a readonly
//! tuple source is accepted because its element list is fixed and its
//! elements can still be checked against the constraint's element type. This
//! mirrors tsc, which accepts `processArray([1, "two", true] as const)`
//! against `T extends unknown[]` even though a *direct* assignment of the
//! same tuple to `unknown[]` is rejected with TS4104.
//!
//! The loosening is intentionally narrow:
//!   * Source must be a readonly **tuple** (not a plain `ReadonlyArray<X>`).
//!   * Constraint must already be a mutable array/tuple shape.
//!   * Direct assignments (`const a: unknown[] = readonlyTuple`) continue to
//!     error via the standard assignability path (TS4104).
//!
//! See
//! `tsz_solver::operations::core::call_evaluator::CallEvaluator::arg_satisfies_constraint_via_readonly_widening`
//! for the helper that implements the rule.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn readonly_tuple_satisfies_unknown_array_constraint_via_as_const() {
    // Reported repro from issue #5804.
    let source = r#"
        function processArray<T extends unknown[]>(arr: T): T[number] {
            return arr[0];
        }
        const mixed = processArray([1, "two", true] as const);
    "#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "readonly tuple should satisfy `T extends unknown[]` at the call \
         site; got: {diags:#?}"
    );
}

#[test]
fn readonly_tuple_constraint_check_is_not_keyed_on_type_parameter_name() {
    // Anti-hardcoding: renaming `T` to `Foo` must not change behavior.
    let source = r#"
        function processArray<Foo extends unknown[]>(arr: Foo): Foo[number] {
            return arr[0];
        }
        const mixed = processArray([1, "two", true] as const);
    "#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "renaming the type parameter must not break the constraint \
         loosening; got: {diags:#?}"
    );
}

#[test]
fn readonly_tuple_satisfies_specific_mutable_element_constraint() {
    // `readonly [1, 2, 3] as const` against `T extends number[]`.
    let source = r#"
        function takesNum<T extends number[]>(arr: T): T[number] {
            return arr[0];
        }
        const nums = takesNum([1, 2, 3] as const);
    "#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "readonly number tuple should satisfy `T extends number[]` at the \
         call site; got: {diags:#?}"
    );
}

#[test]
fn readonly_array_source_still_fails_against_mutable_array_constraint() {
    // tsc rejects a plain `readonly X[]` source at the constraint
    // boundary because its element list is unbounded; only tuples are
    // loosened. tsz must mirror this rejection.
    let source = r#"
        function processArray<T extends unknown[]>(arr: T): T[number] {
            return arr[0];
        }
        const ra: readonly string[] = ["a", "b"];
        const out = processArray(ra);
    "#;
    let codes = error_codes(&check(source));
    assert!(
        codes.contains(&2345),
        "plain ReadonlyArray source must still be rejected at the \
         constraint boundary; got error codes: {codes:?}"
    );
}

#[test]
fn direct_assignment_of_readonly_tuple_to_mutable_array_still_errors() {
    // Negative case (hard constraint from the issue): the loosening must
    // NOT apply to direct assignments. tsc emits TS4104 here.
    let source = r#"
        const tup = [1, "two", true] as const;
        const a: unknown[] = tup;
    "#;
    let codes = error_codes(&check(source));
    assert!(
        codes.contains(&4104),
        "direct readonly-tuple to mutable-array assignment must still emit \
         TS4104; got error codes: {codes:?}"
    );
}
