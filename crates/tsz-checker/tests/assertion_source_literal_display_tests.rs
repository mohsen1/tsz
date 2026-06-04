//! A plain `expr as T` / `<T>expr` type assertion yields the asserted type `T`
//! as written. `tsc` treats that result as a *regular* (non-fresh) type and
//! preserves its literal element/property types in assignability diagnostics —
//! it does NOT widen them the way a fresh object/array literal expression is
//! widened.
//!
//! Before the fix, tsz routed assertion operands through the same fresh-literal
//! widening as bare literal expressions, so `[1, 2, 3] as [1, 2, 3]` rendered
//! as `[number, number, number]` and `{ a: 1 } as { a: 1 }` as `{ a: number; }`
//! in TS2322/TS2345 messages. Declared references and `as const` were already
//! preserved; only the assertion operand diverged.
//!
//! Negative controls confirm the gate is scoped to assertion operands: a fresh
//! array literal still widens to `number[]`, and `as const` still renders a
//! `readonly` literal surface. Binder names, literal values, and target types
//! are varied across cases so the behavior is proven structural, not keyed on a
//! fixture identifier.

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

fn message_for(diags: &[Diagnostic], code: u32) -> String {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert!(
        !matches.is_empty(),
        "expected a TS{code} diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0].message_text.clone()
}

/// `expr as [literal tuple]` assigned to a scalar preserves the tuple's literal
/// element types in the TS2322 source display.
#[test]
fn tuple_assertion_source_preserves_literals_against_scalar_target() {
    let diags = check_strict("const first: 0 = ([1, 2, 3] as [1, 2, 3]);\n");
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type '[1, 2, 3]' is not assignable to type '0'"),
        "assertion tuple source must preserve literals; got: {msg}"
    );
    assert!(
        !msg.contains("[number, number, number]"),
        "assertion tuple source must not widen literal elements; got: {msg}"
    );
}

/// `<[literal tuple]>expr` (angle-bracket form) behaves the same.
#[test]
fn angle_bracket_tuple_assertion_source_preserves_literals() {
    let diags = check_strict("const seq: 7 = (<[8, 9]>(null as any));\n");
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type '[8, 9]' is not assignable to type '7'"),
        "angle-bracket assertion must preserve literals; got: {msg}"
    );
}

/// `expr as { ...literal props }` preserves literal property types.
#[test]
fn object_assertion_source_preserves_literal_properties() {
    let diags = check_strict("const box: 0 = ({ width: 4 } as { width: 4 });\n");
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("width: 4"),
        "assertion object source must preserve the literal property type; got: {msg}"
    );
    assert!(
        !msg.contains("width: number"),
        "assertion object source must not widen the literal property; got: {msg}"
    );
}

/// Assertion argument operands preserve literals in TS2345 just like the
/// assignment path.
#[test]
fn tuple_assertion_argument_preserves_literals() {
    let diags = check_strict(
        "declare function take(value: 0): void;\n\
         take([5, 6] as [5, 6]);\n",
    );
    let msg = message_for(&diags, 2345);
    assert!(
        msg.contains("Argument of type '[5, 6]'"),
        "assertion argument must preserve literals; got: {msg}"
    );
    assert!(
        !msg.contains("[number, number]"),
        "assertion argument must not widen literal elements; got: {msg}"
    );
}

/// Negative control: a fresh array literal (no assertion) still widens to its
/// inferred array type, matching `tsc`.
#[test]
fn fresh_array_literal_source_still_widens() {
    let diags = check_strict("const items: 0 = [1, 2, 3];\n");
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("number[]"),
        "fresh array literal must still widen to number[]; got: {msg}"
    );
}

/// Negative control: `as const` keeps its `readonly` literal surface (its own
/// path), unaffected by the plain-assertion gate.
#[test]
fn const_assertion_source_keeps_readonly_literal_surface() {
    let diags = check_strict("const pair: 0 = ([10, 11] as const);\n");
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("readonly [10, 11]"),
        "as const must keep its readonly literal surface; got: {msg}"
    );
}

/// Negative control: a declared reference of a literal tuple type was already
/// preserved and must stay preserved.
#[test]
fn declared_reference_source_preserves_literals() {
    let diags = check_strict(
        "declare const triple: [1, 2, 3];\n\
         const out: 0 = triple;\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("Type '[1, 2, 3]' is not assignable to type '0'"),
        "declared literal-tuple reference must preserve literals; got: {msg}"
    );
}
