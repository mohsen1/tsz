//! Argument (`TS2345`) relation failures whose cause is a function-signature
//! *shape* must carry the same chained elaboration `tsc` emits under the
//! direct-assignment (`TS2322`) surface.
//!
//! Two function-signature `SubtypeFailureReason`s used to render their
//! elaboration only on the assignment surface and dropped it on the argument
//! surface, because `related_from_failure_reason` (the `TS2345` related-info
//! builder) had no arm for them and fell through to its `_ => return None`
//! catch-all:
//!
//! - [`TooManyParameters`] — the source callback declares more required
//!   parameters than the target signature provides arguments for. `tsc`:
//!   `Target signature provides too few arguments. Expected N or more, but got
//!   M.` (`TS2849`).
//! - [`TypePredicateMismatch`] — the source's `x is A` type predicate is not
//!   assignable to the target's `x is B`. `tsc`: `Type predicate 'x is A' is
//!   not assignable to 'x is B'.` followed by the nested `Type 'A' is not
//!   assignable to type 'B'.` leaf.
//!
//! The header-only `TS2345` (no elaboration) is exactly the shape #14816 fixed
//! for tuple-arity reasons; this is the same class on the function-signature
//! axis. The structural rule is independent of the binder names and the call
//! surface (free call, method call, `new`), so each case varies them.
//!
//! Oracle: `typescript@7.0.2`.
//!
//! [`TooManyParameters`]: tsz_solver::SubtypeFailureReason::TooManyParameters
//! [`TypePredicateMismatch`]: tsz_solver::SubtypeFailureReason::TypePredicateMismatch

use tsz_checker::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_checker::test_utils::check_source_diagnostics;

fn single(source: &str, code: u32) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS{code} diagnostic, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

fn related_messages(diagnostic: &Diagnostic) -> Vec<String> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.clone())
        .collect()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    related_messages(diagnostic)
        .iter()
        .any(|message| message == expected)
}

// ---------------------------------------------------------------------------
// TooManyParameters -> TS2849 "Target signature provides too few arguments."
// on the argument (TS2345) surface. Vary the binder names and the call
// surface: a free call, a method call, and a `new` expression all route
// through `error_argument_not_assignable_at`.
// ---------------------------------------------------------------------------

#[test]
fn too_many_params_free_call_argument_reports_too_few_arguments() {
    let diagnostic = single(
        "declare function run(cb: (x: number) => void): void;
         run((first: number, second: number) => {});",
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Target signature provides too few arguments. Expected 2 or more, but got 1."
        ),
        "argument-surface TS2345 dropped the TS2849 arity line; related = {:#?}",
        related_messages(&diagnostic)
    );
    assert!(
        diagnostic.related_information.iter().any(|r| r.code
            == diagnostic_codes::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT
            && r.depth == 0),
        "the arity leaf must sit at depth 0 under the TS2345 head; related = {:#?}",
        diagnostic.related_information
    );
}

#[test]
fn too_many_params_method_call_argument_reports_too_few_arguments() {
    // Renamed binders + a method-call surface: same structural reason.
    let diagnostic = single(
        "declare class Registry { subscribe(handler: (evt: number) => void): void; }
         declare const registry: Registry;
         registry.subscribe((evt: number, meta: number) => {});",
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Target signature provides too few arguments. Expected 2 or more, but got 1."
        ),
        "method-call TS2345 dropped the TS2849 arity line; related = {:#?}",
        related_messages(&diagnostic)
    );
}

#[test]
fn too_many_params_new_expression_argument_reports_too_few_arguments() {
    let diagnostic = single(
        "declare class Widget { constructor(render: (frame: number) => void); }
         const widget = new Widget((frame: number, layer: number) => {});",
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Target signature provides too few arguments. Expected 2 or more, but got 1."
        ),
        "new-expression TS2345 dropped the TS2849 arity line; related = {:#?}",
        related_messages(&diagnostic)
    );
}

// ---------------------------------------------------------------------------
// TypePredicateMismatch -> "Type predicate 'x is A' is not assignable to
// 'x is B'." plus the nested "Type 'A' is not assignable to type 'B'." leaf,
// on the argument (TS2345) surface.
// ---------------------------------------------------------------------------

#[test]
fn type_predicate_mismatch_argument_reports_predicate_and_nested_leaf() {
    let diagnostic = single(
        "declare function accept(guard: (value: unknown) => value is string): void;
         declare const isNumber: (value: unknown) => value is number;
         accept(isNumber);",
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Type predicate 'value is number' is not assignable to 'value is string'."
        ),
        "argument-surface TS2345 dropped the type-predicate line; related = {:#?}",
        related_messages(&diagnostic)
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'number' is not assignable to type 'string'."
        ),
        "argument-surface TS2345 dropped the nested predicate leaf; related = {:#?}",
        related_messages(&diagnostic)
    );
}

// ---------------------------------------------------------------------------
// Guard: a plain parameter-*type* mismatch (already handled) still elaborates,
// so the new arms did not shadow the existing ParameterTypeMismatch path.
// ---------------------------------------------------------------------------

#[test]
fn parameter_type_mismatch_argument_still_elaborates() {
    let diagnostic = single(
        "declare function run(cb: (x: string) => void): void;
         run((x: number) => {});",
        2345,
    );
    assert!(
        related_messages(&diagnostic)
            .iter()
            .any(|message| message.contains("Types of parameters 'x' and 'x' are incompatible")),
        "parameter-type-mismatch elaboration regressed; related = {:#?}",
        related_messages(&diagnostic)
    );
}
