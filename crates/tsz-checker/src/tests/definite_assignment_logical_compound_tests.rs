//! Definite-assignment (TS2454) parity for logical compound assignments.
//!
//! Structural rule: a logical compound assignment (`x ??= rhs`, `x ||= rhs`,
//! `x &&= rhs`) treats its target `x` as definitely assigned. The implicit read
//! of the target is the conditioning test, not a use of an unassigned value, so
//! tsc does not report TS2454 at the assignment site. For `??=`/`||=` the target
//! holds a value on every continuation (the assigned `rhs` when the condition
//! selects it, otherwise the value the condition already proved present), so a
//! later read is also clean. For `&&=` the assignment runs only when `x` is
//! truthy and is skipped otherwise, so the assignment-site read is clean but a
//! later read on the skipped (falsy) path is still TS2454 — matching tsc.
//!
//! Arithmetic/bitwise compound assignments (`+=`, `**=`, ...) and `++`/`--` are
//! unaffected: their target read genuinely precedes the write, so tsc keeps
//! reporting TS2454 for an unassigned target there. Binder names are varied so
//! no identifier string drives the decision.

use crate::test_utils::check_source_strict_codes as check_strict;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn nullish_compound_assign_target_is_definitely_assigned() {
    // `casePart` reads the result of `lowerPart ??= ...`; both the assignment
    // target read and the later read are clean.
    let codes = check_strict(
        r#"
declare const part: string;
function f(): string {
  let lowerPart: string;
  const casePart = (lowerPart ??= part.toLowerCase());
  return casePart + lowerPart;
}
"#,
    );
    assert_eq!(
        count(&codes, 2454),
        0,
        "`??=` target must be definitely assigned, got codes: {codes:?}"
    );
}

#[test]
fn or_compound_assign_target_is_definitely_assigned() {
    let codes = check_strict(
        r#"
declare const seed: string;
function g(): string {
  let acc: string;
  const out = (acc ||= seed);
  return out + acc;
}
"#,
    );
    assert_eq!(
        count(&codes, 2454),
        0,
        "`||=` target must be definitely assigned, got codes: {codes:?}"
    );
}

#[test]
fn and_compound_assign_site_read_is_not_reported() {
    // `&&=` only conditionally assigns, but tsc still does NOT report TS2454 at
    // the assignment site itself (the target read is the conditioning test).
    let codes = check_strict(
        r#"
declare const tail: string;
function h(): void {
  let head: string;
  head &&= tail;
}
"#,
    );
    assert_eq!(
        count(&codes, 2454),
        0,
        "`&&=` site read must not report TS2454, got codes: {codes:?}"
    );
}

#[test]
fn and_compound_assign_later_read_still_reports() {
    // Negative control matching tsc: `&&=` assigns only on the truthy path, so a
    // read AFTER the assignment is still use-before-assigned on the skipped
    // (falsy) path. tsc reports exactly one TS2454 at the later read.
    let codes = check_strict(
        r#"
declare const tail: string;
function h2(): string {
  let head: string;
  head &&= tail;
  return head;
}
"#,
    );
    assert_eq!(
        count(&codes, 2454),
        1,
        "`&&=` later read must still report TS2454, got codes: {codes:?}"
    );
}

#[test]
fn nullish_then_later_read_is_clean() {
    let codes = check_strict(
        r#"
declare const fallback: number;
function k(): number {
  let total: number;
  total ??= fallback;
  return total;
}
"#,
    );
    assert_eq!(
        count(&codes, 2454),
        0,
        "read after `??=` must be clean, got codes: {codes:?}"
    );
}

#[test]
fn arithmetic_compound_assign_target_still_reports() {
    // Negative control: arithmetic compound assignment reads its target before
    // writing, so an unassigned target still reports TS2454 (tsc parity). tsc
    // reports at the `+=` site and at the following read.
    let codes = check_strict(
        r#"
function a(): number {
  let n: number;
  n += 1;
  return n;
}
"#,
    );
    assert_eq!(
        count(&codes, 2454),
        2,
        "arithmetic `+=` target must still report TS2454, got codes: {codes:?}"
    );
}

#[test]
fn plain_read_before_assignment_still_reports() {
    // Negative control: a bare read before any assignment is still TS2454, even
    // when a later logical compound assignment exists.
    let codes = check_strict(
        r#"
declare const v: string;
function b(): string {
  let s: string;
  const dup = s;
  s ??= v;
  return dup;
}
"#,
    );
    assert_eq!(
        count(&codes, 2454),
        1,
        "plain pre-assignment read must still report TS2454, got codes: {codes:?}"
    );
}
