//! Regression coverage for `statement_falls_through`'s `IF_STATEMENT` arm
//! when there is no `else` clause and the condition is a compile-time
//! constant. tsc treats `if (true) { <terminator> }` with no `else` as
//! having no implicit fall-through path (the `then` branch always runs), the
//! same way `while (true)` with no reachable `break` is a non-completing
//! loop. Before this fix, a missing `else` unconditionally made the whole
//! `if` "fall through" regardless of the condition, so code after
//! `if (true) { throw ...; }` was never flagged unreachable (TS7027).

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn unreachable_codes(source: &str) -> Vec<u32> {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            allow_unreachable_code: Some(false),
            ..CheckerOptions::default()
        },
    );
    diagnostics.iter().map(|diag| diag.code).collect()
}

#[test]
fn if_true_throw_no_else_reports_unreachable_after() {
    let codes = unreachable_codes(
        r#"
function f() {
    if (true) {
        throw new Error("x");
    }
    console.log("dead");
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true) {{ throw }}` with no else, got {codes:?}"
    );
}

#[test]
fn if_true_return_no_else_reports_unreachable_after() {
    let codes = unreachable_codes(
        r#"
function f(): number {
    if (true) {
        return 1;
    }
    console.log("dead");
    return 2;
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true) {{ return }}` with no else, got {codes:?}"
    );
}

#[test]
fn if_true_and_true_throw_no_else_reports_unreachable_after() {
    // Constant-folds through `&&`, matching `is_true_condition`'s existing
    // boolean-operator handling used elsewhere (e.g. loop conditions).
    let codes = unreachable_codes(
        r#"
function f() {
    if (true && true) {
        throw new Error("x");
    }
    console.log("dead");
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true && true) {{ throw }}` with no else, got {codes:?}"
    );
}

#[test]
fn if_true_non_terminating_body_no_else_does_not_report_unreachable() {
    // The `then` branch itself falls through (no throw/return/never-call), so
    // even though the condition is always true, code after the `if` is
    // reachable. This guards against over-correcting to "always unreachable"
    // whenever the condition folds to `true`.
    let codes = unreachable_codes(
        r#"
function f() {
    if (true) {
        console.log("taken");
    }
    console.log("also reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "did not expect TS7027 when the `then` branch falls through, got {codes:?}"
    );
}

#[test]
fn if_false_throw_no_else_does_not_report_unreachable() {
    // Negative control: a constant-FALSE condition with no else always falls
    // through unconditionally (the `then` branch never executes), regardless
    // of whether that branch would itself terminate.
    let codes = unreachable_codes(
        r#"
function f() {
    if (false) {
        throw new Error("x");
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "did not expect TS7027 after `if (false) {{ throw }}`, got {codes:?}"
    );
}

#[test]
fn if_non_constant_condition_throw_no_else_does_not_report_unreachable() {
    // Negative control: a non-constant condition with no else keeps its
    // existing (pre-fix) behavior — the missing else is always a possible
    // skip path, so code after the `if` is reachable.
    let codes = unreachable_codes(
        r#"
function f(cond: boolean) {
    if (cond) {
        throw new Error("x");
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "did not expect TS7027 after `if (cond) {{ throw }}` with a non-constant condition, got {codes:?}"
    );
}

#[test]
fn if_true_throw_inside_renamed_arrow_reports_unreachable_after() {
    // Adjacent case: same shape inside a differently-named arrow function
    // assigned to a `const`, to guard against any accidental name-based
    // suppression.
    let codes = unreachable_codes(
        r#"
const guardAlways = () => {
    if (true) {
        throw new Error("x");
    }
    const neverRuns = 1;
    console.log(neverRuns);
};
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true) {{ throw }}` inside a renamed arrow function, got {codes:?}"
    );
}
