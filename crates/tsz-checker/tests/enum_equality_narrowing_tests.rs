//! Manual tests for enum equality narrowing (`===`/`!==`).
//!
//! Issue #9684: a value typed as a whole enum (`e: E`) must narrow to the
//! matching member literal type after a successful `e === E.A` check, and
//! must narrow to the union of remaining members after a successful
//! `e !== E.A` check. The structural rule is that for control-flow purposes
//! a whole-enum value is treated as the union of its member-typed values
//! (matching tsc's `getBaseTypeOfEnumType`), so equality narrowing remaps the
//! surviving inner literals back to their corresponding member-typed enums.
//!
//! The tests intentionally cover renamed members, heterogeneous enums,
//! negative numeric enums, and the no-op narrowing case so a fix that only
//! special-cases one spelling fails the suite.

use tsz_checker::test_utils::check_source_diagnostics;

fn assert_clean(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {diagnostics:#?}"
    );
}

#[test]
fn equality_narrow_two_member_numeric_enum_if_else() {
    let source = r"
enum E { A, B }
declare const e: E;
if (e === E.A) {
  const x: E.A = e;
} else {
  const y: E.B = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_three_member_enum_exclusion_else_branch() {
    let source = r"
enum E { A, B, C }
declare const e: E;
if (e === E.A) {
  const x: E.A = e;
} else {
  const y: E.B | E.C = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_heterogeneous_enum() {
    let source = r#"
enum H { A = 0, B = "b" }
declare const h: H;
if (h === H.A) {
  const ha: H.A = h;
} else {
  const hb: H.B = h;
}
"#;
    assert_clean(source);
}

#[test]
fn equality_narrow_negative_value_enum() {
    let source = r"
enum N { A = -1, B = -2 }
declare const n: N;
if (n === N.A) {
  const na: N.A = n;
} else {
  const nb: N.B = n;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_renamed_members_is_position_independent() {
    // Rename the enum and members; behavior must not depend on the spellings.
    let source = r"
enum Renamed { Foo, Bar }
declare const r: Renamed;
if (r === Renamed.Foo) {
  const rf: Renamed.Foo = r;
} else {
  const rb: Renamed.Bar = r;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_already_member_is_no_op() {
    let source = r"
enum E { A, B }
declare const ea: E.A;
if (ea === E.A) {
  const x: E.A = ea;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_inequality_two_member_else() {
    let source = r"
enum E { A, B }
declare const e: E;
if (e !== E.A) {
  const y: E.B = e;
} else {
  const x: E.A = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_union_predicate_multiple_members() {
    let source = r"
enum E { A, B, C }
declare const m: E;
if (m === E.A || m === E.B) {
  const x: E.A | E.B = m;
} else {
  const y: E.C = m;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_switch_case_collects_remaining_members_in_default() {
    let source = r"
enum E { A, B, C }
function f(e: E) {
  switch (e) {
    case E.A: {
      const a: E.A = e;
      return;
    }
    case E.B: {
      const b: E.B = e;
      return;
    }
    default: {
      const c: E.C = e;
    }
  }
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_with_literal_value_maps_to_corresponding_member() {
    // `e === 0` for `e: E` narrows `e` to the member whose value is `0`.
    let source = r"
enum E { A, B, C }
declare const e: E;
if (e === 0) {
  const x: E.A = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_preserves_whole_enum_assignment_after_narrow() {
    // After narrowing to a member, the value is still assignable to the
    // whole enum (member <: parent).
    let source = r"
enum E { A, B }
declare const e: E;
if (e === E.A) {
  const x: E = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_does_not_leak_across_unrelated_enums() {
    // The narrowing domain check must use parent enum identity, so
    // narrowing `e: E` by a member of an unrelated enum `F` must not refine
    // `e` (and the assignment to `E.A` must fail).
    let source = r"
enum E { A, B }
enum F { A, B }
declare const e: E;
declare const ok: boolean;
if (ok) {
  const x: E.A = e;
}
";
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322, 1,
        "Expected one TS2322 for unrelated-enum assignment: {diagnostics:?}",
    );
}
