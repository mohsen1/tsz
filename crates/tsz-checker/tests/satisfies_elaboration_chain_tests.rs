//! Regression tests for the TS1360 (`satisfies`) elaboration chain.
//!
//! Structural rule: a `satisfies` failure is rendered through the *same*
//! assignability elaboration tsc would build for an assignment of the operand
//! to the target, with the `Type 'X' does not satisfy the expected type 'Y'`
//! head message layered on top:
//!
//! * When the assignment top is the generic "Type X is not assignable to type
//!   Y" relation, the satisfies head *replaces* it — the relation is never
//!   restated as an extra child (tsc shows only the head plus any deeper
//!   property elaboration).
//! * When the assignment top is a *specific* failure (e.g. TS2741 missing
//!   property), the satisfies head is *prepended* and the specific message is
//!   demoted into the related chain.
//!
//! Before this fix the TS1360 path re-injected the top relation message as a
//! redundant related entry, so `1 satisfies boolean` produced a spurious
//! `Type 'number' is not assignable to type 'boolean'` child that tsc does not
//! emit.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

/// Check `source` and return the single TS1360 it must produce, asserting the
/// head is the `satisfies` message. All tests here expect exactly one TS1360.
fn single_satisfies_diagnostic(source: &str) -> Diagnostic {
    let mut diags: Vec<Diagnostic> = check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|diag| diag.code == 1360)
        .collect();
    assert_eq!(
        diags.len(),
        1,
        "expected one TS1360 for `{source}`, got: {diags:?}"
    );
    let diag = diags.remove(0);
    assert!(
        diag.message_text
            .contains("does not satisfy the expected type"),
        "head must be the satisfies message, got: {}",
        diag.message_text
    );
    diag
}

/// A primitive `satisfies` mismatch is a single head message with no children:
/// tsc does not restate the relation as a redundant "not assignable" child.
#[test]
fn primitive_satisfies_has_no_relation_restatement_child() {
    for source in [
        "const x = 1 satisfies boolean;\n",
        "const x = \"s\" satisfies number;\n",
        "const x = true satisfies string;\n",
    ] {
        let diag = single_satisfies_diagnostic(source);
        assert!(
            diag.related_information.is_empty(),
            "primitive satisfies failure must have no related children, got: {:?}",
            diag.related_information
        );
    }
}

/// A nested property mismatch keeps the satisfies head and the deeper
/// "Types of property ... are incompatible" elaboration, but must NOT contain a
/// redundant top-level "Type X is not assignable to type Y" restatement.
#[test]
fn object_property_satisfies_does_not_restate_top_relation() {
    let diag = single_satisfies_diagnostic(
        "declare const o: { a: number };\n\
         const r = o satisfies { a: string };\n",
    );

    // No related child may simply restate the top-level relation between the
    // *same* source and target the head already names.
    for info in &diag.related_information {
        assert!(
            !(info.message_text.contains("is not assignable to type")
                && info.message_text.contains("{ a: number; }")
                && info.message_text.contains("{ a: string; }")),
            "TS1360 must not restate the top relation as a child, got: {}",
            info.message_text
        );
    }

    // The deeper property elaboration is still present.
    assert!(
        diag.related_information
            .iter()
            .any(|info| info.message_text.contains("property 'a'")),
        "expected the property-level elaboration to survive, got: {:?}",
        diag.related_information
    );
}

/// A missing required property still nests the TS2741 message beneath the
/// satisfies head (tsc reports the missing-property message as a child here,
/// not as the top-level diagnostic).
#[test]
fn missing_property_satisfies_nests_specific_message() {
    // Two name choices for the missing property prove the rule is structural,
    // not keyed to a particular identifier spelling.
    for (prop, source) in [
        (
            "c",
            "declare const o: { a: number };\n\
             const r = o satisfies { a: string; c: boolean };\n",
        ),
        (
            "ready",
            "declare const o: { a: number };\n\
             const r = o satisfies { a: string; ready: boolean };\n",
        ),
    ] {
        let diag = single_satisfies_diagnostic(source);
        assert!(
            diag.related_information.iter().any(|info| info
                .message_text
                .contains(&format!("Property '{prop}' is missing"))),
            "missing-property message must be nested under the satisfies head, got: {:?}",
            diag.related_information
        );
    }
}
