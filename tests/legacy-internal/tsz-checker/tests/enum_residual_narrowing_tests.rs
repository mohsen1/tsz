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

#[test]
fn positive_enum_equality_on_single_object_keeps_receiver() {
    // `e.tag === Mode.Run` (true branch) on a single (non-union) object whose
    // discriminant property is the full enum must keep `e`, not collapse it to
    // `never`. The narrowing subtype probe runs against the env resolver so the
    // enum member-to-parent relation (`Mode.Run <: Mode`) is visible.
    let diags = diags(
        r#"
enum Mode { Idle, Run, Stop }
function run(e: { tag: Mode; act(): number }): void {
  if (e.tag === Mode.Run) {
    e.act();
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2339),
        "expected `e` to survive the positive enum-equality guard; got: {diags:?}"
    );
}

#[test]
fn positive_string_enum_equality_on_single_object_keeps_receiver() {
    // Binder names deliberately differ from the numeric case to prove the rule
    // is structural (enum member-to-parent), not name-driven.
    let diags = diags(
        r#"
enum Colour { Red = "r", Blue = "b" }
function paint(brush: { hue: Colour; stroke(): void }): void {
  if (brush.hue === Colour.Blue) {
    brush.stroke();
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2339),
        "expected `brush` to survive the positive string-enum guard; got: {diags:?}"
    );
}

#[test]
fn positive_enum_equality_on_class_this_keeps_receiver() {
    let diags = diags(
        r#"
enum Phase { Begin, Middle, End }
class Worker {
  constructor(private phase: Phase) {}
  step(): void {
    if (this.phase === Phase.Begin) {
      this.step();
    }
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2339),
        "expected `this` to survive the positive enum guard inside a method; got: {diags:?}"
    );
}

#[test]
fn switch_case_enum_on_single_object_keeps_receiver() {
    let diags = diags(
        r#"
enum Signal { Lo, Hi }
function emit(port: { level: Signal; fire(): void }): void {
  switch (port.level) {
    case Signal.Hi:
      port.fire();
      break;
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2339),
        "expected `port` to survive the positive switch-case enum guard; got: {diags:?}"
    );
}

#[test]
fn positive_enum_discriminated_union_still_narrows_to_member() {
    // The fix must not over-keep: a genuine discriminated union narrows to the
    // matching member on the positive branch (the `b` member's property is gone
    // in the true branch of `u.kind === Kind.A`).
    let diags = diags(
        r#"
enum Kind { A, B }
type U = { kind: Kind.A; a: number } | { kind: Kind.B; b: string };
function f(u: U): void {
  if (u.kind === Kind.A) {
    u.a;
    // @ts-expect-error - `b` only exists on the Kind.B member
    u.b;
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2578),
        "expected discriminated union to still narrow to the Kind.A member; got: {diags:?}"
    );
}
