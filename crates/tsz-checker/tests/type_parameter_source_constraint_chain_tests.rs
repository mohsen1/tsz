//! A type-parameter source that fails a relation must elaborate the failure
//! through its declared base constraint, matching `tsc`'s
//! `getBaseConstraintOfType` chain. Without this the diagnostic stops at the
//! bare `Type 'T' is not assignable to type 'X'.` headline and hides the real
//! root (the constraint-level mismatch) — the "diagnostics hide conditional
//! mismatch in fluent type transforms" family.
//!
//! The constraint relation is surfaced as a `TypeParameterConstraintMismatch`
//! reason whose nested chain reaches the leaf, and the displayed target uses
//! the *evaluated* form so an instantiated conditional alias (`Cond<number>`)
//! shows its concrete result (`number`) exactly like `tsc`.
//!
//! Binder, type-parameter, and alias names are varied across cases so the
//! behavior is proven structural rather than keyed on any identifier.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2322(source: &str) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS2322 diagnostic, got {diagnostics:#?}"
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
        .any(|message| message.contains(expected))
}

/// Primitive constraint: `T extends string` returned as `number` chains down to
/// `Type 'string' is not assignable to type 'number'.`.
#[test]
fn primitive_constraint_chains_to_leaf() {
    let diag = ts2322(
        "function widen<Elem extends string>(value: Elem): number {\n\
         return value;\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("Type 'Elem' is not assignable to type 'number'"),
        "headline keeps the type-parameter name; got: {}",
        diag.message_text
    );
    assert!(
        has_related(&diag, "Type 'string' is not assignable to type 'number'"),
        "the base constraint relation must be elaborated; got: {:?}",
        related_messages(&diag)
    );
}

/// The core issue: an instantiated conditional alias target evaluates to a
/// concrete type, so both the headline target and the chained leaf show the
/// evaluated `number`, not the unreduced `Transform<number>` alias.
#[test]
fn conditional_alias_target_shows_evaluated_form_and_chains() {
    let diag = ts2322(
        "type Transform<Inner> = Inner extends unknown ? Inner : never;\n\
         function pipe<Token extends string>(token: Token): Transform<number> {\n\
         return token;\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("Type 'Token' is not assignable to type 'number'"),
        "the instantiated conditional target must display its evaluated form; got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("Transform<"),
        "the reduced conditional alias must not leak into the message; got: {}",
        diag.message_text
    );
    assert!(
        has_related(&diag, "Type 'string' is not assignable to type 'number'"),
        "the constraint relation against the evaluated target must be chained; got: {:?}",
        related_messages(&diag)
    );
}

/// A deeply nested conditional chain (`Step<Step<Step<number>>>`) still
/// evaluates to `number` and chains, proving the fix is not depth-limited.
#[test]
fn nested_conditional_chain_evaluates_and_chains() {
    let diag = ts2322(
        "type Step<X> = X extends unknown ? X : never;\n\
         type Chain<X> = Step<Step<Step<X>>>;\n\
         function run<Slot extends string>(slot: Slot): Chain<number> {\n\
         return slot;\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("Type 'Slot' is not assignable to type 'number'"),
        "the nested conditional chain must fully evaluate to number; got: {}",
        diag.message_text
    );
    assert!(
        has_related(&diag, "Type 'string' is not assignable to type 'number'"),
        "the constraint relation must reach the leaf; got: {:?}",
        related_messages(&diag)
    );
}

/// Object constraint: the chain drills through the offending property.
#[test]
fn object_constraint_chains_through_property() {
    let diag = ts2322(
        "function shape<Rec extends { value: number }>(rec: Rec): { value: string } {\n\
         return rec;\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("Type 'Rec' is not assignable to type '{ value: string; }'"),
        "headline keeps the parameter and target shape; got: {}",
        diag.message_text
    );
    assert!(
        has_related(&diag, "Types of property 'value' are incompatible"),
        "object constraint must drill into the incompatible property; got: {:?}",
        related_messages(&diag)
    );
    assert!(
        has_related(&diag, "Type 'number' is not assignable to type 'string'"),
        "the property leaf relation must be reached; got: {:?}",
        related_messages(&diag)
    );
}

/// Nested type-parameter constraints recurse: `U extends T extends string`
/// adds one chain level per constraint hop.
#[test]
fn nested_type_parameter_constraints_recurse() {
    let diag = ts2322(
        "function relay<Base extends string, Derived extends Base>(value: Derived): number {\n\
         return value;\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("Type 'Derived' is not assignable to type 'number'"),
        "headline keeps the leaf type parameter; got: {}",
        diag.message_text
    );
    assert!(
        has_related(&diag, "Type 'Base' is not assignable to type 'number'"),
        "the intermediate constraint hop must be chained; got: {:?}",
        related_messages(&diag)
    );
    assert!(
        has_related(&diag, "Type 'string' is not assignable to type 'number'"),
        "the final constraint must reach the primitive leaf; got: {:?}",
        related_messages(&diag)
    );
}

/// Negative control: an unconstrained type parameter has only an implicit
/// `unknown` constraint, for which `tsc` adds no elaboration line. The headline
/// stands alone.
#[test]
fn unconstrained_type_parameter_has_no_chain() {
    let diag = ts2322(
        "function raw<Free>(value: Free): number {\n\
         return value;\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("Type 'Free' is not assignable to type 'number'"),
        "headline is the bare parameter mismatch; got: {}",
        diag.message_text
    );
    assert!(
        related_messages(&diag).is_empty(),
        "an unconstrained parameter must not synthesize a constraint chain; got: {:?}",
        related_messages(&diag)
    );
}

/// Negative control: when the *target* is a bare type parameter, `tsc` reports
/// the `could be instantiated with an arbitrary type` caveat rather than a
/// source-constraint chain, so no `TypeParameterConstraintMismatch` line is
/// added on top of it.
#[test]
fn type_parameter_target_keeps_instantiation_caveat() {
    let diag = ts2322(
        "function cross<Src extends string, Dst extends number>(value: Src): Dst {\n\
         return value;\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("Type 'Src' is not assignable to type 'Dst'"),
        "headline relates the two parameters; got: {}",
        diag.message_text
    );
    assert!(
        has_related(&diag, "could be instantiated with an arbitrary type"),
        "a type-parameter target keeps tsc's instantiation caveat; got: {:?}",
        related_messages(&diag)
    );
    assert!(
        !has_related(&diag, "Type 'string' is not assignable to type 'Dst'"),
        "no source-constraint chain may be stacked on the caveat; got: {:?}",
        related_messages(&diag)
    );
}
