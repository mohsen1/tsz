//! Regression tests for TS2454 (used-before-assigned) when the declared type
//! carries `undefined` behind an *unevaluated* indexed access.
//!
//! Structural rule (owner: `flow/flow_analysis/usage.rs`,
//! `skip_definite_assignment_for_type`): tsc suppresses TS2454 when the declared
//! type includes `undefined`. When a union member is an indexed access into an
//! optional property (`W['opt']` with `opt?`), it resolves to `{ … } | undefined`,
//! so the variable may legitimately be unassigned. `type_contains_undefined` does
//! not look through an `IndexAccess`, so the check now evaluates the declared
//! type's resolved form and re-checks — matching tsc. A required/non-undefined
//! member still draws TS2454.

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    let mut c = check_source_codes(source);
    c.sort_unstable();
    c.dedup();
    c
}

#[test]
fn indexed_access_optional_member_suppresses_ts2454() {
    assert!(
        !codes(
            r#"
interface W { opt?: { connect(): void }; }
declare const w: W;
function f() {
  let x: W["opt"] | false;
  try { x = (true as boolean) && w.opt; } catch {}
  if (!x) return;
  return x;
}
"#,
        )
        .contains(&2454),
        "an indexed access into an optional property carries undefined; TS2454 must be suppressed",
    );
}

#[test]
fn indexed_access_optional_member_direct_union_suppresses_ts2454() {
    // The indexed access is the sole declared type (no `| false` companion).
    assert!(
        !codes(
            r#"
interface W { opt?: { connect(): void }; }
function f() {
  let x: W["opt"];
  if (!x) return;
  return x;
}
"#,
        )
        .contains(&2454),
        "a bare indexed access into an optional property carries undefined",
    );
}

#[test]
fn indexed_access_optional_member_renamed_binders() {
    assert!(
        !codes(
            r#"
interface Bag { handler?: () => void; }
declare const bag: Bag;
function run() {
  let cb: Bag["handler"] | false;
  cb = (true as boolean) && bag.handler;
  if (!cb) return;
  return cb;
}
"#,
        )
        .contains(&2454),
        "not keyed on binder names",
    );
}

#[test]
fn indexed_access_required_member_still_reports_ts2454() {
    // Negative control: a required property's indexed access has no undefined,
    // so a used-before-assigned variable must still draw TS2454.
    assert!(
        codes(
            r#"
interface W { req: { connect(): void }; }
function f() {
  let x: W["req"];
  if (!x) return;
  return x;
}
"#,
        )
        .contains(&2454),
        "required-property indexed access has no undefined; TS2454 must still fire",
    );
}

#[test]
fn plain_uninitialized_still_reports_ts2454() {
    // Negative control: a plain non-undefined type still reports TS2454.
    assert!(
        codes(
            r#"
function f() {
  let x: number;
  if (Math.random() > 0.5) x = 1;
  return x;
}
"#,
        )
        .contains(&2454),
        "plain uninitialized number must still draw TS2454",
    );
}
