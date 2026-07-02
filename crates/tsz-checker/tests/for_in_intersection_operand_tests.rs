//! Regression tests for TS2407 on `for-in` operands that are object-type
//! intersections carrying a disjoint discriminant (issue #15389).
//!
//! `tsc`'s `checkForInStatement` validates the RIGHT-HAND SIDE without ever
//! routing it through `getReducedType`. So an object intersection whose members
//! share a disjoint discriminant — which reduces the intersection to `never`
//! under reduction — is still a valid for-in operand, because the unreduced
//! intersection is assignable to `object`. tsz previously reduced the operand
//! (via property-access resolution for generic-application members) before the
//! validity gate and emitted a false TS2407 rendered with `never`.
//!
//! Binder names are varied across cases so the fix cannot rely on any
//! user-chosen identifier.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2407: u32 = 2407;

fn assert_no_2407(source: &str, label: &str) {
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2407),
        "{label}: expected no TS2407 (tsc accepts the unreduced intersection operand), got codes: {codes:?}"
    );
}

fn assert_has_2407(source: &str, label: &str) {
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2407),
        "{label}: expected TS2407 for a non-object for-in operand, got codes: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// MUST BE CLEAN — object-type intersections (tsc accepts)
// ---------------------------------------------------------------------------

#[test]
fn generic_application_disjoint_discriminant_is_valid_for_in_operand() {
    // The primary witness from #15389.
    let source = r#"
type WithKind<K> = { kind: K };
declare const value: WithKind<'a'> & WithKind<'b'>;
for (const k in value) {}
"#;
    assert_no_2407(source, "generic-application disjoint discriminant");
}

#[test]
fn mixed_concrete_and_generic_disjoint_discriminant_is_valid() {
    let source = r#"
type Labelled<L> = { label: L };
declare const mixed: { label: 'x' } & Labelled<'y'>;
for (const key in mixed) {}
"#;
    assert_no_2407(source, "mixed concrete + generic disjoint discriminant");
}

#[test]
fn alias_wrapped_intersection_with_renamed_binders_is_valid() {
    let source = r#"
type Tagged<Marker> = { tag: Marker };
type Both = Tagged<'left'> & Tagged<'right'>;
declare const both: Both;
for (const entry in both) {}
"#;
    assert_no_2407(source, "alias-wrapped intersection, renamed binders");
}

#[test]
fn nested_alias_intersection_is_valid() {
    let source = r#"
type WithMode<M> = { mode: M };
declare const nested:
    (WithMode<'read'> & { payload: number }) & (WithMode<'write'> & { payload: number });
for (const field in nested) {}
"#;
    assert_no_2407(source, "nested alias intersection");
}

#[test]
fn disjoint_numeric_literal_discriminant_is_valid() {
    let source = r#"
type Slot<N> = { slot: N };
declare const slots: Slot<1> & Slot<2>;
for (const s in slots) {}
"#;
    assert_no_2407(source, "disjoint numeric literal discriminant");
}

#[test]
fn disjoint_boolean_literal_discriminant_is_valid() {
    let source = r#"
type Flagged<B> = { flag: B };
declare const flags: Flagged<true> & Flagged<false>;
for (const bit in flags) {}
"#;
    assert_no_2407(source, "disjoint boolean literal discriminant");
}

#[test]
fn object_intrinsic_intersected_with_type_parameter_stays_valid() {
    // `object & T` — a member (`object`) is already object-like.
    let source = r#"
function iterate<T extends object>(input: object & T): void {
    for (const prop in input) {}
}
iterate;
"#;
    assert_no_2407(source, "object & T");
}

#[test]
fn non_conflicting_application_intersection_stays_valid() {
    let source = r#"
type WithKind<K> = { kind: K };
declare const merged: WithKind<'a'> & { extra: number };
for (const k in merged) {}
"#;
    assert_no_2407(source, "non-conflicting application intersection");
}

#[test]
fn bare_type_parameter_operand_stays_valid() {
    let source = r#"
function loop<Elem>(input: Elem): void {
    for (const k in input) {}
}
loop;
"#;
    assert_no_2407(source, "bare type parameter operand");
}

#[test]
fn record_alias_operand_stays_valid() {
    let source = r#"
declare const table: Record<string, number>;
for (const key in table) {}
"#;
    assert_no_2407(source, "Record<string, number> alias operand");
}

// ---------------------------------------------------------------------------
// MUST ERROR — genuinely invalid for-in operands (tsc rejects)
// ---------------------------------------------------------------------------

#[test]
fn direct_never_operand_still_errors() {
    let source = r#"
declare const nothing: never;
for (const k in nothing) {}
"#;
    assert_has_2407(source, "direct never operand");
}

#[test]
fn disjoint_primitive_intersection_still_errors() {
    // `string & number` reduces to `never` and is NOT object-like — tsc errors.
    let source = r#"
declare const impossible: string & number;
for (const k in impossible) {}
"#;
    assert_has_2407(source, "string & number operand");
}

#[test]
fn primitive_string_operand_still_errors() {
    let source = r#"
declare const text: string;
for (const k in text) {}
"#;
    assert_has_2407(source, "string operand");
}

#[test]
fn primitive_number_operand_still_errors() {
    let source = r#"
declare const count: number;
for (const k in count) {}
"#;
    assert_has_2407(source, "number operand");
}
