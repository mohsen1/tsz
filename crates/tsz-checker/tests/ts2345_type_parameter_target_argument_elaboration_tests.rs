//! A call argument that fails to relate to a bare type-parameter parameter must
//! carry the same instantiation caveat `tsc` attaches to the equivalent TS2322
//! assignment/return surface. When the parameter type is a bare type parameter
//! `T`, the argument cannot be proven assignable because `T` could still be
//! instantiated with a narrower (TS5075) or an arbitrary (TS5082) type, so `tsc`
//! nests that note directly beneath the `Argument of type ... is not assignable
//! to parameter of type 'T'.` head.
//!
//! The TS2322 renderer already owns this note through
//! `unrelated_type_parameter_target_related_info`; the TS2345 argument surface
//! reaches a different emitter (`error_argument_not_assignable_at_impl`), so it
//! attaches the same note there (#17448 / #17449).
//!
//! Binder and type-parameter names are varied across cases so the behavior is
//! proven structural rather than keyed on any identifier, and the argument value
//! kinds (`null`, primitives, fresh object literals) and constraint shapes
//! (absent, primitive, object) are varied to exercise both the TS5075 and TS5082
//! branches.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2345(source: &str) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2345)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS2345 diagnostic, got {diagnostics:#?}"
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

/// Unconstrained type-parameter target, `null` argument: `tsc` reports the
/// TS5082 caveat (the parameter could be instantiated with an arbitrary type
/// unrelated to `null`). The explicit type argument `Elem` fixes the parameter
/// to a bare type parameter from the enclosing scope.
#[test]
fn unconstrained_target_null_argument_keeps_arbitrary_caveat() {
    let diag = ts2345(
        "declare function accept<Slot>(value: Slot): void;\n\
         function outer<Elem>(seed: Elem) { accept<Elem>(null); }\n",
    );
    assert!(
        diag.message_text
            .contains("Argument of type 'null' is not assignable to parameter of type 'Elem'"),
        "headline keeps the bare type-parameter target; got: {}",
        diag.message_text
    );
    assert!(
        has_related(
            &diag,
            "'Elem' could be instantiated with an arbitrary type which could be unrelated to 'null'",
        ),
        "a bare type-parameter target keeps tsc's TS5082 caveat; got: {:?}",
        related_messages(&diag)
    );
}

/// A primitive argument that is unrelated to the target parameter's constraint
/// still takes the TS5082 branch: `number` is not assignable to the `string`
/// constraint, so the parameter could be an arbitrary unrelated type.
#[test]
fn constrained_target_unrelated_primitive_keeps_arbitrary_caveat() {
    let diag = ts2345(
        "declare function store<Cell extends string>(value: Cell): void;\n\
         function scope<Item extends string>(seed: Item) { store<Item>(42); }\n",
    );
    assert!(
        diag.message_text
            .contains("Argument of type 'number' is not assignable to parameter of type 'Item'"),
        "headline keeps the bare type-parameter target; got: {}",
        diag.message_text
    );
    assert!(
        has_related(
            &diag,
            "'Item' could be instantiated with an arbitrary type which could be unrelated to 'number'",
        ),
        "an unrelated primitive argument keeps the TS5082 caveat; got: {:?}",
        related_messages(&diag)
    );
}

/// The constraint-satisfied branch (TS5075): a fresh object literal that *is*
/// assignable to the target parameter's object constraint still fails the
/// relation because the parameter could be instantiated with a narrower subtype
/// of that constraint. `tsc` reports the "is assignable to the constraint of
/// type ... but ... a different subtype of constraint" note.
#[test]
fn constraint_satisfied_object_argument_reports_subtype_caveat() {
    let diag = ts2345(
        "declare function push<Node extends { a: number }>(value: Node): void;\n\
         function region<Cursor extends { a: number }>(seed: Cursor) {\n\
         const wide: { a: number; b: number } = { a: 1, b: 2 };\n\
         push<Cursor>(wide);\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("is not assignable to parameter of type 'Cursor'"),
        "headline keeps the bare type-parameter target; got: {}",
        diag.message_text
    );
    assert!(
        has_related(&diag, "is assignable to the constraint of type 'Cursor'")
            && has_related(
                &diag,
                "could be instantiated with a different subtype of constraint",
            ),
        "a constraint-satisfied argument takes the TS5075 subtype caveat; got: {:?}",
        related_messages(&diag)
    );
}

/// A `null` argument against an unconstrained target still takes the TS5082
/// branch — the constraint defaults to `unknown`, which `null` satisfies, yet an
/// unconstrained parameter reports the arbitrary-type caveat rather than the
/// subtype caveat. Distinct binder names keep the assertion structural.
#[test]
fn unconstrained_target_number_argument_keeps_arbitrary_caveat() {
    let diag = ts2345(
        "declare function feed<Bucket>(value: Bucket): void;\n\
         function region<Token>(seed: Token) { feed<Token>(42); }\n",
    );
    assert!(
        diag.message_text
            .contains("Argument of type 'number' is not assignable to parameter of type 'Token'"),
        "headline keeps the bare type-parameter target; got: {}",
        diag.message_text
    );
    assert!(
        has_related(
            &diag,
            "'Token' could be instantiated with an arbitrary type which could be unrelated to 'number'",
        ),
        "an unconstrained target reports the TS5082 arbitrary caveat; got: {:?}",
        related_messages(&diag)
    );
}

/// Guard: a target that merely *contains* a free type parameter (`{ v: T }`) is
/// not a bare type-parameter target, so the caveat owner must stay silent — the
/// note is specific to a bare `T` target, mirroring the TS2322 surface. (tsc
/// attaches the caveat only at the nested bare-`T` leaf of the property chain,
/// which this emitter does not reach; the invariant asserted here is that no
/// top-level caveat is fabricated for a non-bare target.)
#[test]
fn free_type_parameter_object_target_gets_no_top_level_caveat() {
    let diag = ts2345(
        "declare function stash<Wrap>(value: { v: Wrap }): void;\n\
         function outer<Slot>(seed: Slot) {\n\
         const wide: { v: number } = { v: 1 };\n\
         stash<Slot>(wide);\n\
         }\n",
    );
    assert!(
        diag.message_text
            .contains("is not assignable to parameter of type '{ v: Slot; }'"),
        "headline keeps the object target that contains the free parameter; got: {}",
        diag.message_text
    );
    assert!(
        !has_related(&diag, "could be instantiated with an arbitrary type")
            && !has_related(&diag, "is assignable to the constraint of type"),
        "a non-bare free-type-parameter target must not fabricate a top-level caveat; got: {:?}",
        related_messages(&diag)
    );
}
