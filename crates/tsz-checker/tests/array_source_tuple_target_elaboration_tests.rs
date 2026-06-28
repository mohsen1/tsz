//! Array-source vs tuple-target relation-failure elaboration must match `tsc`.
//!
//! When the **source is an (unbounded) array** and the **target is a tuple**
//! with required positions, the relation correctly rejects, but `tsz` used to
//! drop the tuple-arity sub-line `tsc` appends. The structural rule mirrors
//! `tsc`'s `tupleTypesRelated` gate, modeling the open array as an all-rest
//! source `[...E[]]`:
//!
//! - a **closed target** (no rest element) that requires more than the open
//!   source guarantees -> `TS2620` `Target requires N element(s) but source may
//!   have fewer.`; a closed target the open source may overflow -> `TS2621`
//!   `Target allows only N element(s) but source may have more.`;
//! - a target that **carries a rest element** passes the arity gate, so the
//!   first required slot the open source cannot pin reports `TS2623` `Source
//!   provides no match for required element at position N in target.`
//!
//! Regression guard for #14816 (the array-source arm never synthesized the
//! arity/position reason; the header-only `TS2322`/`TS2345` was emitted alone).

use tsz_checker::diagnostics::Diagnostic;
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

fn related(diagnostic: &Diagnostic) -> Vec<String> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.clone())
        .collect()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    related(diagnostic)
        .iter()
        .any(|message| message == expected)
}

// ---------------------------------------------------------------------------
// Closed target -> TS2620 "Target requires N element(s) but source may have
// fewer.". The rule is independent of the binder name and the element type, so
// vary both.
// ---------------------------------------------------------------------------

#[test]
fn array_to_closed_tuple_reports_target_requires_more() {
    for (source, expected) in [
        (
            "const a: number[] = [1, 2];
             const b: [number, number, number] = a;",
            "Target requires 3 element(s) but source may have fewer.",
        ),
        // Renamed binder + different element type: same structural reason.
        (
            "const values: string[] = [];
             const pair: [string, string] = values;",
            "Target requires 2 element(s) but source may have fewer.",
        ),
    ] {
        let diagnostic = single(source, 2322);
        assert!(
            has_related(&diagnostic, expected),
            "missing TS2620 arity line {expected:?}; related = {:#?}",
            related(&diagnostic)
        );
    }
}

/// A `readonly` array source reaches the same TS2620 reason (the explain branch
/// peels the `readonly` wrapper). The target is also `readonly` so the
/// readonly-to-mutable short-circuit (TS4104) does not pre-empt the arity line.
#[test]
fn readonly_array_to_closed_tuple_reports_target_requires_more() {
    let diagnostic = single(
        "const a: readonly number[] = [1, 2];
         const b: readonly [number, number, number] = a;",
        2322,
    );
    assert!(
        has_related(
            &diagnostic,
            "Target requires 3 element(s) but source may have fewer."
        ),
        "readonly array source dropped the TS2620 arity line; related = {:#?}",
        related(&diagnostic)
    );
}

/// A closed target the open source may overflow -> TS2621.
#[test]
fn array_to_shorter_closed_tuple_reports_target_allows_only() {
    let diagnostic = single(
        "const a: number[] = [1, 2, 3];
         const b: [number?] = a;",
        2322,
    );
    assert!(
        has_related(
            &diagnostic,
            "Target allows only 1 element(s) but source may have more."
        ),
        "missing TS2621 arity line; related = {:#?}",
        related(&diagnostic)
    );
}

// ---------------------------------------------------------------------------
// Target with a rest element -> TS2623 "Source provides no match for required
// element at position N in target.", on both the assignment (TS2322) and the
// argument (TS2345) surfaces.
// ---------------------------------------------------------------------------

#[test]
fn array_to_leading_required_rest_tuple_assignment_reports_no_match() {
    let diagnostic = single(
        "const arr: number[] = [1, 2];
         const t: [string, ...number[]] = arr;",
        2322,
    );
    assert!(
        has_related(
            &diagnostic,
            "Source provides no match for required element at position 0 in target."
        ),
        "missing TS2623 position line on the assignment path; related = {:#?}",
        related(&diagnostic)
    );
}

