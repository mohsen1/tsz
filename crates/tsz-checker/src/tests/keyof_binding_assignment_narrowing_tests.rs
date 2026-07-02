//! Regression tests: a binding declared with a `keyof T` type must narrow to
//! the assigned key literal, exactly as tsc's `getAssignmentReducedType` does.
//!
//! Structural rule: when a binding's declared type is a `keyof T` operator and
//! its assigned value is a key of `T`, tsc resolves `keyof T` to its concrete
//! key set before assignment reduction and narrows the binding to the assigned
//! key. tsz previously left `keyof T` as an unevaluated `KeyOf` operator, whose
//! keys are not enumerable as union members, so the assignment reduction bailed
//! and kept the whole `keyof T` type — surfacing a spurious TS2322 at any later
//! read that expected the narrowed key (e.g. reading a `keyof number[]` binding
//! initialized to `"length"` where a `string` is expected). tsz now owns this
//! through the assignment-reduction boundary
//! (`query_boundaries::flow_analysis::narrow_assignment`), evaluating the `keyof`
//! operator to its key union before reducing.
//!
//! The reduction only *adds* precision for `keyof` declared types that formerly
//! bailed, so genuinely invalid keys still report TS2322 and non-`keyof`
//! declarations keep their prior narrowing.

use crate::test_utils::check_source_strict_codes;

/// `let` binding: `keyof number[]` narrows to the assigned key literal, so a
/// later read into a `string` slot is accepted.
#[test]
fn let_binding_keyof_array_narrows_to_assigned_key() {
    let source = "\
type Arr = number[];
type K = keyof Arr;
let v: K = \"length\";
const w: string = v;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "`let v: keyof number[] = \"length\"` must narrow v to \"length\" (assignable \
         to string); expected no diagnostics, got: {codes:?}",
    );
}

/// The `keyof` operator applied inline (operand not behind an alias) narrows
/// identically — the operand-resolution path is not the trigger.
#[test]
fn let_binding_inline_keyof_array_narrows_to_assigned_key() {
    let source = "\
type K = keyof number[];
let v: K = \"length\";
const w: string = v;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "inline `keyof number[]` must narrow the same way; got: {codes:?}",
    );
}

/// `const` binding: same reduction path, so the initializer narrows too.
#[test]
fn const_binding_keyof_array_narrows_to_assigned_key() {
    let source = "\
type Arr = number[];
type K = keyof Arr;
const v: K = \"length\";
const w: string = v;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "`const v: keyof number[] = \"length\"` must narrow v to \"length\"; got: {codes:?}",
    );
}

/// `keyof` of an object type narrows to a single string-literal key.
#[test]
fn let_binding_keyof_object_narrows_to_assigned_key() {
    let source = "\
type O = { a: 1; b: 2 };
type K = keyof O;
let v: K = \"a\";
const w: \"a\" = v;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "`let v: keyof {{ a; b }} = \"a\"` must narrow v to \"a\"; got: {codes:?}",
    );
}

/// Return position uses the same assignment-reduction narrowing on the local.
#[test]
fn return_position_keyof_binding_narrows_to_assigned_key() {
    let source = "\
type Arr = number[];
type K = keyof Arr;
function f(): string {
  let v: K = \"length\";
  return v;
}
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "returning a `keyof number[]` local narrowed to \"length\" from a `string` \
         function must be accepted; got: {codes:?}",
    );
}

/// Reassignment (not just the initializer) narrows a `keyof`-typed `let`.
#[test]
fn reassignment_of_keyof_binding_narrows_to_assigned_key() {
    let source = "\
type O = { a: 1; b: 2 };
let v: keyof O;
v = \"a\";
const w: \"a\" = v;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "reassigning a `keyof` binding to a key literal must narrow it; got: {codes:?}",
    );
}

/// Renamed binders (no reliance on identifier spelling): the same shape with
/// different alias/binding names must behave identically.
#[test]
fn keyof_binding_narrowing_is_not_binder_name_specific() {
    let source = "\
type Collection = string[];
type Member = keyof Collection;
let selected: Member = \"length\";
const asText: string = selected;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "renamed binders must not change keyof-binding narrowing; got: {codes:?}",
    );
}

/// Control: a value that is *not* a key of `T` still reports TS2322 — the
/// reduction adds precision without hiding genuine errors.
#[test]
fn keyof_binding_rejects_non_key_literal() {
    let source = "\
type Arr = number[];
type K = keyof Arr;
const k: K = \"notamethod\";
";
    let codes = check_source_strict_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "assigning a non-key literal to a `keyof` binding must still report TS2322; \
         got: {codes:?}",
    );
}

/// Control: assigning a valid key was already accepted and must stay accepted.
#[test]
fn keyof_binding_accepts_valid_key_literal() {
    let source = "\
type Arr = number[];
type K = keyof Arr;
const k: K = \"push\";
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "assigning a valid key to a `keyof` binding must be accepted; got: {codes:?}",
    );
}

/// Control: a plain literal union (not a `keyof`) narrows exactly as before —
/// the new path is gated to `keyof` operators.
#[test]
fn plain_literal_union_binding_narrowing_unchanged() {
    let source = "\
let v: \"a\" | \"b\" | number = \"a\";
const w: \"a\" | \"b\" = v;
";
    let codes = check_source_strict_codes(source);
    assert!(
        codes.is_empty(),
        "plain literal-union narrowing must be unchanged; got: {codes:?}",
    );
}
