//! Enum residual narrowing after control-flow exclusion.
//!
//! When flow excludes all but one enum member, the remaining type should be
//! the surviving enum member, not the original enum domain.

use tsz_common::options::checker::CheckerOptions;

fn diags(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn if_chain_excluding_numeric_enum_members_leaves_single_member() {
    let diags = diags(
        r#"
enum E { A, B, C }
declare const e: E;
if (e !== E.A && e !== E.B) {
  const x: E.C = e;
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2322),
        "expected e to narrow to the remaining enum member E.C; got: {diags:?}"
    );
}

#[test]
fn switch_default_excluding_numeric_enum_members_leaves_single_member() {
    let diags = diags(
        r#"
enum State { Start, Middle, End }
declare const state: State;
switch (state) {
  case State.Start:
    break;
  case State.Middle:
    break;
  default:
    const x: State.End = state;
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2322),
        "expected switch default to narrow state to State.End; got: {diags:?}"
    );
}

#[test]
fn if_chain_excluding_string_enum_members_leaves_single_member() {
    let diags = diags(
        r#"
enum Choice { Red = "red", Blue = "blue", Green = "green" }
declare const choice: Choice;
if (choice !== Choice.Red && choice !== Choice.Blue) {
  const x: Choice.Green = choice;
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2322),
        "expected choice to narrow to Choice.Green; got: {diags:?}"
    );
}

#[test]
fn excluding_all_enum_members_still_reaches_never() {
    let diags = diags(
        r#"
enum Flag { Off, On }
declare const flag: Flag;
if (flag !== Flag.Off && flag !== Flag.On) {
  const x: never = flag;
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2322),
        "expected excluding all enum members to narrow to never; got: {diags:?}"
    );
}
