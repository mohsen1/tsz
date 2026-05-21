//! Post-loop narrowing for `do ... while` exit conditions.
//!
//! A `do ... while (cond)` statement reaches the following statement only when
//! `cond` is false, so the post-loop flow must apply the false branch of the
//! loop condition.

use tsz_common::options::checker::CheckerOptions;

fn diags(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn codes(source: &str) -> Vec<u32> {
    diags(source).iter().map(|diag| diag.code).collect()
}

#[test]
fn do_while_exit_applies_false_typeof_branch() {
    let codes = codes(
        r#"
function f(value: string | number) {
  do {
  } while (typeof value === "string");
  const narrowed: number = value;
}
"#,
    );

    assert!(
        !codes.contains(&2322),
        "expected do-while exit to narrow value to number; got {codes:?}"
    );
}

#[test]
fn do_while_exit_applies_false_null_branch_with_renamed_binding() {
    let codes = codes(
        r#"
function f(item: { n: number } | null) {
  do {
  } while (item === null);
  item.n;
}
"#,
    );

    assert!(
        !codes.contains(&18047) && !codes.contains(&2531),
        "expected do-while exit to narrow item away from null; got {codes:?}"
    );
}

#[test]
fn while_and_for_exit_controls_keep_false_branch_narrowing() {
    let codes = codes(
        r#"
function f(value: string | number, other: string | number) {
  while (typeof value === "string") {
  }
  const a: number = value;

  for (; typeof other === "string"; ) {
  }
  const b: number = other;
}
"#,
    );

    assert!(
        !codes.contains(&2322),
        "while and for exit narrowing controls should stay passing; got {codes:?}"
    );
}

#[test]
fn do_while_exit_does_not_apply_true_branch_after_normal_exit() {
    let codes = codes(
        r#"
function f(value: string | number) {
  do {
  } while (typeof value === "string");
  const wrong: string = value;
}
"#,
    );

    assert!(
        codes.contains(&2322),
        "normal do-while exit must not narrow to the true branch; got {codes:?}"
    );
}
