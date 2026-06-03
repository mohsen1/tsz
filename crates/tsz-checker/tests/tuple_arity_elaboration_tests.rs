//! TS2322 tuple length-mismatch elaboration parity with `tsc`.
//!
//! When a closed tuple source has a different fixed length than a tuple target,
//! `tsc` keeps the `TS2322` headline and attaches a nested reason line:
//!   - source longer than target allows -> `Source has N element(s) but target
//!     allows only M.` (`TS2619`)
//!   - source shorter than target requires -> `Source has N element(s) but
//!     target requires M.` (`TS2618`)
//!
//! The solver already produces the arity `SubtypeFailureReason`; this exercises
//! the checker render boundary that previously dropped the reason at the
//! top-level (`depth == 0`) and rendered non-`tsc` wording when nested.
//!
//! Binder/type-parameter names are varied across cases so the rendering is
//! proven structural, not keyed on a fixture identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn ts2322(diags: &[Diagnostic]) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0]
}

/// Source longer than a closed target -> `target allows only M` (`TS2619`).
#[test]
fn too_many_elements_attaches_allows_only_reason() {
    let diags = check_strict("const pair: [number] = [1, 2];\n");
    let diag = ts2322(&diags);
    assert!(
        diag.message_text
            .contains("Type '[number, number]' is not assignable to type '[number]'"),
        "headline should be the assignability message; got: {}",
        diag.message_text
    );
    let reason = diag
        .related_information
        .iter()
        .find(|r| r.code == 2619)
        .unwrap_or_else(|| {
            panic!(
                "expected a TS2619 arity reason; related: {:?}",
                diag.related_information
                    .iter()
                    .map(|r| (r.code, &r.message_text))
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(
        reason.message_text,
        "Source has 2 element(s) but target allows only 1."
    );
}

/// Source shorter than a closed target -> `target requires M` (`TS2618`).
#[test]
fn too_few_elements_attaches_requires_reason() {
    let diags = check_strict("const triple: [number, string, boolean] = [1, \"x\"];\n");
    let diag = ts2322(&diags);
    let reason = diag
        .related_information
        .iter()
        .find(|r| r.code == 2618)
        .unwrap_or_else(|| {
            panic!(
                "expected a TS2618 arity reason; related: {:?}",
                diag.related_information
                    .iter()
                    .map(|r| (r.code, &r.message_text))
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(
        reason.message_text,
        "Source has 2 element(s) but target requires 3."
    );
}

/// A renamed tuple alias on the source side keeps the alias headline AND still
/// carries the arity reason — proves the reason is not keyed to literal sources.
#[test]
fn aliased_tuple_source_still_carries_arity_reason() {
    let source = r#"
type Coords = [number, number];
const single: [number] = (null as unknown as Coords);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert!(
        diag.message_text.contains("Type 'Coords'"),
        "source alias name must be preserved in the headline; got: {}",
        diag.message_text
    );
    assert!(
        diag.related_information.iter().any(|r| r.code == 2619
            && r.message_text == "Source has 2 element(s) but target allows only 1."),
        "aliased source must still attach the arity reason; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
}

/// Anti-regression: matching arity must NOT produce a TS2322 or a spurious
/// arity reason.
#[test]
fn matching_arity_has_no_tuple_arity_diagnostic() {
    let diags = check_strict("const exact: [number, string] = [1, \"x\"];\n");
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "matching-arity tuple assignment must not error; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        !diags
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|r| r.code == 2618 || r.code == 2619),
        "matching-arity assignment must not emit an arity reason"
    );
}