#[test]
fn array_to_leading_required_rest_tuple_argument_reports_no_match() {
    let diagnostic = single(
        "declare function f(t: [string, ...number[]]): void;
         const arr: number[] = [1, 2];
         f(arr);",
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Source provides no match for required element at position 0 in target."
        ),
        "missing TS2623 position line on the argument path; related = {:#?}",
        related(&diagnostic)
    );
}

/// A required element that *trails* a rest reports its true position, not
/// position 0: the array spread covers the leading rest, but the trailing
/// required slot is still unsatisfiable.
#[test]
fn array_to_trailing_required_after_rest_reports_required_position() {
    let diagnostic = single(
        "const arr: number[] = [1, 2];
         const t: [...number[], string] = arr;",
        2322,
    );
    assert!(
        has_related(
            &diagnostic,
            "Source provides no match for required element at position 1 in target."
        ),
        "expected the trailing required slot at position 1; related = {:#?}",
        related(&diagnostic)
    );
}

// ---------------------------------------------------------------------------
// Regression guards: pre-existing element/arity elaborations are unchanged.
// ---------------------------------------------------------------------------

/// A tuple-vs-tuple arity gap keeps the established `Source has N element(s) …`
/// line — the new array-source arm must not intercept tuple sources.
#[test]
fn tuple_source_arity_gap_unchanged() {
    let diagnostic = single(
        "declare let y: [string];
         let x: [string, number] = y;",
        2322,
    );
    assert!(
        has_related(
            &diagnostic,
            "Source has 1 element(s) but target requires 2."
        ),
        "tuple-vs-tuple arity elaboration regressed; related = {:#?}",
        related(&diagnostic)
    );
}

/// An array assigned to a single-rest tuple (`[...number[]]`) fails on the
/// element type, not arity — the array spread covers the rest slot, so the
/// position scan returns nothing and the element relation is elaborated.
#[test]
fn array_to_single_rest_tuple_reports_element_mismatch() {
    let diagnostic = single(
        "const arr: string[] = [];
         const t: [...number[]] = arr;",
        2322,
    );
    let messages = related(&diagnostic);
    assert!(
        messages
            .iter()
            .all(|m| !m.contains("provides no match") && !m.contains("Target requires")),
        "single-rest target must not emit an arity/position line; related = {messages:#?}"
    );
}

/// Regression for #14966 / `mappedTypeWithAny.ts`: a generic *mapping* call
/// whose argument is `any` but whose result element type is concrete must keep
/// its concrete `TS2322` headline. Here `stringifyPair` maps every element to
/// `string`, so the inferred return is `string[]`; the open array still cannot
/// pin the closed `[any, any]` tuple, so the new array-source arm appends the
/// `TS2620` arity sub-line. The display recovery must NOT degrade the concrete
/// `string[]` headline to `any[]` just because the call argument was `any`.
#[test]
fn array_source_mapping_call_keeps_concrete_headline_with_arity_subline() {
    let diagnostic = single(
        "declare function stringifyPair<T extends readonly [any, any]>(arr: T): { -readonly [K in keyof T]: string };
         let def: [any, any] = stringifyPair(void 0 as any);",
        2322,
    );
    assert_eq!(
        diagnostic.message_text,
        "Type 'string[]' is not assignable to type '[any, any]'.",
        "array-source mapping headline must stay 'string[]', not the recovered 'any[]'",
    );
    assert!(
        has_related(
            &diagnostic,
            "Target requires 2 element(s) but source may have fewer."
        ),
        "missing TS2620 arity sub-line; related = {:#?}",
        related(&diagnostic)
    );
}

/// A compatible array source produces no diagnostic.
#[test]
fn array_to_single_rest_tuple_compatible_has_no_diagnostic() {
    let codes: Vec<u32> = check_source_diagnostics(
        "const arr: number[] = [];
         const t: [...number[]] = arr;",
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect();
    assert!(
        !codes.contains(&2322),
        "unexpected TS2322 for a compatible array-to-rest-tuple assignment; got {codes:?}"
    );
}
